use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Context for an incoming request
#[derive(Debug, Clone)]
pub struct RequestContext {
    /// Unique request identifier
    pub request_id: String,
    /// Session ID (from sentry-trace or x-session-id header)
    pub session_id: Option<String>,
    /// Model being used
    pub model: String,
    /// HTTP method
    pub method: String,
    /// Request path
    pub path: String,
    /// Full request body (JSON)
    pub body: serde_json::Value,
    /// Request timestamp (Unix epoch seconds)
    pub timestamp: i64,
    /// ISO8601 formatted timestamp
    pub timestamp_iso: String,
}

impl RequestContext {
    pub fn new(
        session_id: Option<String>,
        model: String,
        method: String,
        path: String,
        body: serde_json::Value,
    ) -> Self {
        let now = chrono::Utc::now();
        Self {
            request_id: uuid::Uuid::new_v4().to_string(),
            session_id,
            model,
            method,
            path,
            body,
            timestamp: now.timestamp(),
            timestamp_iso: now.to_rfc3339(),
        }
    }

    /// Get short session ID (first 16 chars) for directory naming
    pub fn short_session_id(&self) -> Option<String> {
        self.session_id.as_ref().map(|s| {
            s.chars().take(16).collect()
        })
    }
}

/// Context for a response
#[derive(Debug, Clone, Default)]
pub struct ResponseContext {
    /// HTTP status code
    pub status_code: u16,
    /// Request duration in milliseconds
    pub duration_ms: u64,
    /// Input tokens used
    pub input_tokens: i64,
    /// Output tokens used
    pub output_tokens: i64,
    /// Whether the request was successful
    pub is_success: bool,
    /// Error message if failed
    pub error_message: Option<String>,
    /// Full response text (accumulated from streaming)
    pub response_text: String,
    /// Response timestamp (Unix epoch seconds)
    pub timestamp: i64,
    /// Stop reason (end_turn, tool_use, max_tokens, etc.)
    pub stop_reason: Option<String>,
}

impl ResponseContext {
    pub fn new(status_code: u16) -> Self {
        Self {
            status_code,
            is_success: status_code >= 200 && status_code < 300,
            timestamp: chrono::Utc::now().timestamp(),
            ..Default::default()
        }
    }

    pub fn with_duration(mut self, duration_ms: u64) -> Self {
        self.duration_ms = duration_ms;
        self
    }

    pub fn with_tokens(mut self, input: i64, output: i64) -> Self {
        self.input_tokens = input;
        self.output_tokens = output;
        self
    }

    pub fn with_error(mut self, msg: String) -> Self {
        self.error_message = Some(msg);
        self.is_success = false;
        self
    }
}

/// Builder for accumulating response data during streaming
#[derive(Debug, Clone)]
pub struct ResponseBuilder {
    inner: Arc<RwLock<ResponseBuilderInner>>,
}

#[derive(Debug, Default)]
struct ResponseBuilderInner {
    status_code: u16,
    start_time: Option<std::time::Instant>,
    input_tokens: i64,
    output_tokens: i64,
    response_text: String,
    error_message: Option<String>,
    stop_reason: Option<String>,
}

impl ResponseBuilder {
    pub fn new(status_code: u16) -> Self {
        Self {
            inner: Arc::new(RwLock::new(ResponseBuilderInner {
                status_code,
                start_time: Some(std::time::Instant::now()),
                ..Default::default()
            })),
        }
    }

    pub async fn append_text(&self, text: &str) {
        let mut inner = self.inner.write().await;
        inner.response_text.push_str(text);
    }

    pub async fn set_tokens(&self, input: i64, output: i64) {
        let mut inner = self.inner.write().await;
        inner.input_tokens = input;
        inner.output_tokens = output;
    }

    pub async fn set_error(&self, msg: String) {
        let mut inner = self.inner.write().await;
        inner.error_message = Some(msg);
    }

    pub async fn set_stop_reason(&self, reason: String) {
        let mut inner = self.inner.write().await;
        inner.stop_reason = Some(reason);
    }

    pub async fn build(&self) -> ResponseContext {
        let inner = self.inner.read().await;
        let duration_ms = inner.start_time
            .map(|s| s.elapsed().as_millis() as u64)
            .unwrap_or(0);

        ResponseContext {
            status_code: inner.status_code,
            duration_ms,
            input_tokens: inner.input_tokens,
            output_tokens: inner.output_tokens,
            is_success: inner.status_code >= 200 && inner.status_code < 300 && inner.error_message.is_none(),
            error_message: inner.error_message.clone(),
            response_text: inner.response_text.clone(),
            timestamp: chrono::Utc::now().timestamp(),
            stop_reason: inner.stop_reason.clone(),
        }
    }
}

/// JSON structure for hook log files
#[derive(Debug, Serialize, Deserialize)]
pub struct HookLogEntry {
    pub request_id: String,
    pub session_id: String,
    pub request: RequestLogData,
    pub response: ResponseLogData,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RequestLogData {
    pub timestamp: i64,
    pub timestamp_iso: String,
    pub model: String,
    pub method: String,
    pub path: String,
    pub body: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ResponseLogData {
    pub timestamp: i64,
    pub status_code: u16,
    pub duration_ms: u64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub is_success: bool,
    pub error_message: Option<String>,
    pub response_text: String,
    pub stop_reason: Option<String>,
}

impl HookLogEntry {
    pub fn from_contexts(req: &RequestContext, res: &ResponseContext) -> Self {
        Self {
            request_id: req.request_id.clone(),
            session_id: req.session_id.clone().unwrap_or_else(|| "unknown".to_string()),
            request: RequestLogData {
                timestamp: req.timestamp,
                timestamp_iso: req.timestamp_iso.clone(),
                model: req.model.clone(),
                method: req.method.clone(),
                path: req.path.clone(),
                body: req.body.clone(),
            },
            response: ResponseLogData {
                timestamp: res.timestamp,
                status_code: res.status_code,
                duration_ms: res.duration_ms,
                input_tokens: res.input_tokens,
                output_tokens: res.output_tokens,
                is_success: res.is_success,
                error_message: res.error_message.clone(),
                response_text: res.response_text.clone(),
                stop_reason: res.stop_reason.clone(),
            },
        }
    }
}
