use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::webhook::{StepUpdateData, WebhookClient};
use crate::storage::Database;

/// Maps Claude Code tool names to step types
#[derive(Debug)]
pub struct StepTracker {
    /// Current step for each session
    current_steps: Arc<RwLock<HashMap<String, String>>>,
}

impl StepTracker {
    pub fn new() -> Self {
        Self {
            current_steps: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Map tool name to step type
    fn tool_to_step_type(tool_name: &str) -> Option<&'static str> {
        match tool_name {
            // Analysis tools
            "Read" | "Glob" | "Grep" | "WebFetch" | "WebSearch" => Some("ANALYSIS"),

            // Design tools (planning, task management)
            "EnterPlanMode" | "ExitPlanMode" | "TaskCreate" | "TaskUpdate" | "TaskList" | "TaskGet" => Some("DESIGN"),

            // Implementation tools
            "Edit" | "Write" | "NotebookEdit" => Some("IMPLEMENTATION"),

            // Verification tools (testing, building)
            "Bash" => Some("VERIFICATION"), // Could be implementation or verification

            // User interaction
            "AskUserQuestion" | "Skill" => None, // Don't track these as steps

            // Task/Agent tools
            "Task" | "TaskOutput" | "TaskStop" => None, // Sub-agent operations

            _ => None,
        }
    }

    /// Check if this is a test-related bash command
    fn is_test_command(input: Option<&serde_json::Value>) -> bool {
        if let Some(input) = input {
            if let Some(command) = input.get("command").and_then(|v| v.as_str()) {
                let cmd_lower = command.to_lowercase();
                return cmd_lower.contains("test")
                    || cmd_lower.contains("jest")
                    || cmd_lower.contains("pytest")
                    || cmd_lower.contains("npm run test")
                    || cmd_lower.contains("./gradlew test")
                    || cmd_lower.contains("mvn test")
                    || cmd_lower.contains("cargo test");
            }
        }
        false
    }

    /// Process a tool_use event and return step update if needed
    pub async fn process_tool_use(
        &self,
        session_id: &str,
        tool_name: &str,
        tool_input: Option<&serde_json::Value>,
    ) -> Option<StepUpdateData> {
        // Special handling for Bash commands
        let step_type = if tool_name == "Bash" {
            if Self::is_test_command(tool_input) {
                Some("VERIFICATION")
            } else {
                // Bash without test keywords could be implementation
                Some("IMPLEMENTATION")
            }
        } else {
            Self::tool_to_step_type(tool_name)
        };

        let step_type = step_type?;

        // Check if we're already in this step
        let mut steps = self.current_steps.write().await;
        let current = steps.get(session_id);

        if current == Some(&step_type.to_string()) {
            // Already in this step, just update progress
            return Some(StepUpdateData {
                step_type: step_type.to_string(),
                status: "IN_PROGRESS".to_string(),
                progress: None,
                message: Some(format!("Using {}", tool_name)),
                tool_name: Some(tool_name.to_string()),
            });
        }

        // Step changed
        let previous = current.cloned();
        steps.insert(session_id.to_string(), step_type.to_string());

        tracing::info!(
            "STEP CHANGE: session={} {} -> {}",
            &session_id[..std::cmp::min(12, session_id.len())],
            previous.as_deref().unwrap_or("none"),
            step_type
        );

        Some(StepUpdateData {
            step_type: step_type.to_string(),
            status: "IN_PROGRESS".to_string(),
            progress: None,
            message: Some(format!("Started {} with {}", step_type.to_lowercase(), tool_name)),
            tool_name: Some(tool_name.to_string()),
        })
    }

    /// Send step update webhook if enabled
    pub async fn send_update(
        &self,
        db: &Database,
        webhook: &WebhookClient,
        session_id: &str,
        step_data: StepUpdateData,
    ) {
        // Get ThreadCast mapping for this session
        if let Ok(Some((todo_id, _))) = db.get_threadcast_mapping(session_id).await {
            if let Err(e) = webhook
                .send_step_update(Some(todo_id), session_id, step_data)
                .await
            {
                tracing::warn!("Failed to send step update webhook: {}", e);
            }
        }
    }

    /// Clear session state
    pub async fn clear_session(&self, session_id: &str) {
        let mut steps = self.current_steps.write().await;
        steps.remove(session_id);
    }
}

impl Default for StepTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for StepTracker {
    fn clone(&self) -> Self {
        Self {
            current_steps: self.current_steps.clone(),
        }
    }
}
