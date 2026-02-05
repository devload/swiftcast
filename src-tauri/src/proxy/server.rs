use super::hooks::{CompactionConfig, CompactionInjectorHook, CustomTaskHook, FileLoggerHook, HookRegistry, RequestContext, ResponseBuilder};
use super::question_detector::QuestionDetector;
use super::step_tracker::StepTracker;
use super::webhook::{AIQuestionData, SessionCompleteData, UsageData, WebhookClient};
use crate::storage::Database;
use anyhow::Result;
use axum::{
    body::Body,
    extract::{Request, State},
    http::{Method, StatusCode},
    response::Response,
    routing::any,
    Router,
};
use futures::StreamExt;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tower_http::cors::CorsLayer;

// 상수 정의
const MAX_REQUEST_BODY_SIZE: usize = 100 * 1024 * 1024; // 100MB
const MAX_CONCURRENT_DB_TASKS: usize = 10; // 동시 DB 작업 제한
const REQUEST_TIMEOUT_SECS: u64 = 300; // API 요청 타임아웃 (5분)

// 요청에서 모델 및 마지막 메시지 정보 추출
#[derive(Debug, Clone, Default)]
struct RequestInfo {
    model: String,
    last_message: Option<String>,
}

// 응답에서 사용량 정보 추출
#[derive(Debug, Clone, Default)]
struct UsageInfo {
    input_tokens: i64,
    output_tokens: i64,
    stop_reason: Option<String>,
}

fn parse_request_info(body: &[u8]) -> RequestInfo {
    if let Ok(json) = serde_json::from_slice::<serde_json::Value>(body) {
        let model = json.get("model").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();

        // 마지막 user 메시지 추출 (최대 100자로 제한)
        let last_message = json.get("messages")
            .and_then(|v| v.as_array())
            .and_then(|messages| {
                // 배열 뒤에서부터 user role 찾기
                messages.iter().rev().find(|msg| {
                    msg.get("role").and_then(|r| r.as_str()) == Some("user")
                })
            })
            .and_then(|msg| {
                // content가 문자열인 경우
                if let Some(content) = msg.get("content").and_then(|c| c.as_str()) {
                    Some(content.to_string())
                }
                // content가 배열인 경우 (multimodal)
                else if let Some(content_array) = msg.get("content").and_then(|c| c.as_array()) {
                    content_array.iter()
                        .find_map(|item| {
                            if item.get("type").and_then(|t| t.as_str()) == Some("text") {
                                item.get("text").and_then(|t| t.as_str()).map(|s| s.to_string())
                            } else {
                                None
                            }
                        })
                } else {
                    None
                }
            })
            .map(|s| {
                // 100자로 제한
                if s.chars().count() > 100 {
                    format!("{}...", s.chars().take(97).collect::<String>())
                } else {
                    s
                }
            });

        RequestInfo { model, last_message }
    } else {
        RequestInfo::default()
    }
}

// 요청 바디의 모델을 오버라이드
fn override_model_in_body(body: &[u8], new_model: &str) -> (bytes::Bytes, RequestInfo) {
    if let Ok(mut json) = serde_json::from_slice::<serde_json::Value>(body) {
        let original = json
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        // 마지막 user 메시지 추출 (오버라이드 전에 추출)
        let last_message = json.get("messages")
            .and_then(|v| v.as_array())
            .and_then(|messages| {
                messages.iter().rev().find(|msg| {
                    msg.get("role").and_then(|r| r.as_str()) == Some("user")
                })
            })
            .and_then(|msg| {
                if let Some(content) = msg.get("content").and_then(|c| c.as_str()) {
                    Some(content.to_string())
                } else if let Some(content_array) = msg.get("content").and_then(|c| c.as_array()) {
                    content_array.iter()
                        .find_map(|item| {
                            if item.get("type").and_then(|t| t.as_str()) == Some("text") {
                                item.get("text").and_then(|t| t.as_str()).map(|s| s.to_string())
                            } else {
                                None
                            }
                        })
                } else {
                    None
                }
            })
            .map(|s| {
                if s.chars().count() > 100 {
                    format!("{}...", s.chars().take(97).collect::<String>())
                } else {
                    s
                }
            });

        json["model"] = serde_json::Value::String(new_model.to_string());

        tracing::info!("MODEL OVERRIDE: {} -> {}", original, new_model);

        let new_body = serde_json::to_vec(&json).unwrap_or_else(|_| body.to_vec());
        (
            bytes::Bytes::from(new_body),
            RequestInfo {
                model: new_model.to_string(),
                last_message,
            },
        )
    } else {
        (bytes::Bytes::copy_from_slice(body), RequestInfo::default())
    }
}

