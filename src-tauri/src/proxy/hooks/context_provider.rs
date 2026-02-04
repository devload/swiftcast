use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

/// Configuration for a context provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub provider: ProviderMeta,
    #[serde(default)]
    pub http: Option<HttpConfig>,
    #[serde(default)]
    pub response: Option<ResponseConfig>,
    #[serde(default)]
    pub output: Option<OutputConfig>,
    #[serde(default)]
    pub variables: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderMeta {
    pub name: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(rename = "type")]
    pub provider_type: String,
}

fn default_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpConfig {
    #[serde(default = "default_method")]
    pub method: String,
    pub url: String,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    #[serde(default)]
    pub headers: HashMap<String, String>,
}

fn default_method() -> String {
    "GET".to_string()
}

fn default_timeout() -> u64 {
    5
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseConfig {
    /// JSON path to extract (e.g., "data.knowledge" or "result.items")
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputConfig {
    /// Template for formatting output (supports basic {{variable}} substitution)
    pub template: String,
}

/// Trait for context providers
#[async_trait]
pub trait ContextProvider: Send + Sync {
    /// Fetch context from the provider
    async fn fetch_context(&self) -> Result<String, String>;

    /// Get provider name
    fn name(&self) -> &str;

    /// Check if provider is enabled
    fn is_enabled(&self) -> bool;
}

/// HTTP-based context provider
pub struct HttpContextProvider {
    config: ProviderConfig,
    client: Client,
}

impl HttpContextProvider {
    pub fn new(config: ProviderConfig) -> Result<Self, String> {
        let http = config.http.as_ref()
            .ok_or("HTTP config is required for http provider")?;

        let client = Client::builder()
            .timeout(Duration::from_secs(http.timeout_secs))
            .connect_timeout(Duration::from_secs(3))
            .build()
            .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

        Ok(Self { config, client })
    }

    /// Substitute variables in a string (e.g., ${workspace_id} -> actual value)
    fn substitute_variables(&self, input: &str) -> String {
        let mut result = input.to_string();
        for (key, value) in &self.config.variables {
            let pattern = format!("${{{}}}", key);
            result = result.replace(&pattern, value);
        }
        // Also check environment variables
        for (key, value) in std::env::vars() {
            let pattern = format!("${{{}}}", key);
            result = result.replace(&pattern, &value);
        }
        result
    }

    /// Extract value from JSON using a path like "data.knowledge"
    fn extract_json_path(json: &serde_json::Value, path: &str) -> Option<serde_json::Value> {
        let parts: Vec<&str> = path.split('.').collect();
        let mut current = json.clone();

        for part in parts {
            current = current.get(part)?.clone();
        }

        Some(current)
    }

    /// Format the extracted data using the template
    fn format_output(&self, data: &serde_json::Value) -> String {
        let template = self.config.output.as_ref()
            .map(|o| o.template.as_str())
            .unwrap_or("{{data}}");

        // Simple template substitution
        // For objects/arrays, iterate and format
        if let Some(obj) = data.as_object() {
            let mut items = String::new();
            for (key, value) in obj {
                let summary = value.get("summary")
                    .and_then(|s| s.as_str())
                    .unwrap_or("");
                items.push_str(&format!("### {}\n{}\n\n", key, summary));
            }
            template.replace("{{#each this}}", "")
                .replace("{{/each}}", "")
                .replace("{{@key}}", "")
                .replace("{{this.summary}}", "")
                .trim()
                .to_string()
                + "\n" + &items
        } else if let Some(arr) = data.as_array() {
            let items: Vec<String> = arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
            items.join("\n")
        } else if let Some(s) = data.as_str() {
            s.to_string()
        } else {
            serde_json::to_string_pretty(data).unwrap_or_default()
        }
    }
}

#[async_trait]
impl ContextProvider for HttpContextProvider {
    async fn fetch_context(&self) -> Result<String, String> {
        let http = self.config.http.as_ref()
            .ok_or("HTTP config is required")?;

        let url = self.substitute_variables(&http.url);

        // Build request
        let mut request = match http.method.to_uppercase().as_str() {
            "GET" => self.client.get(&url),
            "POST" => self.client.post(&url),
            _ => return Err(format!("Unsupported HTTP method: {}", http.method)),
        };

        // Add headers
        for (key, value) in &http.headers {
            let value = self.substitute_variables(value);
            request = request.header(key, value);
        }

        // Send request
        let response = request.send().await
            .map_err(|e| format!("HTTP request failed: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("HTTP error: {}", response.status()));
        }

        let json: serde_json::Value = response.json().await
            .map_err(|e| format!("Failed to parse JSON response: {}", e))?;

        // Extract data using path
        let data = if let Some(ref resp_config) = self.config.response {
            if let Some(ref path) = resp_config.path {
                Self::extract_json_path(&json, path)
                    .unwrap_or(json)
            } else {
                json
            }
        } else {
            json
        };

        // Format output
        Ok(self.format_output(&data))
    }

    fn name(&self) -> &str {
        &self.config.provider.name
    }

    fn is_enabled(&self) -> bool {
        self.config.provider.enabled
    }
}

/// Manager for loading and running context providers
pub struct ContextProviderManager {
    providers: Vec<Box<dyn ContextProvider>>,
    config_dir: PathBuf,
}

impl ContextProviderManager {
    pub fn new() -> Self {
        // Try ~/.config/swiftcast first (Linux/macOS common), then fall back to system config dir
        let config_dir = dirs::home_dir()
            .map(|h| h.join(".config").join("swiftcast").join("context_providers"))
            .filter(|p| p.exists())
            .or_else(|| {
                dirs::config_dir().map(|c| c.join("swiftcast").join("context_providers"))
            })
            .unwrap_or_else(|| PathBuf::from(".").join("context_providers"));

        Self {
            providers: Vec::new(),
            config_dir,
        }
    }

    pub fn with_config_dir(config_dir: PathBuf) -> Self {
        Self {
            providers: Vec::new(),
            config_dir,
        }
    }

    /// Load all provider configs from the config directory
    pub fn load_providers(&mut self) -> Result<usize, String> {
        if !self.config_dir.exists() {
            tracing::debug!("Context providers config dir does not exist: {:?}", self.config_dir);
            return Ok(0);
        }

        let entries = std::fs::read_dir(&self.config_dir)
            .map_err(|e| format!("Failed to read config dir: {}", e))?;

        let mut count = 0;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "toml").unwrap_or(false) {
                match self.load_provider_from_file(&path) {
                    Ok(_) => {
                        count += 1;
                        tracing::info!("Loaded context provider from {:?}", path);
                    }
                    Err(e) => {
                        tracing::warn!("Failed to load provider from {:?}: {}", path, e);
                    }
                }
            }
        }

        Ok(count)
    }

    fn load_provider_from_file(&mut self, path: &PathBuf) -> Result<(), String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read file: {}", e))?;

        let config: ProviderConfig = toml::from_str(&content)
            .map_err(|e| format!("Failed to parse TOML: {}", e))?;

        if !config.provider.enabled {
            tracing::debug!("Provider {} is disabled, skipping", config.provider.name);
            return Ok(());
        }

        match config.provider.provider_type.as_str() {
            "http" => {
                let provider = HttpContextProvider::new(config)?;
                self.providers.push(Box::new(provider));
            }
            other => {
                return Err(format!("Unknown provider type: {}", other));
            }
        }

        Ok(())
    }

    /// Fetch context from all enabled providers
    pub async fn fetch_all_contexts(&self) -> Vec<(String, Result<String, String>)> {
        let mut results = Vec::new();

        for provider in &self.providers {
            if provider.is_enabled() {
                let name = provider.name().to_string();
                let result = provider.fetch_context().await;
                results.push((name, result));
            }
        }

        results
    }

    /// Fetch and combine all contexts into a single string
    pub async fn fetch_combined_context(&self) -> Option<String> {
        let results = self.fetch_all_contexts().await;

        let mut combined = String::new();
        for (name, result) in results {
            match result {
                Ok(context) if !context.is_empty() => {
                    if !combined.is_empty() {
                        combined.push_str("\n\n");
                    }
                    combined.push_str(&context);
                    tracing::debug!("Added context from provider: {}", name);
                }
                Ok(_) => {
                    tracing::debug!("Provider {} returned empty context", name);
                }
                Err(e) => {
                    tracing::warn!("Provider {} failed: {}", name, e);
                }
            }
        }

        if combined.is_empty() {
            None
        } else {
            Some(combined)
        }
    }

    pub fn provider_count(&self) -> usize {
        self.providers.len()
    }
}

