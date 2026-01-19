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
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::cors::CorsLayer;

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

    // 요청 경로
    let path = req.uri().path();
    let query = req.uri().query().unwrap_or("");
    let full_path = if query.is_empty() {
        path.to_string()
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

    // 원본 요청에 인증 헤더가 있는지 확인
    let has_auth = original_headers.contains_key("x-api-key")
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

    // 바디 추가
    if !body_bytes.is_empty() {
        request_builder = request_builder.body(body_bytes);
    }

    // 요청 로깅
    tracing::info!("=== PROXY REQUEST ===");
    tracing::info!("Method: {:?}, Target: {}", method.as_str(), target_url);
    tracing::info!("Is Anthropic: {}, Using stored key: {}", is_anthropic, !is_anthropic && !api_key.is_empty());

    // 요청 전송
    let response = request_builder
        .send()
        .await
        .map_err(|e| {
            tracing::error!("Proxy request failed: {}", e);
            StatusCode::BAD_GATEWAY
        })?;

    tracing::info!("=== PROXY RESPONSE ===");
    tracing::info!("Status: {}", response.status());

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

    // 응답 바디 스트리밍
    let body_stream = response.bytes_stream();
    let body = Body::from_stream(body_stream);

    // 응답 반환
    Ok(builder.body(body).unwrap())
}