// 에러 응답을 사용자 친화적으로 포맷팅
fn format_error_response(
    status_code: u16,
    error_body: &[u8],
    account_name: &str,
    target_url: &str,
) -> bytes::Bytes {
    let error_text = String::from_utf8_lossy(error_body);

    // 원본 에러 메시지 추출 시도
    let (error_type, error_message) = if let Ok(json) = serde_json::from_slice::<serde_json::Value>(error_body) {
        let err_type = json.get("error")
            .and_then(|e| e.get("type"))
            .and_then(|t| t.as_str())
            .unwrap_or("unknown_error");
        let err_msg = json.get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .unwrap_or(&error_text);
        (err_type.to_string(), err_msg.to_string())
    } else {
        ("unknown_error".to_string(), error_text.to_string())
    };

    // 에러 타입별 이모지와 설명
    let (emoji, description) = match error_type.as_str() {
        "overloaded_error" => ("⚠️", "API 서버가 과부하 상태입니다. 잠시 후 다시 시도해주세요."),
        "rate_limit_error" => ("🚫", "요청 한도를 초과했습니다. 잠시 기다려주세요."),
        "authentication_error" => ("🔐", "인증에 실패했습니다. API 키를 확인해주세요."),
        "invalid_request_error" => ("❌", "잘못된 요청입니다."),
        "api_error" => ("💥", "API 서버 오류가 발생했습니다."),
        "connection_error" => ("🔌", "서버에 연결할 수 없습니다."),
        _ => ("❗", "오류가 발생했습니다."),
    };

    // 포맷된 에러 메시지 생성
    let formatted = serde_json::json!({
        "type": "error",
        "error": {
            "type": error_type,
            "message": format!(
                "{} SwiftCast Error\n\n{}\n\n━━━━━━━━━━━━━━━━━━━━\n📍 Status: {} | Account: {}\n💬 {}\n━━━━━━━━━━━━━━━━━━━━",
                emoji,
                description,
                status_code,
                account_name,
                if error_message.len() > 200 {
                    format!("{}...", &error_message[..200])
                } else {
                    error_message
                }
            )
        }
    });

    bytes::Bytes::from(serde_json::to_vec(&formatted).unwrap_or_else(|_| error_body.to_vec()))
}

// Non-Anthropic 백엔드를 위해 thinking 관련 파라미터 제거
fn strip_thinking_params(body: &[u8]) -> bytes::Bytes {
    if let Ok(mut json) = serde_json::from_slice::<serde_json::Value>(body) {
        let mut modified = false;

        // thinking 파라미터 제거 (extended thinking)
        if json.get("thinking").is_some() {
            json.as_object_mut().map(|obj| obj.remove("thinking"));
            modified = true;
        }

        // betas 배열에서 thinking 관련 항목 제거
        if let Some(betas) = json.get_mut("betas").and_then(|v| v.as_array_mut()) {
            let original_len = betas.len();
            betas.retain(|b| {
                b.as_str().map(|s| !s.contains("thinking")).unwrap_or(true)
            });
            if betas.len() != original_len {
                modified = true;
            }
        }

        // metadata에서 thinking 관련 필드 제거
        if let Some(metadata) = json.get_mut("metadata").and_then(|v| v.as_object_mut()) {
            if metadata.remove("thinking").is_some() {
                modified = true;
            }
        }

        if modified {
            tracing::info!("STRIP THINKING: Removed thinking params for non-Anthropic backend");
            if let Ok(new_body) = serde_json::to_vec(&json) {
                return bytes::Bytes::from(new_body);
            }
        }

        bytes::Bytes::copy_from_slice(body)
    } else {
        bytes::Bytes::copy_from_slice(body)
    }
}

fn parse_usage_from_sse(data: &str) -> Option<UsageInfo> {
    // SSE 이벤트에서 usage 정보 추출
    // event: message_delta 또는 message_stop에 usage가 포함됨
    for line in data.lines() {
        if line.starts_with("data: ") {
            let json_str = &line[6..];
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(json_str) {
                // message_delta 이벤트: stop_reason과 usage 모두 포함
                if json.get("type").and_then(|v| v.as_str()) == Some("message_delta") {
                    let stop_reason = json.get("delta")
                        .and_then(|d| d.get("stop_reason"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());

                    if let Some(usage) = json.get("usage") {
                        return Some(UsageInfo {
                            input_tokens: usage.get("input_tokens").and_then(|v| v.as_i64()).unwrap_or(0),
                            output_tokens: usage.get("output_tokens").and_then(|v| v.as_i64()).unwrap_or(0),
                            stop_reason,
                        });
                    }
                }
                // message_stop 이벤트의 amazon/anthropic 형식
                if json.get("type").and_then(|v| v.as_str()) == Some("message_stop") {
                    if let Some(message) = json.get("message") {
                        let stop_reason = message.get("stop_reason")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());

                        if let Some(usage) = message.get("usage") {
                            return Some(UsageInfo {
                                input_tokens: usage.get("input_tokens").and_then(|v| v.as_i64()).unwrap_or(0),
                                output_tokens: usage.get("output_tokens").and_then(|v| v.as_i64()).unwrap_or(0),
                                stop_reason,
                            });
                        }
                    }
                }
            }
        }
    }
    None
}

