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

fn parse_usage_from_sse(data: &str) -> Option<UsageInfo> {
    // SSE 이벤트에서 usage 정보 추출
    // event: message_delta 또는 message_stop에 usage가 포함됨
    for line in data.lines() {
        if line.starts_with("data: ") {
            let json_str = &line[6..];
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(json_str) {
                // message_delta의 usage 또는 최종 usage
                if let Some(usage) = json.get("usage") {
                    return Some(UsageInfo {
                        input_tokens: usage.get("input_tokens").and_then(|v| v.as_i64()).unwrap_or(0),
                        output_tokens: usage.get("output_tokens").and_then(|v| v.as_i64()).unwrap_or(0),
                    });
                }
                // message_stop 이벤트의 amazon/anthropic 형식
                if json.get("type").and_then(|v| v.as_str()) == Some("message_stop") {
                    if let Some(message) = json.get("message") {
                        if let Some(usage) = message.get("usage") {
                            return Some(UsageInfo {
                                input_tokens: usage.get("input_tokens").and_then(|v| v.as_i64()).unwrap_or(0),
                                output_tokens: usage.get("output_tokens").and_then(|v| v.as_i64()).unwrap_or(0),
                            });
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
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

#[derive(Clone)]
struct ProxyState {
    db: Arc<Database>,
    client: reqwest::Client,
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

        Self {
            db,
            client,
            shutdown_tx: None,
        }
    }

    pub async fn start(&mut self, port: u16) -> Result<()> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.shutdown_tx = Some(tx);

        let state = ProxyState {
            db: self.db.clone(),
            client: self.client.clone(),
            db_task_semaphore: Arc::new(Semaphore::new(MAX_CONCURRENT_DB_TASKS)),
        };

        let app = Router::new()
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

    // 바디 추가 (Content-Length 명시적 설정)
    let body_vec = body_bytes.to_vec();
    if !body_vec.is_empty() {
        request_builder = request_builder
            .header("content-length", body_vec.len().to_string())
            .body(body_vec);
    }

    // 요청 전송
    let response = request_builder
        .send()
        .await
        .map_err(|e| {
            tracing::error!("Proxy request failed: {}", e);
            StatusCode::BAD_GATEWAY
        })?;

    tracing::info!("PROXY RESPONSE: {}", response.status());

    // 응답 상태 및 헤더
    let status = response.status();

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

    let body_stream = response.bytes_stream();

    // 스트림을 래핑하여 사용량 정보 추출
    let wrapped_stream = body_stream.map(move |chunk_result| {
        if let Ok(ref chunk) = chunk_result {
            // SSE 데이터에서 usage 추출 시도
            if let Ok(text) = std::str::from_utf8(chunk) {
                if let Some(usage) = parse_usage_from_sse(text) {
                    tracing::info!(
                        "USAGE: in={}, out={}",
                        usage.input_tokens,
                        usage.output_tokens
                    );

                    // DB에 사용량 로깅 (비동기로 처리, Semaphore로 동시 실행 제한)
                    let db_clone = db.clone();
                    let account_id_clone = account_id.clone();
                    let model_clone = model.clone();
                    let session_id_clone = session_id_for_log.clone();
                    let sem = semaphore.clone();

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
                                usage.input_tokens,
                                usage.output_tokens,
                                session_id_clone.as_deref(),
                            )
                            .await
                        {
                            tracing::error!("Failed to log usage: {}", e);
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
