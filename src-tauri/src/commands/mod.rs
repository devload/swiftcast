use crate::{models::Account, proxy::ProxyServer, AppState};
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
    // 계정 전환
    state
        .db
        .switch_account(&account_id)
        .await
        .map_err(|e| e.to_string())?;

    // 활성화된 계정 정보 가져오기
    let account = state
        .db
        .get_active_account()
        .await
        .map_err(|e| e.to_string())?
        .ok_or("Account not found after switch".to_string())?;

    // 프록시 포트 가져오기
    let proxy_port = state
        .db
        .get_proxy_port()
        .await
        .map_err(|e| e.to_string())?;

    // Claude settings.json 업데이트
    update_claude_settings(&account.base_url, proxy_port)?;

    Ok(())
}

// Claude settings.json 업데이트
fn update_claude_settings(base_url: &str, proxy_port: u16) -> Result<(), String> {
    use std::fs;

    let settings_path = get_claude_settings_path()?;

    // Anthropic 공식 API인 경우 settings.json 삭제 (기본 동작으로 복원)
    if base_url.contains("api.anthropic.com") {
        if settings_path.exists() {
            // env 섹션만 제거하고 나머지는 유지
            if let Ok(content) = fs::read_to_string(&settings_path) {
                if let Ok(mut settings) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(obj) = settings.as_object_mut() {
                        // ANTHROPIC_BASE_URL과 ANTHROPIC_AUTH_TOKEN만 제거
                        if let Some(env) = obj.get_mut("env").and_then(|e| e.as_object_mut()) {
                            env.remove("ANTHROPIC_BASE_URL");
                            env.remove("ANTHROPIC_AUTH_TOKEN");

                            // env가 비어있으면 env 자체를 제거
                            if env.is_empty() {
                                obj.remove("env");
                            }
                        }

                        // 파일이 비어있으면 삭제, 아니면 업데이트
                        if obj.is_empty() {
                            fs::remove_file(&settings_path)
                                .map_err(|e| format!("Failed to delete settings.json: {}", e))?;
                            tracing::info!("Deleted Claude settings.json for direct Anthropic connection");
                        } else {
                            let settings_json = serde_json::to_string_pretty(&settings)
                                .map_err(|e| format!("Failed to serialize settings: {}", e))?;
                            fs::write(&settings_path, settings_json)
                                .map_err(|e| format!("Failed to write settings.json: {}", e))?;
                            tracing::info!("Removed proxy settings from settings.json");
                        }
                        return Ok(());
                    }
                }
            }
            // 파싱 실패시 파일 삭제
            fs::remove_file(&settings_path)
                .map_err(|e| format!("Failed to delete settings.json: {}", e))?;
            tracing::info!("Deleted Claude settings.json for direct Anthropic connection");
        }
        return Ok(());
    }

    // 프록시 사용시 settings.json에 로컬 프록시 URL 설정
    let proxy_url = format!("http://localhost:{}", proxy_port);

    let mut settings: serde_json::Value = if settings_path.exists() {
        let content = fs::read_to_string(&settings_path)
            .map_err(|e| format!("Failed to read settings.json: {}", e))?;
        serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse settings.json: {}", e))?
    } else {
        serde_json::json!({})
    };

    // env 객체가 없으면 생성
    if !settings.as_object().map(|o| o.contains_key("env")).unwrap_or(false) {
        settings["env"] = serde_json::json!({});
    }

    // BASE_URL을 로컬 프록시로 설정 (프록시에서 실제 API로 전달)
    settings["env"]["ANTHROPIC_BASE_URL"] = serde_json::Value::String(proxy_url.clone());

    // settings.json 쓰기
    let settings_json = serde_json::to_string_pretty(&settings)
        .map_err(|e| format!("Failed to serialize settings: {}", e))?;

    // 디렉토리가 없으면 생성
    if let Some(parent) = settings_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create settings directory: {}", e))?;
    }

    fs::write(&settings_path, settings_json)
        .map_err(|e| format!("Failed to write settings.json: {}", e))?;

    tracing::info!("Updated Claude settings.json with proxy_url: {} (target: {})", proxy_url, base_url);

    Ok(())
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
    let port = state.db.get_proxy_port().await.unwrap_or(32080);
    Ok(ProxyStatus {
        running: proxy.is_some(),
        port,
    })
}

