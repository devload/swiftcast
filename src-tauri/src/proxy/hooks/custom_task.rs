use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use tokio::sync::RwLock;

use super::context::RequestContext;

/// Task definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDefinition {
    /// Task name (used in >>swiftcast <name>)
    pub name: String,
    /// Description of the task
    pub description: String,
    /// Task type
    pub task_type: TaskType,
    /// Shell command to execute (for Shell type)
    pub command: Option<String>,
    /// Working directory for shell commands
    pub working_dir: Option<String>,
    /// HTTP URL to call (for Http type)
    pub url: Option<String>,
    /// HTTP method (GET, POST, etc.)
    pub http_method: Option<String>,
    /// File path to read (for ReadFile type)
    pub file_path: Option<String>,
    /// Environment variables to set
    pub env: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TaskType {
    Shell,
    Http,
    ReadFile,
    Composite,
}

/// Context passed to task execution
#[derive(Debug, Clone, Serialize)]
pub struct TaskContext {
    /// Claude session ID
    pub session_id: Option<String>,
    /// Request path
    pub path: String,
    /// Model being used
    pub model: String,
    /// Arguments passed to the task
    pub args: String,
}

/// Result of task interception
#[derive(Debug, Clone)]
pub struct InterceptResult {
    /// Whether to intercept (not forward to Claude)
    pub intercepted: bool,
    /// Response text to return
    pub response_text: String,
    /// Task name that was executed
    pub task_name: Option<String>,
}

/// Custom Task Hook for intercepting >>swiftcast commands
pub struct CustomTaskHook {
    tasks: RwLock<HashMap<String, TaskDefinition>>,
    config_path: PathBuf,
}

impl CustomTaskHook {
    pub fn new(config_path: PathBuf) -> Self {
        let tasks = Self::load_tasks(&config_path).unwrap_or_default();
        Self {
            tasks: RwLock::new(tasks),
            config_path,
        }
    }

