use crate::{models::Account, storage::Database, proxy::ProxyServer, AppState};
use tauri::State;
use std::path::PathBuf;
use std::fs;

#[tauri::command]
pub async fn create_account(
    name: String,
    base_url: String,
    api_key: String,
    state: State<'_, AppState>,
) -> Result<Account, String> {
    let account = Account::new(name, base_url);

    state
        .db
        .create_account(account.clone(), api_key)
        .await
        .map_err(|e| e.to_string())?;

    // 첫 번째 계정이면 자동 활성화
    let accounts = state.db.get_accounts().await.map_err(|e| e.to_string())?;
    if accounts.len() == 1 {
        state
            .db
            .switch_account(&account.id)
            .await
            .map_err(|e| e.to_string())?;
    }

    Ok(account)
}

#[tauri::command]
pub async fn get_accounts(state: State<'_, AppState>) -> Result<Vec<Account>, String> {
    state.db.get_accounts().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_active_account(
    state: State<'_, AppState>,
) -> Result<Option<Account>, String> {
    state
        .db
        .get_active_account()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn switch_account(
    account_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .db
        .switch_account(&account_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_account(
    account_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .db
        .delete_account(&account_id)
        .await
        .map_err(|e| e.to_string())
}

#[derive(serde::Serialize)]
pub struct ProxyStatus {
    pub running: bool,
    pub port: u16,
}

#[tauri::command]
pub async fn start_proxy(port: u16, state: State<'_, AppState>) -> Result<(), String> {
    let mut proxy = state.proxy.write().await;

    if proxy.is_some() {
        return Err("Proxy is already running".to_string());
    }

    let mut server = ProxyServer::new(state.db.clone());
    server.start(port).await.map_err(|e| e.to_string())?;
    *proxy = Some(server);

    Ok(())
}

#[tauri::command]
pub async fn stop_proxy(state: State<'_, AppState>) -> Result<(), String> {
    let mut proxy = state.proxy.write().await;

    if let Some(mut server) = proxy.take() {
        server.stop().await.map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[tauri::command]
pub async fn get_proxy_status(state: State<'_, AppState>) -> Result<ProxyStatus, String> {
    let proxy = state.proxy.read().await;
    Ok(ProxyStatus {
        running: proxy.is_some(),
        port: 8080,
    })
}

// Claude 설정 백업/복원 기능
#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct BackupInfo {
    pub filename: String,
    pub timestamp: i64,
    pub size: u64,
}

fn get_claude_settings_path() -> Result<PathBuf, String> {
    let appdata = std::env::var("APPDATA")
        .or_else(|_| std::env::var("HOME").map(|h| format!("{}/Library/Application Support", h)))
        .map_err(|e| format!("Failed to get app data directory: {}", e))?;

    Ok(PathBuf::from(appdata).join("Claude").join("settings.json"))
}

fn get_backup_dir() -> Result<PathBuf, String> {
    let appdata = std::env::var("APPDATA")
        .or_else(|_| std::env::var("HOME").map(|h| format!("{}/.config", h)))
        .map_err(|e| format!("Failed to get app data directory: {}", e))?;

    Ok(PathBuf::from(appdata).join("com.swiftcast.app").join("backups"))
}

#[tauri::command]
pub async fn backup_claude_settings() -> Result<BackupInfo, String> {
    let settings_path = get_claude_settings_path()?;

    if !settings_path.exists() {
        return Err("Claude settings.json not found".to_string());
    }

    let backup_dir = get_backup_dir()?;
    fs::create_dir_all(&backup_dir)
        .map_err(|e| format!("Failed to create backup directory: {}", e))?;

    let timestamp = chrono::Utc::now().timestamp();
    let filename = format!("settings_backup_{}.json", timestamp);
    let backup_path = backup_dir.join(&filename);

    fs::copy(&settings_path, &backup_path)
        .map_err(|e| format!("Failed to backup settings: {}", e))?;

    let metadata = fs::metadata(&backup_path)
        .map_err(|e| format!("Failed to get backup file metadata: {}", e))?;

    Ok(BackupInfo {
        filename,
        timestamp,
        size: metadata.len(),
    })
}

#[tauri::command]
pub async fn restore_claude_settings(backup_filename: String) -> Result<(), String> {
    let backup_dir = get_backup_dir()?;
    let backup_path = backup_dir.join(&backup_filename);

    if !backup_path.exists() {
        return Err(format!("Backup file not found: {}", backup_filename));
    }

    let settings_path = get_claude_settings_path()?;

    // Claude 설정 디렉토리가 없으면 생성
    if let Some(parent) = settings_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create Claude settings directory: {}", e))?;
    }

    fs::copy(&backup_path, &settings_path)
        .map_err(|e| format!("Failed to restore settings: {}", e))?;

    Ok(())
}

#[tauri::command]
pub async fn list_backups() -> Result<Vec<BackupInfo>, String> {
    let backup_dir = get_backup_dir()?;

    if !backup_dir.exists() {
        return Ok(vec![]);
    }

    let entries = fs::read_dir(&backup_dir)
        .map_err(|e| format!("Failed to read backup directory: {}", e))?;

    let mut backups = Vec::new();

    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
        let path = entry.path();

        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("json") {
            let filename = path.file_name()
                .and_then(|s| s.to_str())
                .ok_or("Invalid filename")?
                .to_string();

            // 파일명에서 타임스탬프 추출: settings_backup_{timestamp}.json
            let timestamp = if let Some(ts_str) = filename.strip_prefix("settings_backup_")
                .and_then(|s| s.strip_suffix(".json")) {
                ts_str.parse::<i64>().unwrap_or(0)
            } else {
                0
            };

            let metadata = fs::metadata(&path)
                .map_err(|e| format!("Failed to get file metadata: {}", e))?;

            backups.push(BackupInfo {
                filename,
                timestamp,
                size: metadata.len(),
            });
        }
    }

    // 최신 순으로 정렬
    backups.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    Ok(backups)
}

#[tauri::command]
pub async fn delete_backup(backup_filename: String) -> Result<(), String> {
    let backup_dir = get_backup_dir()?;
    let backup_path = backup_dir.join(&backup_filename);

    if !backup_path.exists() {
        return Err(format!("Backup file not found: {}", backup_filename));
    }

    fs::remove_file(&backup_path)
        .map_err(|e| format!("Failed to delete backup: {}", e))?;

    Ok(())
}
