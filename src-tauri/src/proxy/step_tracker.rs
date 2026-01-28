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
    /// Completed steps for each session (for session_complete webhook)
    completed_steps: Arc<RwLock<HashMap<String, Vec<String>>>>,
}

impl StepTracker {
    pub fn new() -> Self {
        Self {
            current_steps: Arc::new(RwLock::new(HashMap::new())),
            completed_steps: Arc::new(RwLock::new(HashMap::new())),
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

    /// Process a tool_use event and return step updates if needed
    /// Returns (previous_step_completed, new_step_in_progress)
    pub async fn process_tool_use(
        &self,
        session_id: &str,
        tool_name: &str,
        tool_input: Option<&serde_json::Value>,
    ) -> (Option<StepUpdateData>, Option<StepUpdateData>) {
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

        let step_type = match step_type {
            Some(s) => s,
            None => return (None, None),
        };

        // Check if we're already in this step
        let mut steps = self.current_steps.write().await;
        let current = steps.get(session_id);

        if current == Some(&step_type.to_string()) {
            // Already in this step, just update progress
            return (None, Some(StepUpdateData {
                step_type: step_type.to_string(),
                status: "IN_PROGRESS".to_string(),
                progress: None,
                message: Some(format!("Using {}", tool_name)),
                tool_name: Some(tool_name.to_string()),
            }));
        }

        // Step changed - complete previous step first
        let previous = current.cloned();
        steps.insert(session_id.to_string(), step_type.to_string());

        tracing::info!(
            "STEP CHANGE: session={} {} -> {}",
            &session_id[..std::cmp::min(12, session_id.len())],
            previous.as_deref().unwrap_or("none"),
            step_type
        );

        // Create completed update for previous step
        let completed_update = previous.map(|prev_step| StepUpdateData {
            step_type: prev_step,
            status: "COMPLETED".to_string(),
            progress: Some(100),
            message: Some("Step completed".to_string()),
            tool_name: None,
        });

        // Create in_progress update for new step
        let new_update = Some(StepUpdateData {
            step_type: step_type.to_string(),
            status: "IN_PROGRESS".to_string(),
            progress: None,
            message: Some(format!("Started {} with {}", step_type.to_lowercase(), tool_name)),
            tool_name: Some(tool_name.to_string()),
        });

        (completed_update, new_update)
    }

    /// Send step update webhooks if enabled
    /// Sends completed update first (if any), then new step update
    pub async fn send_updates(
        &self,
        db: &Database,
        webhook: &WebhookClient,
        session_id: &str,
        completed_step: Option<StepUpdateData>,
        new_step: Option<StepUpdateData>,
    ) {
        // Get ThreadCast mapping for this session
        if let Ok(Some((todo_id, _))) = db.get_threadcast_mapping(session_id).await {
            // Send completed step update first
            if let Some(completed) = completed_step {
                tracing::info!("Sending COMPLETED for step: {}", completed.step_type);
                if let Err(e) = webhook
                    .send_step_update(Some(todo_id.clone()), session_id, completed)
                    .await
                {
                    tracing::warn!("Failed to send step completed webhook: {}", e);
                }
            }

            // Then send new step update
            if let Some(new_data) = new_step {
                if let Err(e) = webhook
                    .send_step_update(Some(todo_id), session_id, new_data)
                    .await
                {
                    tracing::warn!("Failed to send step update webhook: {}", e);
                }
            }
        }
    }

    /// Complete the current step when session/response ends
    pub async fn complete_current_step(&self, session_id: &str) -> Option<StepUpdateData> {
        let mut steps = self.current_steps.write().await;
        if let Some(current_step) = steps.remove(session_id) {
            tracing::info!(
                "STEP COMPLETE (session end): session={} step={}",
                &session_id[..std::cmp::min(12, session_id.len())],
                current_step
            );

            // Track completed step
            let mut completed = self.completed_steps.write().await;
            completed
                .entry(session_id.to_string())
                .or_default()
                .push(current_step.clone());

            Some(StepUpdateData {
                step_type: current_step,
                status: "COMPLETED".to_string(),
                progress: Some(100),
                message: Some("Step completed".to_string()),
                tool_name: None,
            })
        } else {
            None
        }
    }

    /// Get list of completed steps for a session
    pub async fn get_completed_steps(&self, session_id: &str) -> Vec<String> {
        let completed = self.completed_steps.read().await;
        completed.get(session_id).cloned().unwrap_or_default()
    }

    /// Send a single step update
    pub async fn send_single_update(
        &self,
        db: &Database,
        webhook: &WebhookClient,
        session_id: &str,
        step_data: StepUpdateData,
    ) {
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
        let mut completed = self.completed_steps.write().await;
        completed.remove(session_id);
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
            completed_steps: self.completed_steps.clone(),
        }
    }
}