    pub fn default_config_path() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join(".sessioncast").join("tasks.json")
    }

    fn load_tasks(path: &PathBuf) -> Option<HashMap<String, TaskDefinition>> {
        let content = std::fs::read_to_string(path).ok()?;
        let tasks: Vec<TaskDefinition> = serde_json::from_str(&content).ok()?;
        let map = tasks.into_iter().map(|t| (t.name.clone(), t)).collect();
        Some(map)
    }

    pub async fn reload_tasks(&self) -> Result<(), String> {
        let tasks = Self::load_tasks(&self.config_path)
            .ok_or_else(|| "Failed to load tasks".to_string())?;
        let mut t = self.tasks.write().await;
        *t = tasks;
        tracing::info!("Reloaded {} custom tasks", t.len());
        Ok(())
    }

    pub async fn save_tasks(&self) -> Result<(), String> {
        let tasks = self.tasks.read().await;
        let task_list: Vec<&TaskDefinition> = tasks.values().collect();
        let content = serde_json::to_string_pretty(&task_list)
            .map_err(|e| e.to_string())?;

        // Ensure directory exists
        if let Some(parent) = self.config_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }

        std::fs::write(&self.config_path, content).map_err(|e| e.to_string())?;
        tracing::info!("Saved tasks to {:?}", self.config_path);
        Ok(())
    }

    pub async fn add_task(&self, task: TaskDefinition) -> Result<(), String> {
        let mut tasks = self.tasks.write().await;
        tasks.insert(task.name.clone(), task);
        drop(tasks);
        self.save_tasks().await
    }

    pub async fn remove_task(&self, name: &str) -> Result<(), String> {
        let mut tasks = self.tasks.write().await;
        tasks.remove(name);
        drop(tasks);
        self.save_tasks().await
    }

    pub async fn list_tasks(&self) -> Vec<TaskDefinition> {
        let tasks = self.tasks.read().await;
        tasks.values().cloned().collect()
    }

    /// Check if message contains >>swiftcast command and extract task name
    fn parse_task_command(message: &str) -> Option<(String, String)> {
        // Look for >>swiftcast <name> pattern
        let patterns = [">>swiftcast "];
        for pattern in patterns {
            if let Some(pos) = message.find(pattern) {
                let after = &message[pos + pattern.len()..];
                // Extract task name (first word)
                let task_name = after.split_whitespace().next()?;
                // Extract remaining args
                let args = after[task_name.len()..].trim().to_string();
                return Some((task_name.to_string(), args));
            }
        }
        None
    }

    /// Execute a task and return the result
    async fn execute_task(&self, task: &TaskDefinition, task_ctx: &TaskContext) -> Result<String, String> {
        match task.task_type {
            TaskType::Shell => {
                self.execute_shell_task(task, task_ctx).await
            }
            TaskType::Http => {
                self.execute_http_task(task, task_ctx).await
            }
            TaskType::ReadFile => {
                self.execute_read_file_task(task, task_ctx).await
            }
            TaskType::Composite => {
                Ok("Composite tasks not yet implemented".to_string())
            }
        }
    }

    async fn execute_shell_task(&self, task: &TaskDefinition, task_ctx: &TaskContext) -> Result<String, String> {
        let command = task.command.as_ref()
            .ok_or_else(|| "No command specified".to_string())?;

        // Replace placeholders with context values
        let command = command
            .replace("{args}", &task_ctx.args)
            .replace("{session_id}", task_ctx.session_id.as_deref().unwrap_or(""))
            .replace("{path}", &task_ctx.path)
            .replace("{model}", &task_ctx.model);

        tracing::info!("Executing shell task '{}': {}", task.name, command);

        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(&command);

        // Set working directory if specified
        if let Some(ref wd) = task.working_dir {
            cmd.current_dir(wd);
        }

        // Set context as environment variables
        if let Some(ref sid) = task_ctx.session_id {
            cmd.env("SWIFTCAST_SESSION_ID", sid);
        }
        cmd.env("SWIFTCAST_PATH", &task_ctx.path);
        cmd.env("SWIFTCAST_MODEL", &task_ctx.model);
        cmd.env("SWIFTCAST_ARGS", &task_ctx.args);

        // Set user-defined environment variables
        if let Some(ref env) = task.env {
            for (key, value) in env {
                cmd.env(key, value);
            }
        }

        let output = cmd.output()
            .map_err(|e| format!("Failed to execute command: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if output.status.success() {
            Ok(format!("```\n{}\n```", stdout.trim()))
        } else {
            Ok(format!("Command failed (exit code: {:?}):\n```\n{}\n{}\n```",
                output.status.code(),
                stdout.trim(),
                stderr.trim()
            ))
        }
    }

    async fn execute_http_task(&self, task: &TaskDefinition, task_ctx: &TaskContext) -> Result<String, String> {
        let url = task.url.as_ref()
            .ok_or_else(|| "No URL specified".to_string())?;

        // Replace placeholders in URL
        let url = url
            .replace("{args}", &task_ctx.args)
            .replace("{session_id}", task_ctx.session_id.as_deref().unwrap_or(""))
            .replace("{path}", &task_ctx.path)
            .replace("{model}", &task_ctx.model);

        let method = task.http_method.as_deref().unwrap_or("GET");

        tracing::info!("Executing HTTP task '{}': {} {}", task.name, method, url);

        let client = reqwest::Client::new();
        let response = match method.to_uppercase().as_str() {
            "GET" => client.get(&url).send().await,
            "POST" => client.post(&url)
                .json(&task_ctx)
                .send().await,
            _ => return Err(format!("Unsupported HTTP method: {}", method)),
        }.map_err(|e| e.to_string())?;

        let status = response.status();
        let body = response.text().await.map_err(|e| e.to_string())?;

        Ok(format!("HTTP {} {}\nStatus: {}\n\n{}", method, url, status, body))
    }

    async fn execute_read_file_task(&self, task: &TaskDefinition, task_ctx: &TaskContext) -> Result<String, String> {
        let path = task.file_path.as_ref()
            .ok_or_else(|| "No file path specified".to_string())?;

        // Replace placeholders in path
        let path = path
            .replace("{args}", &task_ctx.args)
            .replace("{session_id}", task_ctx.session_id.as_deref().unwrap_or(""));

        tracing::info!("Executing read file task '{}': {}", task.name, path);

        let content = std::fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read file: {}", e))?;

        Ok(format!("```\n{}\n```", content))
    }

    /// Try to intercept a request containing >>swiftcast command
    pub async fn try_intercept(&self, ctx: &RequestContext) -> InterceptResult {
        // Extract user message from request body
        let user_message = ctx.body.get("messages")
            .and_then(|m| m.as_array())
            .and_then(|msgs| msgs.last())
            .and_then(|msg| {
                if msg.get("role").and_then(|r| r.as_str()) == Some("user") {
                    // Handle both string and array content
                    if let Some(content) = msg.get("content").and_then(|c| c.as_str()) {
                        return Some(content.to_string());
                    }
                    if let Some(content_arr) = msg.get("content").and_then(|c| c.as_array()) {
                        for item in content_arr {
                            if item.get("type").and_then(|t| t.as_str()) == Some("text") {
                                if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                                    return Some(text.to_string());
                                }
                            }
                        }
                    }
                }
                None
            });

        let user_message = match user_message {
            Some(m) => m,
            None => return InterceptResult {
                intercepted: false,
                response_text: String::new(),
                task_name: None,
            },
        };

        // Check for >>swiftcast command
        let (task_name, args) = match Self::parse_task_command(&user_message) {
            Some((name, args)) => (name, args),
            None => return InterceptResult {
                intercepted: false,
                response_text: String::new(),
                task_name: None,
            },
        };

        // Special command: >>swiftcast list
        if task_name == "list" {
            let tasks = self.list_tasks().await;
            let list = if tasks.is_empty() {
                "No custom tasks defined.\n\nAdd tasks to ~/.sessioncast/tasks.json".to_string()
            } else {
                tasks.iter()
                    .map(|t| format!("- **{}**: {} ({:?})", t.name, t.description, t.task_type))
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            return InterceptResult {
                intercepted: true,
                response_text: format!("## Available Custom Tasks\n\n{}", list),
                task_name: Some("list".to_string()),
            };
        }

        // Special command: >>swiftcast reload
        if task_name == "reload" {
            match self.reload_tasks().await {
                Ok(_) => {
                    let count = self.tasks.read().await.len();
                    return InterceptResult {
                        intercepted: true,
                        response_text: format!("Reloaded {} tasks from {:?}", count, self.config_path),
                        task_name: Some("reload".to_string()),
                    };
                }
                Err(e) => {
                    return InterceptResult {
                        intercepted: true,
                        response_text: format!("Failed to reload tasks: {}", e),
                        task_name: Some("reload".to_string()),
                    };
                }
            }
        }

        // Look up the task
        let tasks = self.tasks.read().await;
        let task = match tasks.get(&task_name) {
            Some(t) => t.clone(),
            None => {
                return InterceptResult {
                    intercepted: true,
                    response_text: format!("Unknown task: '{}'\n\nUse `>>swiftcast list` to see available tasks.", task_name),
                    task_name: Some(task_name),
                };
            }
        };
        drop(tasks);

        // Build task context
        let task_ctx = TaskContext {
            session_id: ctx.session_id.clone(),
            path: ctx.path.clone(),
            model: ctx.model.clone(),
            args,
        };

        // Execute the task
        tracing::info!(
            "[CustomTask] Executing task: {} (session: {:?}, path: {}, args: {})",
            task_name,
            task_ctx.session_id,
            task_ctx.path,
            task_ctx.args
        );

        match self.execute_task(&task, &task_ctx).await {
            Ok(result) => InterceptResult {
                intercepted: true,
                response_text: format!("## Task: {}\n\n{}\n\n---\n{}", task.name, task.description, result),
                task_name: Some(task_name),
            },
            Err(e) => InterceptResult {
                intercepted: true,
                response_text: format!("## Task Failed: {}\n\nError: {}", task.name, e),
                task_name: Some(task_name),
            },
        }
    }

    /// Generate a fake SSE response for intercepted requests
    pub fn generate_sse_response(text: &str) -> String {
        let message_id = format!("msg_{}", uuid::Uuid::new_v4().to_string().replace("-", "")[..24].to_string());

        // Build SSE events
        let mut events = Vec::new();

        // message_start
        events.push(format!(
            r#"event: message_start
data: {{"type":"message_start","message":{{"id":"{}","type":"message","role":"assistant","content":[],"model":"custom-task","stop_reason":null,"stop_sequence":null,"usage":{{"input_tokens":0,"output_tokens":0}}}}}}"#,
            message_id
        ));

        // content_block_start
        events.push(r#"event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#.to_string());

        // content_block_delta (split text into chunks for more natural streaming feel)
        let chunk_size = 50;
        for (i, chunk) in text.chars().collect::<Vec<_>>().chunks(chunk_size).enumerate() {
            let chunk_text: String = chunk.iter().collect();
            let escaped = serde_json::to_string(&chunk_text).unwrap_or_else(|_| format!("\"{}\"", chunk_text));
            // Remove surrounding quotes from escaped string
            let escaped = &escaped[1..escaped.len()-1];
            events.push(format!(
                r#"event: content_block_delta
data: {{"type":"content_block_delta","index":0,"delta":{{"type":"text_delta","text":"{}"}}}}"#,
                escaped
            ));
        }

        // content_block_stop
        events.push(r#"event: content_block_stop
data: {"type":"content_block_stop","index":0}"#.to_string());

        // message_delta
        // Use stop_reason: null to avoid triggering session_complete webhook
        // "end_turn" would cause ThreadCast to think the session is complete
        let output_tokens = text.len() / 4; // Rough estimate
        events.push(format!(
            r#"event: message_delta
data: {{"type":"message_delta","delta":{{"stop_reason":null,"stop_sequence":null}},"usage":{{"output_tokens":{}}}}}"#,
            output_tokens
        ));

        // message_stop
        events.push(r#"event: message_stop
data: {"type":"message_stop"}"#.to_string());

        events.join("\n\n") + "\n\n"
    }
}

impl Clone for CustomTaskHook {
    fn clone(&self) -> Self {
        Self {
            tasks: RwLock::new(HashMap::new()), // Clone with empty tasks, will reload
            config_path: self.config_path.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_task_command() {
        assert_eq!(
            CustomTaskHook::parse_task_command(">>swiftcast build"),
            Some(("build".to_string(), "".to_string()))
        );
        assert_eq!(
            CustomTaskHook::parse_task_command(">>swiftcast deploy prod"),
            Some(("deploy".to_string(), "prod".to_string()))
        );
        assert_eq!(
            CustomTaskHook::parse_task_command("hello >>swiftcast test arg1 arg2"),
            Some(("test".to_string(), "arg1 arg2".to_string()))
        );
        assert_eq!(
            CustomTaskHook::parse_task_command("no task here"),
            None
        );
    }
}
