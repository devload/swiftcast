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
use tower_http::cors::CorsLayer;

// 요청에서 모델 정보 추출
#[derive(Debug, Clone, Default)]
struct RequestInfo {
    model: String,
    stream: bool,
}

// 응답에서 사용량 정보 추출
#[derive(Debug, Clone, Default)]
struct UsageInfo {
    input_tokens: i64,
    output_tokens: i64,
}

fn parse_request_info(body: &[u8]) -> RequestInfo {
    if let Ok(json) = serde_json::from_slice::<serde_json::Value>(body) {
        RequestInfo {
            model: json.get("model").and_then(|v| v.as_str()).unwrap_or("unknown").to_string(),
            stream: json.get("stream").and_then(|v| v.as_bool()).unwrap_or(true),
        }
    } else {
        RequestInfo::default()
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
}

impl ProxyServer {
    pub fn new(db: Arc<Database>) -> Self {
        Self {
            db,
            client: reqwest::Client::new(),
            shutdown_tx: None,
        }
    }

    pub async fn start(&mut self, port: u16) -> Result<()> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.shutdown_tx = Some(tx);

        let state = ProxyState {
            db: self.db.clone(),
            client: self.client.clone(),
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
    // 활성 계정 가져오기
    let account = state
        .db
        .get_active_account()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    // API 키 로드
    let api_key = state
        .db
        .get_api_key(&account.id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // 요청 경로 (소유권 이전 전에 복사)
    let path = req.uri().path().to_string();
    let query = req.uri().query().unwrap_or("").to_string();
    let full_path = if query.is_empty() {
        path.clone()
    } else {
        format!("{}?{}", path, query)
    };

    // 타겟 URL 생성
    let target_url = format!("{}{}", account.base_url, full_path);

    // 원본 헤더 저장
    let original_headers = req.headers().clone();

    // 요청 바디 읽기
    let body_bytes = axum::body::to_bytes(req.into_body(), usize::MAX)
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    // 요청 정보 파싱 (모델, 스트리밍 여부)
    let request_info = parse_request_info(&body_bytes);

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

                    // DB에 사용량 로깅 (비동기로 처리)
                    let db_clone = db.clone();
                    let account_id_clone = account_id.clone();
                    let model_clone = model.clone();
                    let session_id_clone = session_id_for_log.clone();
                    tokio::spawn(async move {
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
