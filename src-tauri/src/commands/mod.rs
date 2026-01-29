use crate::{models::{Account, SessionDetail}, proxy::ProxyServer, AppState};
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

// 앱 시작 시 Claude settings.json 초기화 (main.rs에서 호출)
pub fn init_claude_settings(base_url: &str, proxy_port: u16) -> Result<(), String> {
    update_claude_settings(base_url, proxy_port)
}

// 프록시 중지 시 settings.json에서 프록시 설정 제거
fn clear_claude_settings() -> Result<(), String> {
    use std::fs;

    let settings_path = get_claude_settings_path()?;

    if !settings_path.exists() {
        return Ok(());
    }

    // 기존 설정 읽기
    let content = fs::read_to_string(&settings_path)
        .map_err(|e| format!("Failed to read settings.json: {}", e))?;

    if let Ok(mut settings) = serde_json::from_str::<serde_json::Value>(&content) {
        if let Some(obj) = settings.as_object_mut() {
            // env에서 ANTHROPIC_BASE_URL 제거
            if let Some(env) = obj.get_mut("env").and_then(|e| e.as_object_mut()) {
                env.remove("ANTHROPIC_BASE_URL");

                // env가 비어있으면 env 자체 제거
                if env.is_empty() {
                    obj.remove("env");
                }
            }

            // 파일이 비어있으면 삭제, 아니면 업데이트
            if obj.is_empty() {
                fs::remove_file(&settings_path)
                    .map_err(|e| format!("Failed to delete settings.json: {}", e))?;
            } else {
                let settings_json = serde_json::to_string_pretty(&settings)
                    .map_err(|e| format!("Failed to serialize settings: {}", e))?;
                fs::write(&settings_path, settings_json)
                    .map_err(|e| format!("Failed to write settings.json: {}", e))?;
            }
        }
    } else {
        // 파싱 실패시 파일 삭제
        fs::remove_file(&settings_path)
            .map_err(|e| format!("Failed to delete settings.json: {}", e))?;
    }

    Ok(())
}

// Claude settings.json 업데이트 - 항상 프록시 URL 설정 (사용량 추적을 위해)
fn update_claude_settings(_base_url: &str, proxy_port: u16) -> Result<(), String> {
    use std::fs;

    let settings_path = get_claude_settings_path()?;

    // 프록시 URL 설정 (Anthropic이든 GLM이든 모두 프록시를 통해 사용량 추적)
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

    tracing::info!("Updated Claude settings.json with proxy_url: {}", proxy_url);

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

    // 프록시 시작 시 활성 계정에 따라 settings.json 업데이트
    if let Ok(Some(account)) = state.db.get_active_account().await {
        init_claude_settings(&account.base_url, port)?;
        tracing::info!("Updated Claude settings.json for proxy");
    }

    Ok(())
}

