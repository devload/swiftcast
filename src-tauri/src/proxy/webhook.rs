use reqwest::Client;
use serde::Serialize;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

#[derive(Debug, Serialize)]
pub struct WebhookPayload {
    pub event: String,
    pub todo_id: Option<String>,
    pub session_id: String,
    pub timestamp: i64,
    pub data: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct UsageData {
    pub model: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_summary: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AIQuestionData {
    pub question: String,
    pub options: Vec<String>,
    pub context: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct StepUpdateData {
    pub step_type: String,      // ANALYSIS, DESIGN, IMPLEMENTATION, VERIFICATION
    pub status: String,         // PENDING, IN_PROGRESS, COMPLETED
    pub progress: Option<i32>,  // 0-100
    pub message: Option<String>,
    pub tool_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionCompleteData {
    pub stop_reason: String,           // end_turn, tool_use, max_tokens
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub duration_ms: u64,
    pub completed_steps: Vec<String>,  // List of completed step types
}

pub struct WebhookClient {
    client: Client,
    base_url: Arc<RwLock<Option<String>>>,
    enabled: Arc<RwLock<bool>>,
}

impl WebhookClient {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(5))
            .connect_timeout(Duration::from_secs(3))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            base_url: Arc::new(RwLock::new(None)),
            enabled: Arc::new(RwLock::new(false)),
        }
    }

    /// Update webhook configuration
    pub async fn configure(&self, base_url: Option<String>, enabled: bool) {
        let mut url = self.base_url.write().await;
        *url = base_url;
        drop(url);

        let mut en = self.enabled.write().await;
        *en = enabled;

        tracing::info!("Webhook configured: enabled={}, url={:?}", enabled, self.base_url);
    }

    /// Check if webhook is enabled
    pub async fn is_enabled(&self) -> bool {
        let enabled = self.enabled.read().await;
        let url = self.base_url.read().await;
        *enabled && url.is_some()
    }

    /// Send usage data to ThreadCast
    pub async fn send_usage(
        &self,
        todo_id: Option<String>,
        session_id: &str,
        usage: UsageData,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if !self.is_enabled().await {
            return Ok(());
        }

        let payload = WebhookPayload {
            event: "usage_logged".to_string(),
            todo_id,
            session_id: session_id.to_string(),
            timestamp: chrono::Utc::now().timestamp(),
            data: serde_json::to_value(&usage)?,
        };

        self.send(&payload).await
    }

    /// Send AI question detection to ThreadCast
    pub async fn send_ai_question(
        &self,
        todo_id: Option<String>,
        session_id: &str,
        question: AIQuestionData,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if !self.is_enabled().await {
            return Ok(());
        }

        let payload = WebhookPayload {
            event: "ai_question_detected".to_string(),
            todo_id,
            session_id: session_id.to_string(),
            timestamp: chrono::Utc::now().timestamp(),
            data: serde_json::to_value(&question)?,
        };

        self.send(&payload).await
    }

    /// Send step update to ThreadCast
    pub async fn send_step_update(
        &self,
        todo_id: Option<String>,
        session_id: &str,
        step: StepUpdateData,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if !self.is_enabled().await {
            return Ok(());
        }

        let payload = WebhookPayload {
            event: "step_update".to_string(),
            todo_id,
            session_id: session_id.to_string(),
            timestamp: chrono::Utc::now().timestamp(),
            data: serde_json::to_value(&step)?,
        };

        self.send(&payload).await
    }

    /// Send session complete to ThreadCast (when stop_reason is "end_turn")
    pub async fn send_session_complete(
        &self,
        todo_id: Option<String>,
        session_id: &str,
        data: SessionCompleteData,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if !self.is_enabled().await {
            return Ok(());
        }

        tracing::info!(
            "Sending session_complete: todo={:?}, stop_reason={}",
            todo_id,
            data.stop_reason
        );

        let payload = WebhookPayload {
            event: "session_complete".to_string(),
            todo_id,
            session_id: session_id.to_string(),
            timestamp: chrono::Utc::now().timestamp(),
            data: serde_json::to_value(&data)?,
        };

        self.send(&payload).await
    }

    async fn send(
        &self,
        payload: &WebhookPayload,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let base_url = self.base_url.read().await;
        let url = match base_url.as_ref() {
            Some(url) => format!("{}/api/webhooks/swiftcast", url),
            None => return Ok(()),
        };
        drop(base_url);

        // Non-blocking send - spawn task and don't wait
        let client = self.client.clone();
        let payload_json = serde_json::to_value(payload)?;
        let event = payload.event.clone();
        let todo_id = payload.todo_id.clone();

        tokio::spawn(async move {
            match client
                .post(&url)
                .json(&payload_json)
                .send()
                .await
            {
                Ok(response) => {
                    if response.status().is_success() {
                        tracing::debug!(
                            "Webhook sent: event={}, todo={:?}",
                            event,
                            todo_id
                        );
                    } else {
                        tracing::warn!(
                            "Webhook failed: status={}, event={}",
                            response.status(),
                            event
                        );
                    }
                }
                Err(e) => {
                    tracing::debug!("Webhook error (ignored): {}", e);
                }
            }
        });

        Ok(())
    }
}

impl Default for WebhookClient {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for WebhookClient {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            base_url: self.base_url.clone(),
            enabled: self.enabled.clone(),
        }
    }
}
