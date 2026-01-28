use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::sync::RwLock;

use super::context::RequestContext;
use super::traits::ModifyHook;

/// Configuration for compaction injection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionConfig {
    /// Whether injection is enabled
    pub enabled: bool,
    /// Instructions to add to summarization prompt
    pub summarization_instructions: Option<String>,
    /// Context to inject into compacted conversation
    pub context_injection: Option<String>,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            summarization_instructions: None,
            context_injection: None,
        }
    }
}

/// Hook for injecting content during conversation compaction
pub struct CompactionInjectorHook {
    config: RwLock<CompactionConfig>,
    config_path: PathBuf,
}

impl CompactionInjectorHook {
    pub fn new(config_path: PathBuf) -> Self {
        let config = Self::load_config(&config_path).unwrap_or_default();
        Self {
            config: RwLock::new(config),
            config_path,
        }
    }

    fn load_config(path: &PathBuf) -> Option<CompactionConfig> {
        let content = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&content).ok()
    }

    pub async fn update_config(&self, config: CompactionConfig) -> std::io::Result<()> {
        // Save to file
        let content = serde_json::to_string_pretty(&config)?;
        std::fs::write(&self.config_path, content)?;

        // Update in memory
        let mut cfg = self.config.write().await;
        *cfg = config;

        tracing::info!("CompactionInjector config updated");
        Ok(())
    }

    pub async fn get_config(&self) -> CompactionConfig {
        self.config.read().await.clone()
    }

    /// Check if this is a compaction summarization request
    fn is_compaction_request(body: &str) -> bool {
        body.contains("Your task is to create a detailed summary of the conversation")
    }

    /// Check if this is a compacted conversation (post-compaction)
    fn is_compacted_conversation(body: &str) -> bool {
        body.contains("This session is being continued from a previous conversation that ran out of context")
    }

    /// Inject instructions into summarization prompt
    fn inject_summarization_instructions(body: &str, instructions: &str) -> String {
        // Find the end of summarization instructions and inject before it
        let marker = "Please provide your summary based on the conversation so far";

        if let Some(pos) = body.find(marker) {
            let injection = format!(
                "\n\n## Additional Summarization Instructions (IMPORTANT - Must be included in summary):\n{}\n\n",
                instructions
            );
            let mut result = body.to_string();
            result.insert_str(pos, &injection);
            result
        } else {
            // Fallback: append to the end of the body
            format!("{}\n\n## Additional Instructions:\n{}", body, instructions)
        }
    }

    /// Inject context into compacted conversation
    fn inject_context(body: &str, context: &str) -> String {
        // Find the summary section and inject context
        let marker = "This session is being continued from a previous conversation that ran out of context.";

        if let Some(pos) = body.find(marker) {
            let after_marker = pos + marker.len();
            let injection = format!(
                "\n\n## Persistent Context (Always Remember):\n{}\n",
                context
            );
            let mut result = body.to_string();
            result.insert_str(after_marker, &injection);
            result
        } else {
            body.to_string()
        }
    }
}

#[async_trait]
impl ModifyHook for CompactionInjectorHook {
    async fn modify_request_body(&self, body: &str, ctx: &RequestContext) -> Option<String> {
        let config = self.config.read().await;

        if !config.enabled {
            return None;
        }

        // Case 1: Summarization request - inject instructions for summary generation
        if Self::is_compaction_request(body) {
            if let Some(ref instructions) = config.summarization_instructions {
                tracing::info!(
                    "[CompactionInjector] Detected summarization request (req_id: {}), injecting instructions",
                    ctx.request_id
                );
                return Some(Self::inject_summarization_instructions(body, instructions));
            }
        }

        // Case 2: Compacted conversation - inject persistent context
        if Self::is_compacted_conversation(body) {
            if let Some(ref context) = config.context_injection {
                tracing::info!(
                    "[CompactionInjector] Detected compacted conversation (req_id: {}), injecting context",
                    ctx.request_id
                );
                return Some(Self::inject_context(body, context));
            }
        }

        None
    }

    async fn modify_response_text(&self, _text: &str, _ctx: &RequestContext) -> Option<String> {
        // Not modifying responses for now
        None
    }

    fn name(&self) -> &'static str {
        "CompactionInjectorHook"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_compaction_request() {
        let body = r#"{"messages":[{"content":"Your task is to create a detailed summary of the conversation"}]}"#;
        assert!(CompactionInjectorHook::is_compaction_request(body));

        let body = r#"{"messages":[{"content":"Hello world"}]}"#;
        assert!(!CompactionInjectorHook::is_compaction_request(body));
    }

    #[test]
    fn test_is_compacted_conversation() {
        let body = r#"This session is being continued from a previous conversation that ran out of context."#;
        assert!(CompactionInjectorHook::is_compacted_conversation(body));
    }

    #[test]
    fn test_inject_summarization_instructions() {
        let body = "Some text here. Please provide your summary based on the conversation so far, following this structure.";
        let instructions = "Always include Korean language preference.";

        let result = CompactionInjectorHook::inject_summarization_instructions(body, instructions);

        assert!(result.contains("Additional Summarization Instructions"));
        assert!(result.contains("Always include Korean language preference"));
        assert!(result.contains("Please provide your summary")); // Original marker still exists
    }

    #[test]
    fn test_inject_context() {
        let body = "This session is being continued from a previous conversation that ran out of context. Summary follows...";
        let context = "User prefers Korean responses.";

        let result = CompactionInjectorHook::inject_context(body, context);

        assert!(result.contains("Persistent Context"));
        assert!(result.contains("User prefers Korean responses"));
    }
}