#[tauri::command]
pub async fn stop_proxy(state: State<'_, AppState>) -> Result<(), String> {
    let mut proxy = state.proxy.write().await;

    if let Some(mut server) = proxy.take() {
        server.stop().await.map_err(|e| e.to_string())?;
    }

    // 프록시 중지 시 settings.json 정리 (Claude Code가 직접 연결하도록)
    clear_claude_settings()?;
    tracing::info!("Cleared Claude settings.json - Claude Code will use direct connection");

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
// Claude Code는 모든 플랫폼에서 ~/.claude/settings.json 사용
fn get_claude_settings_path() -> Result<PathBuf, String> {
    let home = get_home_dir()?;
    Ok(PathBuf::from(home).join(".claude").join("settings.json"))
}

// 크로스 플랫폼 홈 디렉토리 가져오기
fn get_home_dir() -> Result<String, String> {
    // Windows: USERPROFILE 환경변수 사용
    // macOS/Linux: HOME 환경변수 사용
    #[cfg(target_os = "windows")]
    {
        std::env::var("USERPROFILE")
            .map_err(|e| format!("Failed to get USERPROFILE: {}", e))
    }

    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME")
            .map_err(|e| format!("Failed to get HOME: {}", e))
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
    // 유효한 포트 범위 확인 (1024 이상, u16이므로 최대 65535)
    if port < 1024 {
        return Err("포트는 1024 이상이어야 합니다".to_string());
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

// ===== Hook 설정 Commands =====

#[derive(serde::Serialize, serde::Deserialize)]
pub struct HookConfig {
    pub hooks_enabled: bool,
    pub hooks_retention_days: u64,
    pub compaction_injection_enabled: bool,
    pub compaction_summarization_instructions: String,
    pub compaction_context_injection: String,
}

#[tauri::command]
pub async fn get_hook_config(state: State<'_, AppState>) -> Result<HookConfig, String> {
    let hooks_enabled = state.db.get_config("hooks_enabled").await
        .map_err(|e| e.to_string())?
        .map(|v| v == "true")
        .unwrap_or(true);

    let hooks_retention_days = state.db.get_config("hooks_retention_days").await
        .map_err(|e| e.to_string())?
        .and_then(|v| v.parse().ok())
        .unwrap_or(30);

    let compaction_injection_enabled = state.db.get_config("compaction_injection_enabled").await
        .map_err(|e| e.to_string())?
        .map(|v| v == "true")
        .unwrap_or(false);

    let compaction_summarization_instructions = state.db.get_config("compaction_summarization_instructions").await
        .map_err(|e| e.to_string())?
        .unwrap_or_default();

    let compaction_context_injection = state.db.get_config("compaction_context_injection").await
        .map_err(|e| e.to_string())?
        .unwrap_or_default();

    Ok(HookConfig {
        hooks_enabled,
        hooks_retention_days,
        compaction_injection_enabled,
        compaction_summarization_instructions,
        compaction_context_injection,
    })
}

#[tauri::command]
pub async fn set_hook_config(config: HookConfig, state: State<'_, AppState>) -> Result<(), String> {
    state.db.set_config("hooks_enabled", if config.hooks_enabled { "true" } else { "false" })
        .await.map_err(|e| e.to_string())?;

    state.db.set_config("hooks_retention_days", &config.hooks_retention_days.to_string())
        .await.map_err(|e| e.to_string())?;

    state.db.set_config("compaction_injection_enabled", if config.compaction_injection_enabled { "true" } else { "false" })
        .await.map_err(|e| e.to_string())?;

    state.db.set_config("compaction_summarization_instructions", &config.compaction_summarization_instructions)
        .await.map_err(|e| e.to_string())?;

    state.db.set_config("compaction_context_injection", &config.compaction_context_injection)
        .await.map_err(|e| e.to_string())?;

    tracing::info!("Hook config updated: enabled={}, compaction={}", config.hooks_enabled, config.compaction_injection_enabled);
    Ok(())
}

#[tauri::command]
pub fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

// ===== 세션별 Hook 설정 Commands =====

use crate::storage::database::SessionHookConfig;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct SessionHookConfigInput {
    pub session_id: String,
    pub api_logging_enabled: bool,
    pub compaction_injection_enabled: bool,
    pub compaction_summarization_instructions: Option<String>,
    pub compaction_context_injection: Option<String>,
    pub custom_tasks_enabled: bool,
}

#[tauri::command]
pub async fn get_session_hooks(session_id: String, state: State<'_, AppState>) -> Result<Option<SessionHookConfig>, String> {
    state.db.get_session_hooks(&session_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_session_hooks(config: SessionHookConfigInput, state: State<'_, AppState>) -> Result<(), String> {
    let now = chrono::Utc::now().timestamp();
    let hook_config = SessionHookConfig {
        session_id: config.session_id,
        api_logging_enabled: config.api_logging_enabled,
        compaction_injection_enabled: config.compaction_injection_enabled,
        compaction_summarization_instructions: config.compaction_summarization_instructions,
        compaction_context_injection: config.compaction_context_injection,
        custom_tasks_enabled: config.custom_tasks_enabled,
        created_at: now,
        updated_at: now,
    };
    state.db.set_session_hooks(&hook_config).await.map_err(|e| e.to_string())?;
    tracing::info!("Session hooks updated for session: {}", hook_config.session_id);
    Ok(())
}

#[tauri::command]
pub async fn delete_session_hooks(session_id: String, state: State<'_, AppState>) -> Result<(), String> {
    state.db.delete_session_hooks(&session_id).await.map_err(|e| e.to_string())?;
    tracing::info!("Session hooks deleted for session: {}", session_id);
    Ok(())
}

#[tauri::command]
pub async fn get_all_session_hooks(state: State<'_, AppState>) -> Result<Vec<SessionHookConfig>, String> {
    state.db.get_all_session_hooks().await.map_err(|e| e.to_string())
}

// 사용량 관련 명령어
use crate::storage::database::{UsageLog, AccountUsageStats, ModelUsageStats, DailyUsageStats, SessionUsageStats};

#[tauri::command]
pub async fn get_recent_usage(limit: i64, state: State<'_, AppState>) -> Result<Vec<UsageLog>, String> {
    state.db.get_recent_usage(limit).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_usage_by_account(state: State<'_, AppState>) -> Result<Vec<AccountUsageStats>, String> {
    state.db.get_usage_by_account().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_usage_by_model(state: State<'_, AppState>) -> Result<Vec<ModelUsageStats>, String> {
    state.db.get_usage_by_model().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_daily_usage(days: i64, state: State<'_, AppState>) -> Result<Vec<DailyUsageStats>, String> {
    state.db.get_daily_usage(days).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn clear_usage_logs(state: State<'_, AppState>) -> Result<(), String> {
    state.db.clear_usage_logs().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_usage_by_session(state: State<'_, AppState>) -> Result<Vec<SessionUsageStats>, String> {
    state.db.get_usage_by_session().await.map_err(|e| e.to_string())
}

// ===== 세션 관리 Commands =====

// 모델 정보
#[derive(serde::Serialize, Clone)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
}

// Anthropic 모델 목록
fn get_anthropic_models() -> Vec<ModelInfo> {
    vec![
        ModelInfo {
            id: "claude-sonnet-4-20250514".to_string(),
            name: "Claude Sonnet 4".to_string(),
        },
        ModelInfo {
            id: "claude-opus-4-20250514".to_string(),
            name: "Claude Opus 4".to_string(),
        },
        ModelInfo {
            id: "claude-3-5-haiku-20241022".to_string(),
            name: "Claude 3.5 Haiku".to_string(),
        },
        ModelInfo {
            id: "claude-3-5-sonnet-20241022".to_string(),
            name: "Claude 3.5 Sonnet".to_string(),
        },
    ]
}

// GLM 모델 목록
fn get_glm_models() -> Vec<ModelInfo> {
    vec![
        ModelInfo {
            id: "glm-4".to_string(),
            name: "GLM-4".to_string(),
        },
        ModelInfo {
            id: "glm-4-flash".to_string(),
            name: "GLM-4 Flash".to_string(),
        },
    ]
}

// Provider별 사용 가능한 모델 목록 조회
#[tauri::command]
pub async fn get_available_models(base_url: String) -> Result<Vec<ModelInfo>, String> {
    if base_url.contains("anthropic.com") {
        Ok(get_anthropic_models())
    } else if base_url.contains("z.ai") || base_url.contains("glm") {
        Ok(get_glm_models())
    } else {
        // 알 수 없는 provider - Anthropic 기본값 반환
        Ok(get_anthropic_models())
    }
}

// 활성 세션 목록 조회 (최근 24시간)
#[tauri::command]
pub async fn get_active_sessions(state: State<'_, AppState>) -> Result<Vec<SessionDetail>, String> {
    state.db.get_active_sessions().await.map_err(|e| e.to_string())
}

// 세션 설정 변경 (계정 및 모델 오버라이드)
#[tauri::command]
pub async fn set_session_config(
    session_id: String,
    account_id: String,
    model_override: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // 계정이 존재하는지 확인
    let account = state
        .db
        .get_account(&account_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Account not found".to_string())?;

    state
        .db
        .upsert_session_config(&session_id, &account_id, model_override.as_deref())
        .await
        .map_err(|e| e.to_string())?;

    tracing::info!(
        "Session {} config updated: account={}, model={}",
        &session_id[..std::cmp::min(12, session_id.len())],
        account.name,
        model_override.as_deref().unwrap_or("original")
    );

    Ok(())
}

// 세션 설정 삭제
#[tauri::command]
pub async fn delete_session_config(
    session_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .db
        .delete_session_config(&session_id)
        .await
        .map_err(|e| e.to_string())?;

    tracing::info!(
        "Session {} config deleted",
        &session_id[..std::cmp::min(12, session_id.len())]
    );

    Ok(())
}