impl Default for ContextProviderManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_substitute_variables() {
        let config = ProviderConfig {
            provider: ProviderMeta {
                name: "test".to_string(),
                enabled: true,
                provider_type: "http".to_string(),
            },
            http: Some(HttpConfig {
                method: "GET".to_string(),
                url: "http://example.com/${workspace_id}/data".to_string(),
                timeout_secs: 5,
                headers: HashMap::new(),
            }),
            response: None,
            output: None,
            variables: vec![
                ("workspace_id".to_string(), "abc123".to_string()),
            ].into_iter().collect(),
        };

        let provider = HttpContextProvider::new(config).unwrap();
        let result = provider.substitute_variables("http://example.com/${workspace_id}/data");
        assert_eq!(result, "http://example.com/abc123/data");
    }

    #[test]
    fn test_extract_json_path() {
        let json = serde_json::json!({
            "data": {
                "knowledge": {
                    "topic1": { "summary": "Summary 1" },
                    "topic2": { "summary": "Summary 2" }
                }
            }
        });

        let result = HttpContextProvider::extract_json_path(&json, "data.knowledge");
        assert!(result.is_some());

        let knowledge = result.unwrap();
        assert!(knowledge.get("topic1").is_some());
    }

    #[test]
    fn test_provider_config_parsing() {
        let toml_content = r#"
[provider]
name = "Test Provider"
enabled = true
type = "http"

[http]
method = "GET"
url = "http://localhost:8080/api/data"
timeout_secs = 10

[http.headers]
Authorization = "Bearer ${TOKEN}"

[response]
path = "data.items"

[output]
template = "<context>{{data}}</context>"

[variables]
workspace_id = "test-123"
"#;

        let config: ProviderConfig = toml::from_str(toml_content).unwrap();
        assert_eq!(config.provider.name, "Test Provider");
        assert!(config.provider.enabled);
        assert_eq!(config.http.as_ref().unwrap().url, "http://localhost:8080/api/data");
        assert_eq!(config.variables.get("workspace_id"), Some(&"test-123".to_string()));
    }

    /// Integration test - requires ThreadCast server running on localhost:21000
    /// Run with: THREADCAST_TOKEN=xxx cargo test test_threadcast_integration -- --nocapture --ignored
    #[tokio::test]
    #[ignore] // Run manually with --ignored flag
    async fn test_threadcast_integration() {
        let token = std::env::var("THREADCAST_TOKEN").expect("THREADCAST_TOKEN not set");
        let workspace_id = "b7f3362b-658f-4f72-98f1-95b218b31fa9";

        let config = ProviderConfig {
            provider: ProviderMeta {
                name: "ThreadCast Knowledge".to_string(),
                enabled: true,
                provider_type: "http".to_string(),
            },
            http: Some(HttpConfig {
                method: "GET".to_string(),
                url: format!("http://localhost:21000/api/workspaces/{}/meta", workspace_id),
                timeout_secs: 5,
                headers: vec![
                    ("Authorization".to_string(), format!("Bearer {}", token)),
                ].into_iter().collect(),
            }),
            response: Some(ResponseConfig {
                path: Some("data.knowledge".to_string()),
            }),
            output: Some(OutputConfig {
                template: "<project-knowledge>\n{{data}}\n</project-knowledge>".to_string(),
            }),
            variables: HashMap::new(),
        };

        let provider = HttpContextProvider::new(config).unwrap();
        let result = provider.fetch_context().await;

        println!("Result: {:?}", result);
        assert!(result.is_ok());

        let context = result.unwrap();
        println!("Context:\n{}", context);
        assert!(!context.is_empty());
        assert!(context.contains("aws-deployment") || context.contains("coding-conventions"));
    }

    /// Test loading providers from config directory
    #[tokio::test]
    #[ignore]
    async fn test_load_providers_from_config() {
        let mut manager = ContextProviderManager::new();
        println!("Config dir: {:?}", manager.config_dir);

        let count = manager.load_providers().unwrap_or(0);
        println!("Loaded {} providers", count);

        if count > 0 {
            let results = manager.fetch_all_contexts().await;
            for (name, result) in results {
                println!("Provider '{}': {:?}", name, result);
            }
        }
    }
}