// Claude 설정 경로 가져오기
fn get_claude_settings_path() -> Result<PathBuf, String> {
    // Windows: %APPDATA%\Claude\settings.json
    // macOS/Linux: ~/.claude/settings.json
    #[cfg(target_os = "windows")]
    {
        let appdata = std::env::var("APPDATA")
            .map_err(|e| format!("Failed to get APPDATA: {}", e))?;
        Ok(PathBuf::from(appdata).join("Claude").join("settings.json"))
    }

    #[cfg(not(target_os = "windows"))]
    {
        let home = std::env::var("HOME")
            .map_err(|e| format!("Failed to get HOME: {}", e))?;
        Ok(PathBuf::from(home).join(".claude").join("settings.json"))
    }
}

// macOS Keychain에서 Claude 토큰 가져오기
#[derive(serde::Serialize)]
pub struct ClaudeTokenInfo {
    pub access_token: String,
    pub subscription_type: String,
    pub rate_limit_tier: String,
}

#[tauri::command]
pub async fn get_claude_token_from_keychain() -> Result<ClaudeTokenInfo, String> {
    use std::process::Command;

    // macOS에서만 작동
    #[cfg(target_os = "macos")]
    {
        let output = Command::new("security")
            .args(&["find-generic-password", "-s", "Claude Code-credentials", "-w"])
            .output()
            .map_err(|e| format!("Failed to execute security command: {}", e))?;

        if !output.status.success() {
            return Err("Failed to retrieve credentials from Keychain".to_string());
        }

        let credentials_json = String::from_utf8(output.stdout)
            .map_err(|e| format!("Failed to parse credentials: {}", e))?;

        // JSON 파싱
        let credentials: serde_json::Value = serde_json::from_str(&credentials_json)
            .map_err(|e| format!("Failed to parse credentials JSON: {}", e))?;

        let access_token = credentials
            .get("claudeAiOauth")
            .and_then(|oauth| oauth.get("accessToken"))
            .and_then(|token| token.as_str())
            .ok_or("accessToken not found")?;

        let subscription_type = credentials
            .get("claudeAiOauth")
            .and_then(|oauth| oauth.get("subscriptionType"))
            .and_then(|sub| sub.as_str())
            .unwrap_or("unknown")
            .to_string();

        let rate_limit_tier = credentials
            .get("claudeAiOauth")
            .and_then(|oauth| oauth.get("rateLimitTier"))
            .and_then(|tier| tier.as_str())
            .unwrap_or("unknown")
            .to_string();

        Ok(ClaudeTokenInfo {
            access_token: access_token.to_string(),
            subscription_type,
            rate_limit_tier,
        })
    }

    #[cfg(not(target_os = "macos"))]
    {
        Err("Keychain access is only available on macOS".to_string())
    }
}

// Auto Auth Scan 결과
#[derive(serde::Serialize)]
pub struct ScanResult {
    pub found_accounts: usize,
    pub imported_accounts: usize,
    pub messages: Vec<String>,
}