/// Extract text content from SSE streaming response
fn parse_text_from_sse(data: &str) -> Option<String> {
    let mut text = String::new();

    for line in data.lines() {
        if line.starts_with("data: ") {
            let json_str = &line[6..];
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(json_str) {
                // content_block_delta 이벤트에서 텍스트 추출
                if json.get("type").and_then(|v| v.as_str()) == Some("content_block_delta") {
                    if let Some(delta) = json.get("delta") {
                        if let Some(t) = delta.get("text").and_then(|v| v.as_str()) {
                            text.push_str(t);
                        }
                    }
                }
            }
        }
    }

    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

/// Tool use info extracted from SSE
#[derive(Debug, Clone)]
struct ToolUseInfo {
    name: String,
    input: Option<serde_json::Value>,
}

/// Extract tool_use from SSE streaming response (content_block_start event)
fn parse_tool_use_from_sse(data: &str) -> Option<ToolUseInfo> {
    for line in data.lines() {
        if line.starts_with("data: ") {
            let json_str = &line[6..];
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(json_str) {
                // content_block_start with type: tool_use
                if json.get("type").and_then(|v| v.as_str()) == Some("content_block_start") {
                    if let Some(content_block) = json.get("content_block") {
                        if content_block.get("type").and_then(|v| v.as_str()) == Some("tool_use") {
                            if let Some(name) = content_block.get("name").and_then(|v| v.as_str()) {
                                return Some(ToolUseInfo {
                                    name: name.to_string(),
                                    input: content_block.get("input").cloned(),
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

pub struct ProxyServer {
    db: Arc<Database>,
    client: reqwest::Client,
    webhook: WebhookClient,
    question_detector: QuestionDetector,
    step_tracker: StepTracker,
    hook_registry: HookRegistry,
    custom_task_hook: Arc<CustomTaskHook>,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

#[derive(Clone)]
struct ProxyState {
    db: Arc<Database>,
    client: reqwest::Client,
    webhook: WebhookClient,
    question_detector: QuestionDetector,
    step_tracker: StepTracker,
    hook_registry: HookRegistry,
    custom_task_hook: Arc<CustomTaskHook>,
    db_task_semaphore: Arc<Semaphore>, // DB 작업 동시 실행 제한
}

impl ProxyServer {
    pub fn new(db: Arc<Database>) -> Self {
        // 타임아웃이 설정된 HTTP 클라이언트 생성
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .connect_timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        // Initialize CustomTaskHook with default config path
        let custom_task_hook = Arc::new(CustomTaskHook::new(CustomTaskHook::default_config_path()));

        Self {
            db,
            client,
            webhook: WebhookClient::new(),
            question_detector: QuestionDetector::new(),
            step_tracker: StepTracker::new(),
            hook_registry: HookRegistry::new(),
            custom_task_hook,
            shutdown_tx: None,
        }
    }

    pub async fn start(&mut self, port: u16) -> Result<()> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.shutdown_tx = Some(tx);

        // Load webhook configuration from DB
        let webhook_url = self.db.get_config("threadcast_webhook_url").await.ok().flatten();
        let webhook_enabled = self.db.get_config("threadcast_webhook_enabled").await
            .ok()
            .flatten()
            .map(|v| v == "true")
            .unwrap_or(false);
        self.webhook.configure(webhook_url.clone(), webhook_enabled).await;

        tracing::info!(
            "Webhook configured: enabled={}, url={:?}",
            webhook_enabled,
            webhook_url
        );

        // Load hook configuration from DB
        let hooks_enabled = self.db.get_config("hooks_enabled").await
            .ok()
            .flatten()
            .map(|v| v == "true")
            .unwrap_or(true); // Default to enabled
        let hooks_log_dir = self.db.get_config("hooks_log_dir").await
            .ok()
            .flatten()
            .map(std::path::PathBuf::from)
            .unwrap_or_else(FileLoggerHook::default_log_dir);
        let hooks_retention_days = self.db.get_config("hooks_retention_days").await
            .ok()
            .flatten()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(30);

        // Initialize and register FileLoggerHook
        let file_logger = FileLoggerHook::new(hooks_log_dir.clone())
            .with_retention_days(hooks_retention_days);
        self.hook_registry.register(Arc::new(file_logger)).await;
        self.hook_registry.set_enabled(hooks_enabled).await;

        tracing::info!(
            "Hooks configured: enabled={}, log_dir={:?}, retention_days={}",
            hooks_enabled,
            hooks_log_dir,
            hooks_retention_days
        );

        // Cleanup old log files on startup
        if hooks_enabled {
            let cleanup_dir = hooks_log_dir.clone();
            let cleanup_days = hooks_retention_days;
            tokio::spawn(async move {
                let cleaner = FileLoggerHook::new(cleanup_dir).with_retention_days(cleanup_days);
                cleaner.cleanup_old_logs().await;
            });
        }

        // Initialize and register CompactionInjectorHook
        let compaction_config_path = hooks_log_dir.parent()
            .unwrap_or(&hooks_log_dir)
            .join("compaction_config.json");
        let compaction_injector = CompactionInjectorHook::new(compaction_config_path.clone());

        // Load compaction config from DB or create default
        let compaction_enabled = self.db.get_config("compaction_injection_enabled").await
            .ok()
            .flatten()
            .map(|v| v == "true")
            .unwrap_or(false); // Default to disabled
        let summarization_instructions = self.db.get_config("compaction_summarization_instructions").await
            .ok()
            .flatten();
        let context_injection = self.db.get_config("compaction_context_injection").await
            .ok()
            .flatten();

        let config = CompactionConfig {
            enabled: compaction_enabled,
            summarization_instructions,
            context_injection,
            context_providers_enabled: true, // Enable external context providers by default
        };
        let _ = compaction_injector.update_config(config.clone()).await;
        self.hook_registry.register_modify_hook(Arc::new(compaction_injector)).await;

        tracing::info!(
            "CompactionInjector configured: enabled={}, config_path={:?}",
            compaction_enabled,
            compaction_config_path
        );

        // Log custom task loading
        let task_count = self.custom_task_hook.list_tasks().await.len();
        tracing::info!("CustomTaskHook loaded {} tasks from {:?}", task_count, CustomTaskHook::default_config_path());

        let state = ProxyState {
            db: self.db.clone(),
            client: self.client.clone(),
            webhook: self.webhook.clone(),
            question_detector: self.question_detector.clone(),
            step_tracker: self.step_tracker.clone(),
            hook_registry: self.hook_registry.clone(),
            custom_task_hook: self.custom_task_hook.clone(),
            db_task_semaphore: Arc::new(Semaphore::new(MAX_CONCURRENT_DB_TASKS)),
        };

        let app = Router::new()
            .route("/_swiftcast/threadcast/mapping", axum::routing::post(register_threadcast_mapping))
            .route("/*path", any(proxy_handler))
            .layer(CorsLayer::permissive())
            .with_state(state);

        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        tracing::info!("Proxy server starting on {}", addr);

        let server = axum::serve(
            tokio::net::TcpListener::bind(addr).await?,
            app.into_make_service(),
        );

        tokio::spawn(async move {
            tokio::select! {
                _ = server => {},
                _ = rx => {
                    tracing::info!("Proxy server shutting down");
                }
            }
        });

        Ok(())
    }

    pub async fn stop(&mut self) -> Result<()> {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        Ok(())
    }
}

async fn proxy_handler(
    State(state): State<ProxyState>,
    method: Method,
    req: Request,
) -> Result<Response, StatusCode> {
    // 요청 경로 (소유권 이전 전에 복사)
    let path = req.uri().path().to_string();

    // Handle internal SwiftCast API endpoints
    tracing::info!("Checking path: '{}', method: {:?}", path, method);
    if path == "/_swiftcast/threadcast/mapping" && method == Method::POST {
        tracing::info!("Handling ThreadCast mapping request");
        return handle_threadcast_mapping_internal(state, req).await;
    }

    let query = req.uri().query().unwrap_or("").to_string();
    let full_path = if query.is_empty() {
        path.clone()
    } else {
        format!("{}?{}", path, query)
    };

    // 원본 헤더 저장
    let original_headers = req.headers().clone();

    // 요청 바디 읽기 (최대 100MB 제한)
    let body_bytes = axum::body::to_bytes(req.into_body(), MAX_REQUEST_BODY_SIZE)
        .await
        .map_err(|e| {
            tracing::error!("Failed to read request body (may exceed {}MB limit): {}", MAX_REQUEST_BODY_SIZE / 1024 / 1024, e);
            StatusCode::PAYLOAD_TOO_LARGE
        })?;

    // 세션 ID 추출 (sentry-trace 또는 baggage에서 trace_id 추출)
    let session_id = original_headers
        .get("x-session-id")
        .or_else(|| original_headers.get("x-request-id"))
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .or_else(|| {
            // sentry-trace 헤더에서 trace_id 추출 (형식: trace_id-span_id)
            original_headers
                .get("sentry-trace")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.split('-').next())
                .map(|s| s.to_string())
        });

    // 세션별 계정 및 모델 오버라이드 결정
    let (account, model_override, is_existing_session) = if let Some(ref sid) = session_id {
        // 세션 설정이 있는지 확인
        if let Ok(Some(config)) = state.db.get_session_config(sid).await {
            // 세션 설정이 있으면 해당 계정 사용
            let acc = state
                .db
                .get_account(&config.account_id)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

            tracing::info!(
                "SESSION ROUTE: {} -> {} ({})",
                &sid[..std::cmp::min(12, sid.len())],
                acc.name,
                config.model_override.as_deref().unwrap_or("original")
            );

            (acc, config.model_override, true)
        } else {
            // 새 세션: 활성 계정으로 자동 등록
            let acc = state
                .db
                .get_active_account()
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

            // 세션 설정 자동 등록 (에러 무시)
            let _ = state.db.upsert_session_config(sid, &acc.id, None).await;

            // ThreadCast 환경변수 읽기 및 매핑 저장
            let threadcast_todo_id = std::env::var("THREADCAST_TODO_ID").ok();
            let threadcast_mission_id = std::env::var("THREADCAST_MISSION_ID").ok();

            if let Some(ref todo_id) = threadcast_todo_id {
                // ThreadCast 매핑 저장
                if let Err(e) = state.db.save_threadcast_mapping(
                    sid,
                    todo_id,
                    threadcast_mission_id.as_deref(),
                ).await {
                    tracing::warn!("Failed to save ThreadCast mapping: {}", e);
                } else {
                    tracing::info!(
                        "THREADCAST MAPPING: session={} -> todo={}, mission={:?}",
                        &sid[..std::cmp::min(12, sid.len())],
                        todo_id,
                        threadcast_mission_id
                    );
                }
            }

            tracing::info!(
                "NEW SESSION: {} -> {} (auto-assigned)",
                &sid[..std::cmp::min(12, sid.len())],
                acc.name
            );

            (acc, None, false)
        }
    } else {
        // 세션 ID 없음: 기존 동작 (활성 계정)
        let acc = state
            .db
            .get_active_account()
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
        (acc, None, false)
    };

    // API 키 로드
    let api_key = state
        .db
        .get_api_key(&account.id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // 모델 오버라이드 적용
    let (body_bytes, request_info) = if let Some(ref model) = model_override {
        override_model_in_body(&body_bytes, model)
    } else {
        let info = parse_request_info(&body_bytes);
        (body_bytes, info)
    };

    // 세션 활동 시간 및 마지막 메시지 업데이트 (기존 세션인 경우)
    if is_existing_session {
        if let Some(ref sid) = session_id {
            let _ = state.db.update_session_activity(sid, request_info.last_message.as_deref()).await;
        }
    }

    // 타겟 URL 생성
    let target_url = format!("{}{}", account.base_url, full_path);

    // HTTP 요청 생성
    let reqwest_method = match method.as_str() {
        "GET" => reqwest::Method::GET,
        "POST" => reqwest::Method::POST,
        "PUT" => reqwest::Method::PUT,
        "DELETE" => reqwest::Method::DELETE,
        "PATCH" => reqwest::Method::PATCH,
        _ => reqwest::Method::POST,
    };

    let mut request_builder = state.client.request(
        reqwest_method,
        &target_url,
    );

    // Anthropic 공식 API인지 확인
    let is_anthropic = account.base_url.contains("api.anthropic.com");

    // 원본 요청에 인증 헤더가 있는지 확인 (디버깅용)
    let _has_auth = original_headers.contains_key("x-api-key")
        || original_headers.contains_key("authorization");

    // 원본 헤더 전달 (일부 제외)
    for (key, value) in original_headers.iter() {
        let key_str = key.as_str().to_lowercase();
        match key_str.as_str() {
            "host" | "content-length" | "connection" | "transfer-encoding" | "accept-encoding" => {
                // 제외
            }
            "x-api-key" | "authorization" => {
                // Anthropic API일 때만 원본 인증 헤더 전달
                if is_anthropic {
                    if let Ok(v) = value.to_str() {
                        request_builder = request_builder.header(key.as_str(), v);
                    }
                }
            }
            _ => {
                if let Ok(v) = value.to_str() {
                    request_builder = request_builder.header(key.as_str(), v);
                }
            }
        }
    }

    // Anthropic이 아닌 경우 (GLM 등) 저장된 API 키 사용
    if !is_anthropic && !api_key.is_empty() {
        request_builder = request_builder.header("x-api-key", &api_key);
    }

    // 요청 로깅 (body move 전에)
    let body_len = body_bytes.len();
    tracing::info!(
        "PROXY: {} {} | Model: {} | Session: {} | BodySize: {}",
        method.as_str(),
        path,
        request_info.model,
        session_id.as_deref().unwrap_or("none"),
        body_len
    );

    // Create RequestContext for hooks
    let request_body_json = serde_json::from_slice(&body_bytes).unwrap_or(serde_json::Value::Null);
    let request_context = RequestContext::new(
        session_id.clone(),
        request_info.model.clone(),
        method.as_str().to_string(),
        path.clone(),
        request_body_json,
    );

    // Load session-specific hook configuration (if exists)
    let session_hooks = if let Some(ref sid) = session_id {
        state.db.get_session_hooks(sid).await.ok().flatten()
    } else {
        None
    };

    // Check if custom tasks are enabled for this session
    let custom_tasks_enabled = session_hooks.as_ref()
        .map(|h| h.custom_tasks_enabled)
        .unwrap_or(true); // Default: enabled

    // Check for custom task interception (>>swiftcast <name>)
    let intercept_result = if custom_tasks_enabled {
        state.custom_task_hook.try_intercept(&request_context).await
    } else {
        super::hooks::custom_task::InterceptResult {
            intercepted: false,
            response_text: String::new(),
            task_name: None,
        }
    };
    if intercept_result.intercepted {
        tracing::info!(
            "CUSTOM TASK INTERCEPTED: {:?} (session: {:?})",
            intercept_result.task_name,
            session_id
        );

        // Generate fake SSE response
        let sse_response = CustomTaskHook::generate_sse_response(&intercept_result.response_text);

        // Return SSE response
        return Ok(Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "text/event-stream")
            .header("cache-control", "no-cache")
            .body(Body::from(sse_response))
            .unwrap());
    }

    // Check if API logging is enabled for this session
    let api_logging_enabled = session_hooks.as_ref()
        .map(|h| h.api_logging_enabled)
        .unwrap_or(true); // Default: enabled

    // Trigger request_before hook (only if logging enabled for session)
    if api_logging_enabled {
        state.hook_registry.trigger_request_before(&request_context).await;
    }

    // Check if compaction injection is enabled for this session
    let compaction_enabled = session_hooks.as_ref()
        .map(|h| h.compaction_injection_enabled)
        .unwrap_or(false); // Default: disabled (use system setting)

    // Apply modify hooks to request body (for compaction injection etc.)
    let body_str = String::from_utf8_lossy(&body_bytes);
    let modified_body = if compaction_enabled {
        if let Some(modified_body) = state.hook_registry.apply_request_modifications(&body_str, &request_context).await {
            tracing::info!("Request body modified by hooks (compaction injection)");
            bytes::Bytes::from(modified_body)
        } else {
            body_bytes
        }
    } else {
        body_bytes
    };

    // Non-Anthropic 백엔드일 경우 thinking 파라미터 제거
    let final_body = if !is_anthropic {
        strip_thinking_params(&modified_body)
    } else {
        modified_body
    };

    // 바디 추가 (Content-Length 명시적 설정)
    let body_vec = final_body.to_vec();
    if !body_vec.is_empty() {
        request_builder = request_builder
            .header("content-length", body_vec.len().to_string())
            .body(body_vec);
    }

    // 요청 전송
    let response = match request_builder.send().await {
        Ok(resp) => resp,
        Err(e) => {
            tracing::error!(
                "PROXY REQUEST FAILED: {} | Target: {} | Account: {} | Session: {:?} | Error: {}",
                method.as_str(),
                target_url,
                account.name,
                session_id,
                e
            );

            // 연결 실패 에러 포맷팅
            let error_json = serde_json::json!({
                "type": "error",
                "error": {
                    "type": "connection_error",
                    "message": e.to_string()
                }
            });
            let formatted_error = format_error_response(
                502,
                serde_json::to_vec(&error_json).unwrap_or_default().as_slice(),
                &account.name,
                &target_url,
            );

            let builder = Response::builder()
                .status(StatusCode::BAD_GATEWAY)
                .header("content-type", "application/json");
            return Ok(builder.body(Body::from(formatted_error)).unwrap());
        }
    };

    let response_status = response.status();

    // 에러 응답인 경우 상세 로깅 (응답 바디 포함)
    if !response_status.is_success() {
        // 에러 응답은 바디 전체를 읽어서 로깅
        let error_body = response.bytes().await.unwrap_or_default();
        let error_text = String::from_utf8_lossy(&error_body);

        tracing::error!(
            "PROXY RESPONSE ERROR: {} {} | Target: {} | Account: {} | Session: {:?} | Body: {}",
            response_status.as_u16(),
            response_status.canonical_reason().unwrap_or("Unknown"),
            target_url,
            account.name,
            session_id,
            if error_text.len() > 500 {
                format!("{}...(truncated)", &error_text[..500])
            } else {
                error_text.to_string()
            }
        );

        // 포맷된 에러 응답 반환
        let formatted_error = format_error_response(
            response_status.as_u16(),
            &error_body,
            &account.name,
            &target_url,
        );
        let mut builder = Response::builder().status(response_status.as_u16());
        builder = builder.header("content-type", "application/json");
        return Ok(builder.body(Body::from(formatted_error)).unwrap());
    }

    tracing::info!("PROXY RESPONSE: {}", response_status);

    // 응답 상태 및 헤더
    let status = response_status;

    // 응답 헤더 전달
    let mut builder = Response::builder().status(status.as_u16());

    for (key, value) in response.headers().iter() {
        let key_str = key.as_str().to_lowercase();
        match key_str.as_str() {
            "transfer-encoding" | "connection" => {
                // 제외
            }
            _ => {
                if let Ok(v) = value.to_str() {
                    builder = builder.header(key.as_str(), v);
                }
            }
        }
    }

    // 응답 바디 스트리밍 + 사용량 추출
    let account_id = account.id.clone();
    let model = request_info.model.clone();
    let db = state.db.clone();
    let session_id_for_log = session_id.clone();
    let semaphore = state.db_task_semaphore.clone();
    let webhook = state.webhook.clone();
    let question_detector = state.question_detector.clone();
    let step_tracker = state.step_tracker.clone();
    let hook_registry = state.hook_registry.clone();
    let request_context_for_stream = request_context.clone();
    let api_logging_enabled_for_stream = api_logging_enabled;

    // Create ResponseBuilder for accumulating response data
    let response_builder = ResponseBuilder::new(status.as_u16());

    let body_stream = response.bytes_stream();

    // 스트림을 래핑하여 사용량 정보 및 AI 질문 추출
    let response_builder_for_stream = response_builder.clone();
    let wrapped_stream = body_stream.map(move |chunk_result| {
        if let Ok(ref chunk) = chunk_result {
            // SSE 데이터에서 텍스트 및 usage 추출 시도
            if let Ok(text) = std::str::from_utf8(chunk) {
                // Tool use 감지 및 step tracking
                if let Some(tool_info) = parse_tool_use_from_sse(text) {
                    let tracker = step_tracker.clone();
                    let db_for_step = db.clone();
                    let webhook_for_step = webhook.clone();
                    let session_for_step = session_id_for_log.clone();

                    tokio::spawn(async move {
                        if let Some(ref sid) = session_for_step {
                            let (completed, new_step) = tracker
                                .process_tool_use(sid, &tool_info.name, tool_info.input.as_ref())
                                .await;

                            if completed.is_some() || new_step.is_some() {
                                tracker.send_updates(&db_for_step, &webhook_for_step, sid, completed, new_step).await;
                            }
                        }
                    });
                }

                // AI 질문 감지를 위한 텍스트 처리
                if let Some(content_text) = parse_text_from_sse(text) {
                    // Accumulate response text for hooks
                    let rb = response_builder_for_stream.clone();
                    let text_clone = content_text.clone();
                    tokio::spawn(async move {
                        rb.append_text(&text_clone).await;
                    });

                    let detector = question_detector.clone();
                    let db_for_question = db.clone();
                    let webhook_for_question = webhook.clone();
                    let session_id_for_question = session_id_for_log.clone();

                    tokio::spawn(async move {
                        if let Some(detected) = detector.process_text(&content_text).await {
                            tracing::info!(
                                "AI QUESTION DETECTED: {}",
                                detected.question.chars().take(50).collect::<String>()
                            );

                            // Send webhook if session has ThreadCast mapping
                            if let Some(ref sid) = session_id_for_question {
                                if let Ok(Some((todo_id, _))) = db_for_question.get_threadcast_mapping(sid).await {
                                    let _ = webhook_for_question.send_ai_question(
                                        Some(todo_id),
                                        sid,
                                        AIQuestionData {
                                            question: detected.question,
                                            options: detected.options,
                                            context: detected.context,
                                        },
                                    ).await;
                                }
                            }
                        }
                    });
                }

                if let Some(usage) = parse_usage_from_sse(text) {
                    tracing::info!(
                        "USAGE: in={}, out={}, stop_reason={:?}",
                        usage.input_tokens,
                        usage.output_tokens,
                        usage.stop_reason
                    );

                    // Update response builder with token counts and stop_reason
                    let rb = response_builder_for_stream.clone();
                    let input = usage.input_tokens;
                    let output = usage.output_tokens;
                    let stop_reason = usage.stop_reason.clone();
                    tokio::spawn(async move {
                        rb.set_tokens(input, output).await;
                        if let Some(reason) = stop_reason {
                            rb.set_stop_reason(reason).await;
                        }
                    });

                    // DB에 사용량 로깅 (비동기로 처리, Semaphore로 동시 실행 제한)
                    let db_clone = db.clone();
                    let account_id_clone = account_id.clone();
                    let model_clone = model.clone();
                    let session_id_clone = session_id_for_log.clone();
                    let sem = semaphore.clone();
                    let webhook_clone = webhook.clone();
                    let usage_input = usage.input_tokens;
                    let usage_output = usage.output_tokens;
                    let rb_for_webhook = response_builder_for_stream.clone();

                    // Trigger hooks with final response when we have usage (stream end indicator)
                    // Only if API logging is enabled for this session
                    if api_logging_enabled_for_stream {
                        let hr = hook_registry.clone();
                        let req_ctx = request_context_for_stream.clone();
                        let rb_final = response_builder_for_stream.clone();
                        tokio::spawn(async move {
                            // Small delay to ensure all text is accumulated
                            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                            let res_ctx = rb_final.build().await;
                            if res_ctx.is_success {
                                hr.trigger_request_success(&req_ctx, &res_ctx).await;
                            } else {
                                hr.trigger_request_failed(&req_ctx, &res_ctx).await;
                            }
                            hr.trigger_response_complete(&req_ctx, &res_ctx).await;
                            hr.trigger_request_after(&req_ctx, &res_ctx).await;
                        });
                    }

                    // Complete current step when response ends (usage indicates stream end)
                    let tracker_for_complete = step_tracker.clone();
                    let db_for_complete = db.clone();
                    let webhook_for_complete = webhook.clone();
                    let session_for_complete = session_id_for_log.clone();
                    let stop_reason_for_complete = usage.stop_reason.clone();
                    let usage_input_for_complete = usage.input_tokens;
                    let usage_output_for_complete = usage.output_tokens;

                    tokio::spawn(async move {
                        // Complete current step first
                        if let Some(ref sid) = session_for_complete {
                            if let Some(step_data) = tracker_for_complete.complete_current_step(sid).await {
                                tracker_for_complete.send_single_update(&db_for_complete, &webhook_for_complete, sid, step_data).await;
                            }

                            // Send session_complete webhook when stop_reason is "end_turn"
                            if stop_reason_for_complete.as_deref() == Some("end_turn") {
                                if let Ok(Some((todo_id, _))) = db_for_complete.get_threadcast_mapping(sid).await {
                                    let completed_steps = tracker_for_complete.get_completed_steps(sid).await;
                                    let _ = webhook_for_complete.send_session_complete(
                                        Some(todo_id),
                                        sid,
                                        SessionCompleteData {
                                            stop_reason: "end_turn".to_string(),
                                            total_input_tokens: usage_input_for_complete,
                                            total_output_tokens: usage_output_for_complete,
                                            duration_ms: 0, // TODO: track actual duration
                                            completed_steps,
                                        },
                                    ).await;
                                }
                            }
                        }
                    });

                    tokio::spawn(async move {
                        // Semaphore permit 획득 시도 (논블로킹)
                        // permit 획득 실패 시 로깅만 스킵하고 서비스는 정상 진행
                        let _permit = match sem.try_acquire() {
                            Ok(permit) => permit,
                            Err(_) => {
                                // 동시 DB 작업이 너무 많음 - 로깅 스킵 (서비스 우선)
                                tracing::debug!("Too many concurrent DB tasks, skipping usage log");
                                return;
                            }
                        };

                        if let Err(e) = db_clone
                            .log_usage(
                                &account_id_clone,
                                &model_clone,
                                usage_input,
                                usage_output,
                                session_id_clone.as_deref(),
                            )
                            .await
                        {
                            tracing::error!("Failed to log usage: {}", e);
                        }

                        // Send webhook to ThreadCast unconditionally
                        // ThreadCast will look up the session_id -> todo_id mapping
                        if let Some(ref sid) = session_id_clone {
                            // Small delay to ensure response text is fully accumulated
                            tokio::time::sleep(std::time::Duration::from_millis(50)).await;

                            // Get response summary (first 200 chars)
                            let res_ctx = rb_for_webhook.build().await;
                            let response_summary = if res_ctx.response_text.is_empty() {
                                None
                            } else {
                                Some(res_ctx.response_text.chars().take(200).collect::<String>())
                            };

                            let _ = webhook_clone.send_usage(
                                None,  // ThreadCast will resolve todo_id from session_id
                                sid,
                                UsageData {
                                    model: model_clone,
                                    input_tokens: usage_input,
                                    output_tokens: usage_output,
                                    response_summary,
                                },
                            ).await;
                        }
                    });
                }
            }
        }
        chunk_result
    });

    let body = Body::from_stream(wrapped_stream);

    // 응답 반환
    Ok(builder.body(body).unwrap())
}

/// Request body for ThreadCast mapping registration
#[derive(Debug, serde::Deserialize)]
struct ThreadcastMappingRequest {
    session_id: String,
    #[serde(default)]
    todo_id: Option<String>,
    mission_id: Option<String>,
    // Alternative format: args string like "--todo-id=XXX"
    #[serde(default)]
    args: Option<String>,
    // Optional fields from TaskContext format
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    model: Option<String>,
}

impl ThreadcastMappingRequest {
    /// Extract todo_id from either direct field or args string
    fn get_todo_id(&self) -> Option<String> {
        // Direct todo_id takes precedence
        if let Some(ref tid) = self.todo_id {
            if !tid.is_empty() {
                return Some(tid.clone());
            }
        }
        // Parse from args: "--todo-id=XXX" or "--todo-id XXX"
        if let Some(ref args) = self.args {
            // Try --todo-id=VALUE format
            if let Some(pos) = args.find("--todo-id=") {
                let start = pos + 10; // len of "--todo-id="
                let rest = &args[start..];
                let end = rest.find(' ').unwrap_or(rest.len());
                let todo_id = rest[..end].trim().to_string();
                if !todo_id.is_empty() {
                    return Some(todo_id);
                }
            }
        }
        None
    }
}

/// Handle ThreadCast mapping from proxy_handler
/// Accepts both direct format { session_id, todo_id } and args format { session_id, args: "--todo-id=XXX" }
async fn handle_threadcast_mapping_internal(
    state: ProxyState,
    req: Request,
) -> Result<Response, StatusCode> {
    let body_bytes = axum::body::to_bytes(req.into_body(), 1024 * 1024)
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    let payload: ThreadcastMappingRequest = serde_json::from_slice(&body_bytes)
        .map_err(|e| {
            tracing::error!("Failed to parse ThreadCast mapping request: {}", e);
            StatusCode::BAD_REQUEST
        })?;

    // Extract todo_id from either direct field or args
    let todo_id = match payload.get_todo_id() {
        Some(tid) => tid,
        None => {
            tracing::warn!("ThreadCast mapping missing todo_id: session={}", &payload.session_id[..std::cmp::min(12, payload.session_id.len())]);
            return Err(StatusCode::BAD_REQUEST);
        }
    };

    tracing::info!(
        "Registering ThreadCast mapping: session={} -> todo={}",
        &payload.session_id[..std::cmp::min(12, payload.session_id.len())],
        todo_id
    );

    match state
        .db
        .save_threadcast_mapping(
            &payload.session_id,
            &todo_id,
            payload.mission_id.as_deref(),
        )
        .await
    {
        Ok(_) => {
            tracing::info!("ThreadCast mapping registered successfully");
            let body = serde_json::json!({
                "success": true,
                "session_id": payload.session_id,
                "todo_id": todo_id
            });
            Ok(Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap())
        }
        Err(e) => {
            tracing::error!("Failed to register ThreadCast mapping: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Register ThreadCast session -> todo mapping
/// Accepts both direct format { session_id, todo_id } and args format { session_id, args: "--todo-id=XXX" }
async fn register_threadcast_mapping(
    State(state): State<ProxyState>,
    axum::Json(payload): axum::Json<ThreadcastMappingRequest>,
) -> Result<axum::Json<serde_json::Value>, StatusCode> {
    // Extract todo_id from either direct field or args
    let todo_id = match payload.get_todo_id() {
        Some(tid) => tid,
        None => {
            tracing::warn!("ThreadCast mapping missing todo_id: session={}", &payload.session_id[..std::cmp::min(12, payload.session_id.len())]);
            return Err(StatusCode::BAD_REQUEST);
        }
    };

    tracing::info!(
        "Registering ThreadCast mapping: session={} -> todo={}",
        &payload.session_id[..std::cmp::min(12, payload.session_id.len())],
        todo_id
    );

    match state
        .db
        .save_threadcast_mapping(
            &payload.session_id,
            &todo_id,
            payload.mission_id.as_deref(),
        )
        .await
    {
        Ok(_) => {
            tracing::info!("ThreadCast mapping registered successfully");

            // Forward mapping to ThreadCast webhook
            let webhook_url = state.db.get_config("threadcast_webhook_url").await
                .ok()
                .flatten()
                .unwrap_or_else(|| "http://localhost:21000".to_string());

            let forward_url = format!("{}/api/webhooks/session-mapping", webhook_url);
            let forward_payload = serde_json::json!({
                "session_id": payload.session_id,
                "args": format!("--todo-id={}", todo_id)
            });

            let client = reqwest::Client::new();
            match client.post(&forward_url)
                .json(&forward_payload)
                .timeout(std::time::Duration::from_secs(5))
                .send()
                .await
            {
                Ok(resp) => {
                    tracing::info!("Forwarded mapping to ThreadCast: status={}", resp.status());
                }
                Err(e) => {
                    tracing::warn!("Failed to forward mapping to ThreadCast: {}", e);
                }
            }

            Ok(axum::Json(serde_json::json!({
                "success": true,
                "session_id": payload.session_id,
                "todo_id": todo_id
            })))
        }
        Err(e) => {
            tracing::error!("Failed to register ThreadCast mapping: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
