use crate::models::Account;
use anyhow::Result;
use sqlx::{sqlite::SqlitePool, Row};
use std::path::PathBuf;

pub struct Database {
    pool: SqlitePool,
}

impl Database {
    pub async fn init() -> Result<Self> {
        // 데이터베이스 파일 경로
        let app_data_dir = Self::get_app_data_dir()?;
        std::fs::create_dir_all(&app_data_dir)?;

        let db_path = app_data_dir.join("data.db");
        let db_url = format!("sqlite:{}", db_path.display());

        // 연결 풀 생성
        let pool = SqlitePool::connect(&db_url).await?;

        // 테이블 생성
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS accounts (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                base_url TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                is_active INTEGER DEFAULT 0
            )
            "#,
        )
        .execute(&pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS usage_logs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp INTEGER NOT NULL,
                account_id TEXT NOT NULL,
                model TEXT NOT NULL,
                input_tokens INTEGER NOT NULL,
                output_tokens INTEGER NOT NULL,
                cost_usd REAL NOT NULL,
                duration_ms INTEGER NOT NULL,
                request_path TEXT,
                status_code INTEGER NOT NULL,
                error_message TEXT,
                FOREIGN KEY (account_id) REFERENCES accounts(id) ON DELETE CASCADE
            )
            "#,
        )
        .execute(&pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS config (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )
            "#,
        )
        .execute(&pool)
        .await?;

        // 초기 설정값
        sqlx::query(
            r#"
            INSERT OR IGNORE INTO config (key, value) VALUES
                ('proxy_port', '8080'),
                ('auto_start', 'false'),
                ('theme', 'system')
            "#,
        )
        .execute(&pool)
        .await?;

        Ok(Self { pool })
    }

    fn get_app_data_dir() -> Result<PathBuf> {
        let app_data = std::env::var("APPDATA")
            .or_else(|_| std::env::var("HOME").map(|h| format!("{}/.config", h)))?;
        Ok(PathBuf::from(app_data).join("com.swiftcast.app"))
    }

    pub async fn create_account(&self, account: Account, api_key: String) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO accounts (id, name, base_url, created_at, is_active)
            VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(&account.id)
        .bind(&account.name)
        .bind(&account.base_url)
        .bind(account.created_at)
        .bind(account.is_active)
        .execute(&self.pool)
        .await?;

        // API 키 저장
        self.save_api_key(&account.id, &api_key)?;

        Ok(())
    }

    pub async fn get_accounts(&self) -> Result<Vec<Account>> {
        let accounts = sqlx::query_as::<_, Account>(
            "SELECT id, name, base_url, created_at, is_active FROM accounts ORDER BY created_at DESC"
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(accounts)
    }

    pub async fn get_active_account(&self) -> Result<Option<Account>> {
        let account = sqlx::query_as::<_, Account>(
            "SELECT id, name, base_url, created_at, is_active FROM accounts WHERE is_active = 1"
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(account)
    }

    pub async fn switch_account(&self, account_id: &str) -> Result<()> {
        // 모든 계정 비활성화
        sqlx::query("UPDATE accounts SET is_active = 0")
            .execute(&self.pool)
            .await?;

        // 선택된 계정 활성화
        sqlx::query("UPDATE accounts SET is_active = 1 WHERE id = ?")
            .bind(account_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    pub async fn delete_account(&self, account_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM accounts WHERE id = ?")
            .bind(account_id)
            .execute(&self.pool)
            .await?;

        // API 키 삭제
        self.delete_api_key(account_id)?;

        Ok(())
    }

    // API 키 관리 (JSON 파일)
    fn get_api_keys_path(&self) -> Result<PathBuf> {
        let app_data_dir = Self::get_app_data_dir()?;
        Ok(app_data_dir.join(".api_keys.json"))
    }

    fn load_api_keys(&self) -> Result<serde_json::Value> {
        let path = self.get_api_keys_path()?;
        if !path.exists() {
            return Ok(serde_json::json!({}));
        }
        let content = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&content)?)
    }

    fn save_api_keys(&self, keys: &serde_json::Value) -> Result<()> {
        let path = self.get_api_keys_path()?;
        std::fs::write(path, serde_json::to_string_pretty(keys)?)?;
        Ok(())
    }

    pub fn save_api_key(&self, account_id: &str, api_key: &str) -> Result<()> {
        let mut keys = self.load_api_keys()?;
        keys[account_id] = serde_json::Value::String(api_key.to_string());
        self.save_api_keys(&keys)?;
        Ok(())
    }

    pub fn get_api_key(&self, account_id: &str) -> Result<String> {
        let keys = self.load_api_keys()?;
        keys[account_id]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("API key not found for account {}", account_id))
    }

    fn delete_api_key(&self, account_id: &str) -> Result<()> {
        let mut keys = self.load_api_keys()?;
        if let Some(obj) = keys.as_object_mut() {
            obj.remove(account_id);
        }
        self.save_api_keys(&keys)?;
        Ok(())
    }
}
