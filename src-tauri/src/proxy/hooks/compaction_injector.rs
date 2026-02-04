use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::sync::RwLock;

use super::context::RequestContext;
use super::context_provider::ContextProviderManager;
use super::traits::ModifyHook;

/// Configuration for compaction injection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionConfig {
    /// Whether injection is enabled
    pub enabled: bool,
    /// Instructions to add to summarization prompt
    pub summarization_instructions: Option<String>,
    /// Context to inject into compacted conversation (static)
    pub context_injection: Option<String>,
    /// Whether to fetch context from external providers
    #[serde(default = "default_providers_enabled")]
    pub context_providers_enabled: bool,
}

fn default_providers_enabled() -> bool {
    true
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            summarization_instructions: None,
            context_injection: None,
            context_providers_enabled: true,
        }
    }
}

/// Hook for injecting content during conversation compaction
pub struct CompactionInjectorHook {
    config: RwLock<CompactionConfig>,
    config_path: PathBuf,
    provider_manager: RwLock<ContextProviderManager>,
}

impl CompactionInjectorHook {
    pub fn new(config_path: PathBuf) -> Self {
        let config = Self::load_config(&config_path).unwrap_or_default();

        // Initialize provider manager and load providers
        let mut provider_manager = ContextProviderManager::new();
        match provider_manager.load_providers() {
            Ok(count) => {
                if count > 0 {
                    tracing::info!("Loaded {} context providers", count);
                }
            }
            Err(e) => {
                tracing::warn!("Failed to load context providers: {}", e);
            }
        }

        Self {
            config: RwLock::new(config),
            config_path,
            provider_manager: RwLock::new(provider_manager),
        }
    }

    /// Reload context providers from config directory
    pub async fn reload_providers(&self) -> Result<usize, String> {
        let mut manager = self.provider_manager.write().await;
        *manager = ContextProviderManager::new();
        manager.load_providers()
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
    fn inject_context(body: &str, static_context: Option<&str>, provider_context: Option<&str>) -> String {
        // Find the summary section and inject context
        let marker = "This session is being continued from a previous conversation that ran out of context.";

        if let Some(pos) = body.find(marker) {
            let after_marker = pos + marker.len();

            let mut injection = String::new();

            // Add provider context first (external services like ThreadCast)
            if let Some(ctx) = provider_context {
                if !ctx.is_empty() {
                    injection.push_str("\n\n");
                    injection.push_str(ctx);
                }
            }

            // Add static context
            if let Some(ctx) = static_context {
                if !ctx.is_empty() {
                    injection.push_str(&format!(
                        "\n\n## Persistent Context (Always Remember):\n{}\n",
                        ctx
                    ));
                }
            }

            if injection.is_empty() {
                return body.to_string();
            }

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

        // Case 2: Compacted conversation - inject persistent context + provider context
        if Self::is_compacted_conversation(body) {
            let static_context = config.context_injection.clone();
            let providers_enabled = config.context_providers_enabled;
            drop(config); // Release lock before async call

            // Fetch context from external providers if enabled
            let provider_context = if providers_enabled {
                let manager = self.provider_manager.read().await;
                if manager.provider_count() > 0 {
                    manager.fetch_combined_context().await
                } else {
                    None
                }
            } else {
                None
            };

            // Only inject if we have something to inject
            if static_context.is_some() || provider_context.is_some() {
                tracing::info!(
                    "[CompactionInjector] Detected compacted conversation (req_id: {}), injecting context (static: {}, providers: {})",
                    ctx.request_id,
                    static_context.is_some(),
                    provider_context.is_some()
                );
                return Some(Self::inject_context(
                    body,
                    static_context.as_deref(),
                    provider_context.as_deref(),
                ));
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
    fn test_inject_context_static_only() {
        let body = "This session is being continued from a previous conversation that ran out of context. Summary follows...";
        let context = "User prefers Korean responses.";

        let result = CompactionInjectorHook::inject_context(body, Some(context), None);

        assert!(result.contains("Persistent Context"));
        assert!(result.contains("User prefers Korean responses"));
    }

    #[test]
    fn test_inject_context_provider_only() {
        let body = "This session is being continued from a previous conversation that ran out of context. Summary follows...";
        let provider_ctx = "<project-knowledge>\n### deployment\nDeploy via SSH\n</project-knowledge>";

        let result = CompactionInjectorHook::inject_context(body, None, Some(provider_ctx));

        assert!(result.contains("project-knowledge"));
        assert!(result.contains("Deploy via SSH"));
        assert!(!result.contains("Persistent Context")); // No static context
    }

    #[test]
    fn test_inject_context_both() {
        let body = "This session is being continued from a previous conversation that ran out of context. Summary follows...";
        let static_ctx = "User prefers Korean.";
        let provider_ctx = "<knowledge>Test</knowledge>";

        let result = CompactionInjectorHook::inject_context(body, Some(static_ctx), Some(provider_ctx));

        assert!(result.contains("knowledge"));
        assert!(result.contains("Persistent Context"));
        assert!(result.contains("User prefers Korean"));
    }

    #[test]
    fn test_inject_context_empty() {
        let body = "This session is being continued from a previous conversation that ran out of context. Summary follows...";

        let result = CompactionInjectorHook::inject_context(body, None, None);

        assert_eq!(result, body); // No changes
    }
}
