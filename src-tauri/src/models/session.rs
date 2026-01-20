use serde::{Deserialize, Serialize};

/// 세션별 설정 (DB 저장용)
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SessionConfig {
    pub session_id: String,
    pub account_id: String,
    pub model_override: Option<String>,
    pub last_message: Option<String>,
    pub created_at: i64,
    pub last_activity_at: i64,
}

/// 세션 상세 정보 (UI 표시용)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionDetail {
    pub session_id: String,
    pub account_id: String,
    pub account_name: String,
    pub model_override: Option<String>,
    pub last_message: Option<String>,
    pub created_at: i64,
    pub last_activity_at: i64,
    pub request_count: i64,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
}
