use std::sync::Arc;
use tokio::sync::RwLock;
use super::context::{RequestContext, ResponseContext};
use super::traits::{Hook, ModifyHook};

/// Registry for managing hooks
#[derive(Clone)]
pub struct HookRegistry {
    hooks: Arc<RwLock<Vec<Arc<dyn Hook>>>>,
    modify_hooks: Arc<RwLock<Vec<Arc<dyn ModifyHook>>>>,
    enabled: Arc<RwLock<bool>>,
}

impl HookRegistry {
    pub fn new() -> Self {
        Self {
            hooks: Arc::new(RwLock::new(Vec::new())),
            modify_hooks: Arc::new(RwLock::new(Vec::new())),
            enabled: Arc::new(RwLock::new(true)),
        }
    }

    /// Register a new read-only hook
    pub async fn register(&self, hook: Arc<dyn Hook>) {
        let mut hooks = self.hooks.write().await;
        tracing::info!("Registering hook: {}", hook.name());
        hooks.push(hook);
    }

    /// Register a new modify hook
    pub async fn register_modify_hook(&self, hook: Arc<dyn ModifyHook>) {
        let mut hooks = self.modify_hooks.write().await;
        tracing::info!("Registering modify hook: {}", hook.name());
        hooks.push(hook);
    }

    /// Enable or disable all hooks
    pub async fn set_enabled(&self, enabled: bool) {
        let mut e = self.enabled.write().await;
        *e = enabled;
        tracing::info!("Hooks enabled: {}", enabled);
    }

    /// Check if hooks are enabled
    pub async fn is_enabled(&self) -> bool {
        *self.enabled.read().await
    }

    /// Trigger on_request_before for all hooks
    pub async fn trigger_request_before(&self, ctx: &RequestContext) {
        if !self.is_enabled().await {
            return;
        }

        let hooks = self.hooks.read().await;
        for hook in hooks.iter() {
            hook.on_request_before(ctx).await;
        }
    }

    /// Trigger on_request_after for all hooks
    pub async fn trigger_request_after(&self, req_ctx: &RequestContext, res_ctx: &ResponseContext) {
        if !self.is_enabled().await {
            return;
        }

        let hooks = self.hooks.read().await;
        for hook in hooks.iter() {
            hook.on_request_after(req_ctx, res_ctx).await;
        }
    }

    /// Trigger on_request_success for all hooks
    pub async fn trigger_request_success(&self, req_ctx: &RequestContext, res_ctx: &ResponseContext) {
        if !self.is_enabled().await {
            return;
        }

        let hooks = self.hooks.read().await;
        for hook in hooks.iter() {
            hook.on_request_success(req_ctx, res_ctx).await;
        }
    }

    /// Trigger on_request_failed for all hooks
    pub async fn trigger_request_failed(&self, req_ctx: &RequestContext, res_ctx: &ResponseContext) {
        if !self.is_enabled().await {
            return;
        }

        let hooks = self.hooks.read().await;
        for hook in hooks.iter() {
            hook.on_request_failed(req_ctx, res_ctx).await;
        }
    }

    /// Trigger on_response_chunk for all hooks
    pub async fn trigger_response_chunk(&self, req_ctx: &RequestContext, chunk: &[u8]) {
        if !self.is_enabled().await {
            return;
        }

        let hooks = self.hooks.read().await;
        for hook in hooks.iter() {
            hook.on_response_chunk(req_ctx, chunk).await;
        }
    }

    /// Trigger on_response_complete for all hooks
    pub async fn trigger_response_complete(&self, req_ctx: &RequestContext, res_ctx: &ResponseContext) {
        if !self.is_enabled().await {
            return;
        }

        let hooks = self.hooks.read().await;
        for hook in hooks.iter() {
            hook.on_response_complete(req_ctx, res_ctx).await;
        }
    }

    /// Get the number of registered hooks
    pub async fn hook_count(&self) -> usize {
        self.hooks.read().await.len()
    }

    /// Get the number of registered modify hooks
    pub async fn modify_hook_count(&self) -> usize {
        self.modify_hooks.read().await.len()
    }

    /// Apply all modify hooks to request body
    /// Returns modified body if any hook modified it, otherwise None
    pub async fn apply_request_modifications(&self, body: &str, ctx: &RequestContext) -> Option<String> {
        if !self.is_enabled().await {
            return None;
        }

        let hooks = self.modify_hooks.read().await;
        let mut current_body = body.to_string();
        let mut modified = false;

        for hook in hooks.iter() {
            if let Some(new_body) = hook.modify_request_body(&current_body, ctx).await {
                tracing::debug!("Request body modified by hook: {}", hook.name());
                current_body = new_body;
                modified = true;
            }
        }

        if modified {
            Some(current_body)
        } else {
            None
        }
    }

    /// Apply all modify hooks to response text
    /// Returns modified text if any hook modified it, otherwise None
    pub async fn apply_response_modifications(&self, text: &str, ctx: &RequestContext) -> Option<String> {
        if !self.is_enabled().await {
            return None;
        }

        let hooks = self.modify_hooks.read().await;
        let mut current_text = text.to_string();
        let mut modified = false;

        for hook in hooks.iter() {
            if let Some(new_text) = hook.modify_response_text(&current_text, ctx).await {
                tracing::debug!("Response text modified by hook: {}", hook.name());
                current_text = new_text;
                modified = true;
            }
        }

        if modified {
            Some(current_text)
        } else {
            None
        }
    }
}

impl Default for HookRegistry {
    fn default() -> Self {
        Self::new()
    }
}
