use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Account {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub created_at: i64,
    pub is_active: bool,
}

impl Account {
    pub fn new(name: String, base_url: String) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            base_url,
            created_at: chrono::Utc::now().timestamp(),
            is_active: false,
        }
    }
}
