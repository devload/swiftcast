use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use super::context::{HookLogEntry, RequestContext, ResponseContext};
use super::traits::Hook;

const DEFAULT_RETENTION_DAYS: u64 = 30;

/// File logging hook that saves request/response data to JSON files
pub struct FileLoggerHook {
    /// Base directory for log files
    log_dir: PathBuf,
    /// Retention period in days
    retention_days: u64,
    /// Whether the hook is enabled
    enabled: Arc<RwLock<bool>>,
}

impl FileLoggerHook {
    pub fn new(log_dir: PathBuf) -> Self {
        Self {
            log_dir,
            retention_days: DEFAULT_RETENTION_DAYS,
            enabled: Arc::new(RwLock::new(true)),
        }
    }

    pub fn with_retention_days(mut self, days: u64) -> Self {
        self.retention_days = days;
        self
    }

    /// Get the default log directory
    pub fn default_log_dir() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join(".sessioncast").join("logs")
    }

    /// Enable or disable the hook
    pub async fn set_enabled(&self, enabled: bool) {
        let mut e = self.enabled.write().await;
        *e = enabled;
    }

    /// Check if the hook is enabled
    async fn is_enabled(&self) -> bool {
        *self.enabled.read().await
    }

    /// Get the session directory for a given session ID
    fn get_session_dir(&self, session_id: &str) -> PathBuf {
        // Use first 16 chars of session ID for directory name
        let short_id: String = session_id.chars().take(16).collect();
        self.log_dir.join(&short_id)
    }

    /// Generate log file name based on timestamp and request info
    fn generate_filename(&self, req_ctx: &RequestContext, request_num: u64) -> String {
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let short_request_id: String = req_ctx.request_id.chars().take(8).collect();
        format!("{}_{}_{}_{}.json",
            timestamp,
            short_request_id,
            request_num,
            // Sanitize model name for filename
            req_ctx.model.replace(['/', ':', '.'], "_")
        )
    }

    /// Write log entry to file
    async fn write_log(&self, req_ctx: &RequestContext, res_ctx: &ResponseContext) {
        let session_id = req_ctx.session_id.as_deref().unwrap_or("unknown");
        let session_dir = self.get_session_dir(session_id);

        // Ensure directory exists
        if let Err(e) = tokio::fs::create_dir_all(&session_dir).await {
            tracing::error!("Failed to create log directory {:?}: {}", session_dir, e);
            return;
        }

        // Count existing files to generate sequence number
        let request_num = match tokio::fs::read_dir(&session_dir).await {
            Ok(mut entries) => {
                let mut count = 0u64;
                while entries.next_entry().await.ok().flatten().is_some() {
                    count += 1;
                }
                count + 1
            }
            Err(_) => 1,
        };

        let filename = self.generate_filename(req_ctx, request_num);
        let filepath = session_dir.join(&filename);

        // Create log entry
        let entry = HookLogEntry::from_contexts(req_ctx, res_ctx);

        // Write to file
        match serde_json::to_string_pretty(&entry) {
            Ok(json) => {
                if let Err(e) = tokio::fs::write(&filepath, json).await {
                    tracing::error!("Failed to write log file {:?}: {}", filepath, e);
                } else {
                    tracing::debug!("Hook log written: {:?}", filepath);
                }
            }
            Err(e) => {
                tracing::error!("Failed to serialize log entry: {}", e);
            }
        }
    }

    /// Clean up old log files based on retention policy
    pub async fn cleanup_old_logs(&self) {
        let cutoff = chrono::Utc::now() - chrono::Duration::days(self.retention_days as i64);
        let cutoff_timestamp = cutoff.timestamp();

        let mut entries = match tokio::fs::read_dir(&self.log_dir).await {
            Ok(entries) => entries,
            Err(e) => {
                tracing::debug!("No log directory to clean: {}", e);
                return;
            }
        };

        let mut deleted_count = 0u64;
        let mut deleted_dirs = 0u64;

        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            // Check files in session directory
            let mut session_entries = match tokio::fs::read_dir(&path).await {
                Ok(e) => e,
                Err(_) => continue,
            };

            let mut session_file_count = 0u64;
            let mut session_deleted = 0u64;

            while let Ok(Some(file_entry)) = session_entries.next_entry().await {
                session_file_count += 1;
                let file_path = file_entry.path();

                // Check file modification time
                if let Ok(metadata) = tokio::fs::metadata(&file_path).await {
                    if let Ok(modified) = metadata.modified() {
                        let modified_timestamp = modified
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs() as i64)
                            .unwrap_or(0);

                        if modified_timestamp < cutoff_timestamp {
                            if let Err(e) = tokio::fs::remove_file(&file_path).await {
                                tracing::warn!("Failed to delete old log file {:?}: {}", file_path, e);
                            } else {
                                deleted_count += 1;
                                session_deleted += 1;
                            }
                        }
                    }
                }
            }

            // Remove empty session directories
            if session_file_count == session_deleted && session_file_count > 0 {
                if let Err(e) = tokio::fs::remove_dir(&path).await {
                    tracing::debug!("Failed to remove empty session dir {:?}: {}", path, e);
                } else {
                    deleted_dirs += 1;
                }
            }
        }

        if deleted_count > 0 || deleted_dirs > 0 {
            tracing::info!(
                "Hook log cleanup: deleted {} files and {} empty directories (retention: {} days)",
                deleted_count,
                deleted_dirs,
                self.retention_days
            );
        }
    }
}

#[async_trait]
impl Hook for FileLoggerHook {
    async fn on_request_before(&self, ctx: &RequestContext) {
        if !self.is_enabled().await {
            return;
        }
        tracing::debug!(
            "FileLoggerHook: request_before [{}] model={}",
            ctx.request_id,
            ctx.model
        );
    }

    async fn on_request_after(&self, _req_ctx: &RequestContext, _res_ctx: &ResponseContext) {
        // Not used - we use on_response_complete instead
    }

    async fn on_request_success(&self, _req_ctx: &RequestContext, _res_ctx: &ResponseContext) {
        // Handled in on_response_complete
    }

    async fn on_request_failed(&self, _req_ctx: &RequestContext, _res_ctx: &ResponseContext) {
        // Handled in on_response_complete
    }

    async fn on_response_chunk(&self, _req_ctx: &RequestContext, _chunk: &[u8]) {
        // We don't log individual chunks - accumulation happens in ResponseBuilder
    }

    async fn on_response_complete(&self, req_ctx: &RequestContext, res_ctx: &ResponseContext) {
        if !self.is_enabled().await {
            return;
        }

        tracing::debug!(
            "FileLoggerHook: response_complete [{}] status={} duration={}ms tokens=({},{})",
            req_ctx.request_id,
            res_ctx.status_code,
            res_ctx.duration_ms,
            res_ctx.input_tokens,
            res_ctx.output_tokens
        );

        // Write log file asynchronously
        self.write_log(req_ctx, res_ctx).await;
    }

    fn name(&self) -> &'static str {
        "FileLoggerHook"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_log_dir() {
        let dir = FileLoggerHook::default_log_dir();
        assert!(dir.to_string_lossy().contains(".sessioncast"));
        assert!(dir.to_string_lossy().ends_with("logs"));
    }

    #[test]
    fn test_session_dir() {
        let hook = FileLoggerHook::new(PathBuf::from("/tmp/logs"));
        let session_dir = hook.get_session_dir("abc123def456ghi789jkl");
        assert_eq!(session_dir, PathBuf::from("/tmp/logs/abc123def456ghi7"));
    }
}