#[tauri::command]
pub async fn auto_scan_accounts(state: State<'_, AppState>) -> Result<ScanResult, String> {
    let mut messages = Vec::new();
    let mut imported_count = 0;
    let mut found_count = 0;

    // 1. settings.json 확인
    let settings_path = get_claude_settings_path().unwrap_or_else(|e| {
        messages.push(format!("⚠️  settings.json 경로 확인 실패: {}", e));
        PathBuf::from("")
    });

    // settings.json 읽기 시도
    if settings_path.exists() {
        if let Ok(settings_content) = fs::read_to_string(&settings_path) {
            if let Ok(settings_json) = serde_json::from_str::<serde_json::Value>(&settings_content) {
                messages.push("✅ settings.json 찾음".to_string());

                // Base URL과 토큰 추출
                let base_url = settings_json
                    .get("env")
                    .and_then(|env| env.get("ANTHROPIC_BASE_URL"))
                    .and_then(|url| url.as_str())
                    .unwrap_or("");

                let auth_token = settings_json
                    .get("env")
                    .and_then(|env| env.get("ANTHROPIC_AUTH_TOKEN"))
                    .and_then(|token| token.as_str())
                    .unwrap_or("");

                if !base_url.is_empty() {
                    found_count += 1;
                    messages.push(format!("Base URL: {}", base_url));

                    // GLM/Z.AI API인 경우
                    if base_url.contains("api.z.ai") || base_url.contains("glm") {
                        messages.push("📦 GLM API 발견".to_string());

                        let glm_account = Account::new(
                            "GLM from Settings".to_string(),
                            base_url.to_string()
                        );

                        match state.db.create_account(glm_account.clone(), auth_token.to_string()).await {
                            Ok(_) => {
                                imported_count += 1;
                                messages.push("✅ GLM 계정 추가 완료".to_string());
                            }
                            Err(e) => {
                                messages.push(format!("⚠️  GLM 계정 추가 실패: {}", e));
                            }
                        }
                    }
                    // Anthropic 공식 API인 경우
                    else if base_url.contains("api.anthropic.com") {
                        messages.push("🔑 Anthropic 공식 API - Keychain 시도".to_string());

                        #[cfg(target_os = "macos")]
                        {
                            if let Ok(token_info) = get_claude_token_from_keychain().await {
                                let anthropic_account = Account::new(
                                    "Anthropic (Keychain)".to_string(),
                                    "https://api.anthropic.com".to_string()
                                );

                                match state.db.create_account(anthropic_account.clone(), token_info.access_token).await {
                                    Ok(_) => {
                                        imported_count += 1;
                                        messages.push("✅ Anthropic 계정 추가 완료 (Keychain)".to_string());
                                    }
                                    Err(e) => {
                                        messages.push(format!("⚠️  Anthropic 계정 추가 실패: {}", e));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    } else {
        messages.push("⚠️  settings.json 없음".to_string());
    }

    // 2. Keychain 확인 (macOS만)
    #[cfg(target_os = "macos")]
    {
        if let Ok(token_info) = get_claude_token_from_keychain().await {
            messages.push("🔑 Keychain에서 Anthropic 토큰 발견".to_string());

            // 이미 Anthropic 계정이 있는지 확인
            let existing_accounts = state.db.get_accounts().await
                .map_err(|e| e.to_string())?;

            let has_anthropic = existing_accounts.iter()
                .any(|acc| acc.base_url.contains("anthropic.com"));

            if !has_anthropic {
                found_count += 1;
                let anthropic_account = Account::new(
                    "Anthropic Official".to_string(),
                    "https://api.anthropic.com".to_string()
                );

                match state.db.create_account(anthropic_account.clone(), token_info.access_token).await {
                    Ok(_) => {
                        imported_count += 1;
                        messages.push("✅ Anthropic 기본 계정 추가 완료".to_string());
                    }
                    Err(e) => {
                        messages.push(format!("⚠️  Anthropic 계정 추가 실패: {}", e));
                    }
                }
            }
        }
    }

    Ok(ScanResult {
        found_accounts: found_count,
        imported_accounts: imported_count,
        messages,
    })
}

// 사용량 통계
#[derive(serde::Serialize)]
pub struct UsageStats {
    pub request_count: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
}

#[tauri::command]
pub async fn get_usage_stats(state: State<'_, AppState>) -> Result<UsageStats, String> {
    let (request_count, input_tokens, output_tokens) = state
        .db
        .get_usage_stats()
        .await
        .map_err(|e| e.to_string())?;

    Ok(UsageStats {
        request_count,
        input_tokens,
        output_tokens,
    })
}

// 설정 관련 커맨드
#[derive(serde::Serialize)]
pub struct AppConfig {
    pub proxy_port: u16,
    pub auto_start: bool,
}

#[tauri::command]
pub async fn get_app_config(state: State<'_, AppState>) -> Result<AppConfig, String> {
    let proxy_port = state.db.get_proxy_port().await.map_err(|e| e.to_string())?;
    let auto_start = state.db.get_auto_start().await.map_err(|e| e.to_string())?;

    Ok(AppConfig {
        proxy_port,
        auto_start,
    })
}

#[tauri::command]
pub async fn set_proxy_port(port: u16, state: State<'_, AppState>) -> Result<(), String> {
    // 유효한 포트 범위 확인 (1024-65535)
    if port < 1024 || port > 65535 {
        return Err("포트는 1024-65535 범위여야 합니다".to_string());
    }

    state.db.set_config("proxy_port", &port.to_string()).await.map_err(|e| e.to_string())?;
    tracing::info!("Proxy port changed to {}", port);
    Ok(())
}

#[tauri::command]
pub async fn set_auto_start(enabled: bool, state: State<'_, AppState>) -> Result<(), String> {
    state.db.set_config("auto_start", if enabled { "true" } else { "false" }).await.map_err(|e| e.to_string())?;
    tracing::info!("Auto start set to {}", enabled);
    Ok(())
}

#[tauri::command]
pub async fn get_proxy_port(state: State<'_, AppState>) -> Result<u16, String> {
    state.db.get_proxy_port().await.map_err(|e| e.to_string())
}
