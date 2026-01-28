use async_trait::async_trait;
use super::context::{RequestContext, ResponseContext};

/// Hook trait for intercepting proxy request lifecycle events
#[async_trait]
pub trait Hook: Send + Sync {
    /// Called before the request is sent to the upstream server
    async fn on_request_before(&self, ctx: &RequestContext);

    /// Called after the request completes (success or failure)
    async fn on_request_after(&self, req_ctx: &RequestContext, res_ctx: &ResponseContext);

    /// Called when the request succeeds (2xx status)
    async fn on_request_success(&self, req_ctx: &RequestContext, res_ctx: &ResponseContext);

    /// Called when the request fails (non-2xx status or error)
    async fn on_request_failed(&self, req_ctx: &RequestContext, res_ctx: &ResponseContext);

    /// Called for each streaming response chunk
    async fn on_response_chunk(&self, req_ctx: &RequestContext, chunk: &[u8]);

    /// Called when the entire response is complete
    async fn on_response_complete(&self, req_ctx: &RequestContext, res_ctx: &ResponseContext);

    /// Return the hook name for logging purposes
    fn name(&self) -> &'static str;
}
