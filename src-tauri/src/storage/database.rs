use crate::models::{Account, SessionConfig, SessionDetail};
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
        // Use absolute path with sqlite: prefix
        let db_url = format!("sqlite:{}", db_path.to_str().unwrap());

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
                session_id TEXT,
                FOREIGN KEY (account_id) REFERENCES accounts(id) ON DELETE CASCADE
            )
            "#,
        )
        .execute(&pool)
        .await?;

        // 기존 테이블에 session_id 컬럼 추가 (마이그레이션)
        let _ = sqlx::query("ALTER TABLE usage_logs ADD COLUMN session_id TEXT")
            .execute(&pool)
            .await;

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
                ('proxy_port', '32080'),
                ('auto_start', 'true'),
                ('theme', 'system')
            "#,
        )
        .execute(&pool)
        .await?;

        // 기존 8080 포트를 32080으로 마이그레이션
        sqlx::query(
            r#"
            UPDATE config SET value = '32080' WHERE key = 'proxy_port' AND value = '8080'
            "#,
        )
        .execute(&pool)
        .await?;

        // 기존 auto_start false를 true로 마이그레이션
        sqlx::query(
            r#"
            UPDATE config SET value = 'true' WHERE key = 'auto_start' AND value = 'false'
            "#,
        )
        .execute(&pool)
        .await?;

        // 세션 설정 테이블 생성
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS session_config (
                session_id TEXT PRIMARY KEY,
                account_id TEXT NOT NULL,
                model_override TEXT,
                last_message TEXT,
                created_at INTEGER NOT NULL,
                last_activity_at INTEGER NOT NULL,
                FOREIGN KEY (account_id) REFERENCES accounts(id) ON DELETE CASCADE
            )
            "#,
        )
        .execute(&pool)
        .await?;

        // 기존 테이블에 last_message 컬럼 추가 (마이그레이션)
        let _ = sqlx::query("ALTER TABLE session_config ADD COLUMN last_message TEXT")
            .execute(&pool)
            .await;

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

    // 사용량 로깅
    pub async fn log_usage(
        &self,
        account_id: &str,
        model: &str,
        input_tokens: i64,
        output_tokens: i64,
        session_id: Option<&str>,
    ) -> Result<()> {
        let timestamp = chrono::Utc::now().timestamp();

        sqlx::query(
            r#"
            INSERT INTO usage_logs (timestamp, account_id, model, input_tokens, output_tokens, cost_usd, duration_ms, status_code, session_id)
            VALUES (?, ?, ?, ?, ?, 0, 0, 200, ?)
            "#,
        )
        .bind(timestamp)
        .bind(account_id)
        .bind(model)
        .bind(input_tokens)
        .bind(output_tokens)
        .bind(session_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    // 사용량 통계 조회
    pub async fn get_usage_stats(&self) -> Result<(i64, i64, i64)> {
        let row = sqlx::query(
            r#"
            SELECT
                COUNT(*) as request_count,
                COALESCE(SUM(input_tokens), 0) as total_input_tokens,
                COALESCE(SUM(output_tokens), 0) as total_output_tokens
            FROM usage_logs
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        let request_count: i64 = row.try_get("request_count")?;
        let input_tokens: i64 = row.try_get("total_input_tokens")?;
        let output_tokens: i64 = row.try_get("total_output_tokens")?;

        Ok((request_count, input_tokens, output_tokens))
    }

    // 설정값 조회
    pub async fn get_config(&self, key: &str) -> Result<Option<String>> {
        let row = sqlx::query("SELECT value FROM config WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.map(|r| r.try_get("value").unwrap_or_default()))
    }

    // 설정값 저장
    pub async fn set_config(&self, key: &str, value: &str) -> Result<()> {
        sqlx::query("INSERT OR REPLACE INTO config (key, value) VALUES (?, ?)")
            .bind(key)
            .bind(value)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // 프록시 포트 조회
    pub async fn get_proxy_port(&self) -> Result<u16> {
        let port_str = self.get_config("proxy_port").await?.unwrap_or_else(|| "32080".to_string());
        Ok(port_str.parse().unwrap_or(32080))
    }

    // 자동 시작 설정 조회
    pub async fn get_auto_start(&self) -> Result<bool> {
        let auto_start = self.get_config("auto_start").await?.unwrap_or_else(|| "true".to_string());
        Ok(auto_start == "true")
    }

    // 최근 사용량 로그 조회
    pub async fn get_recent_usage(&self, limit: i64) -> Result<Vec<UsageLog>> {
        let rows = sqlx::query_as::<_, UsageLog>(
            r#"
            SELECT id, timestamp, account_id, model, input_tokens, output_tokens, cost_usd, duration_ms, status_code, session_id
            FROM usage_logs
            ORDER BY timestamp DESC
            LIMIT ?
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }

    // 계정별 사용량 통계
    pub async fn get_usage_by_account(&self) -> Result<Vec<AccountUsageStats>> {
        let rows = sqlx::query_as::<_, AccountUsageStats>(
            r#"
            SELECT
                account_id,
                COUNT(*) as request_count,
                COALESCE(SUM(input_tokens), 0) as total_input_tokens,
                COALESCE(SUM(output_tokens), 0) as total_output_tokens
            FROM usage_logs
            GROUP BY account_id
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }

    // 모델별 사용량 통계
    pub async fn get_usage_by_model(&self) -> Result<Vec<ModelUsageStats>> {
        let rows = sqlx::query_as::<_, ModelUsageStats>(
            r#"
            SELECT
                model,
                COUNT(*) as request_count,
                COALESCE(SUM(input_tokens), 0) as total_input_tokens,
                COALESCE(SUM(output_tokens), 0) as total_output_tokens
            FROM usage_logs
            GROUP BY model
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }

    // 일별 사용량 통계 (최근 N일)
    pub async fn get_daily_usage(&self, days: i64) -> Result<Vec<DailyUsageStats>> {
        let rows = sqlx::query_as::<_, DailyUsageStats>(
            r#"
            SELECT
                date(timestamp, 'unixepoch', 'localtime') as date,
                COUNT(*) as request_count,
                COALESCE(SUM(input_tokens), 0) as total_input_tokens,
                COALESCE(SUM(output_tokens), 0) as total_output_tokens
            FROM usage_logs
            WHERE timestamp > unixepoch() - (? * 86400)
            GROUP BY date
            ORDER BY date DESC
            "#,
        )
        .bind(days)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }

    // 사용량 로그 초기화
    pub async fn clear_usage_logs(&self) -> Result<()> {
        sqlx::query("DELETE FROM usage_logs")
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // 세션별 사용량 통계
    pub async fn get_usage_by_session(&self) -> Result<Vec<SessionUsageStats>> {
        let rows = sqlx::query_as::<_, SessionUsageStats>(
            r#"
            SELECT
                COALESCE(session_id, 'unknown') as session_id,
                MIN(timestamp) as first_request,
                MAX(timestamp) as last_request,
                COUNT(*) as request_count,
                COALESCE(SUM(input_tokens), 0) as total_input_tokens,
                COALESCE(SUM(output_tokens), 0) as total_output_tokens
            FROM usage_logs
            WHERE session_id IS NOT NULL
            GROUP BY session_id
            ORDER BY last_request DESC
            LIMIT 50
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }

    // 특정 계정 조회
    pub async fn get_account(&self, account_id: &str) -> Result<Option<Account>> {
        let account = sqlx::query_as::<_, Account>(
            "SELECT id, name, base_url, created_at, is_active FROM accounts WHERE id = ?"
        )
        .bind(account_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(account)
    }

    // ===== 세션 설정 관리 =====

    // 세션 설정 조회
    pub async fn get_session_config(&self, session_id: &str) -> Result<Option<SessionConfig>> {
        let config = sqlx::query_as::<_, SessionConfig>(
            "SELECT session_id, account_id, model_override, last_message, created_at, last_activity_at FROM session_config WHERE session_id = ?"
        )
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(config)
    }

    // 세션 설정 생성/업데이트
    pub async fn upsert_session_config(
        &self,
        session_id: &str,
        account_id: &str,
        model_override: Option<&str>,
    ) -> Result<()> {
        let now = chrono::Utc::now().timestamp();

        sqlx::query(
            r#"
            INSERT INTO session_config (session_id, account_id, model_override, last_message, created_at, last_activity_at)
            VALUES (?, ?, ?, NULL, ?, ?)
            ON CONFLICT(session_id) DO UPDATE SET
                account_id = excluded.account_id,
                model_override = excluded.model_override,
                last_activity_at = excluded.last_activity_at
            "#,
        )
        .bind(session_id)
        .bind(account_id)
        .bind(model_override)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    // 세션 활동 시간 및 마지막 메시지 업데이트
    pub async fn update_session_activity(&self, session_id: &str, last_message: Option<&str>) -> Result<()> {
        let now = chrono::Utc::now().timestamp();

        if let Some(msg) = last_message {
            // 메시지가 있으면 함께 업데이트
            sqlx::query("UPDATE session_config SET last_activity_at = ?, last_message = ? WHERE session_id = ?")
                .bind(now)
                .bind(msg)
                .bind(session_id)
                .execute(&self.pool)
                .await?;
        } else {
            // 메시지가 없으면 시간만 업데이트
            sqlx::query("UPDATE session_config SET last_activity_at = ? WHERE session_id = ?")
                .bind(now)
                .bind(session_id)
                .execute(&self.pool)
                .await?;
        }

        Ok(())
    }

    // 활성 세션 목록 조회 (최근 24시간 + 사용량 통계 포함)
    pub async fn get_active_sessions(&self) -> Result<Vec<SessionDetail>> {
        let cutoff = chrono::Utc::now().timestamp() - (24 * 60 * 60); // 24시간 전

        let rows = sqlx::query(
            r#"
            SELECT
                sc.session_id,
                sc.account_id,
                a.name as account_name,
                sc.model_override,
                sc.last_message,
                sc.created_at,
                sc.last_activity_at,
                COALESCE(ul.request_count, 0) as request_count,
                COALESCE(ul.total_input_tokens, 0) as total_input_tokens,
                COALESCE(ul.total_output_tokens, 0) as total_output_tokens
            FROM session_config sc
            LEFT JOIN accounts a ON sc.account_id = a.id
            LEFT JOIN (
                SELECT
                    session_id,
                    COUNT(*) as request_count,
                    SUM(input_tokens) as total_input_tokens,
                    SUM(output_tokens) as total_output_tokens
                FROM usage_logs
                GROUP BY session_id
            ) ul ON sc.session_id = ul.session_id
            WHERE sc.last_activity_at > ?
            ORDER BY sc.last_activity_at DESC
            "#,
        )
        .bind(cutoff)
        .fetch_all(&self.pool)
        .await?;

        let sessions = rows
            .iter()
            .map(|row| SessionDetail {
                session_id: row.try_get("session_id").unwrap_or_default(),
                account_id: row.try_get("account_id").unwrap_or_default(),
                account_name: row.try_get("account_name").unwrap_or_else(|_| "Unknown".to_string()),
                model_override: row.try_get("model_override").ok(),
                last_message: row.try_get("last_message").ok(),
                created_at: row.try_get("created_at").unwrap_or(0),
                last_activity_at: row.try_get("last_activity_at").unwrap_or(0),
                request_count: row.try_get("request_count").unwrap_or(0),
                total_input_tokens: row.try_get("total_input_tokens").unwrap_or(0),
                total_output_tokens: row.try_get("total_output_tokens").unwrap_or(0),
            })
            .collect();

        Ok(sessions)
    }

    // 세션 설정 삭제
    pub async fn delete_session_config(&self, session_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM session_config WHERE session_id = ?")
            .bind(session_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }
}

// 사용량 로그 모델
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct UsageLog {
    pub id: i64,
    pub timestamp: i64,
    pub account_id: String,
    pub model: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cost_usd: f64,
    pub duration_ms: i64,
    pub status_code: i64,
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct AccountUsageStats {
    pub account_id: String,
    pub request_count: i64,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct ModelUsageStats {
    pub model: String,
    pub request_count: i64,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct DailyUsageStats {
    pub date: String,
    pub request_count: i64,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct SessionUsageStats {
    pub session_id: String,
    pub first_request: i64,
    pub last_request: i64,
    pub request_count: i64,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
}
