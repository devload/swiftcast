use regex::Regex;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Patterns that indicate an AI is asking a question
const QUESTION_PATTERNS: &[&str] = &[
    r"(?i)should i (?:proceed|continue)\??",
    r"(?i)do you (?:want|approve|confirm)\??",
    r"(?i)is (?:this|that) (?:okay|correct|right)\??",
    r"(?i)shall i (?:proceed|continue|go ahead)\??",
    r"(?i)would you like (?:me to|to)\??",
    r"(?i)can i (?:proceed|continue|go ahead)\??",
    r"(?i)are you (?:sure|okay with)\??",
    r"(?i)please (?:confirm|approve|verify)",
    r"(?i)\[y(?:es)?/n(?:o)?\]",
    r"(?i)press.*(?:enter|y|n).*to.*(?:continue|proceed|confirm)",
];

/// Extracted question information
#[derive(Debug, Clone)]
pub struct DetectedQuestion {
    pub question: String,
    pub context: String,
    pub options: Vec<String>,
}

/// Detects AI questions from streaming text
pub struct QuestionDetector {
    patterns: Vec<Regex>,
    text_buffer: Arc<Mutex<String>>,
    max_buffer_size: usize,
}

impl QuestionDetector {
    pub fn new() -> Self {
        let patterns = QUESTION_PATTERNS
            .iter()
            .filter_map(|p| Regex::new(p).ok())
            .collect();

        Self {
            patterns,
            text_buffer: Arc::new(Mutex::new(String::new())),
            max_buffer_size: 4096, // Keep last 4KB for context
        }
    }

    /// Append text to buffer and check for questions
    pub async fn process_text(&self, text: &str) -> Option<DetectedQuestion> {
        let mut buffer = self.text_buffer.lock().await;

        // Append new text
        buffer.push_str(text);

        // Trim buffer if too large (keep end)
        if buffer.len() > self.max_buffer_size {
            let start = buffer.len() - self.max_buffer_size;
            *buffer = buffer[start..].to_string();
        }

        // Check for question patterns
        for pattern in &self.patterns {
            if let Some(m) = pattern.find(&buffer) {
                let start = m.start();
                let end = m.end();

                // Extract the question text (sentence containing the match)
                let question = extract_sentence(&buffer, start, end);

                // Extract context (text before the question)
                let context_start = start.saturating_sub(200);
                let context = buffer[context_start..start].trim().to_string();

                // Extract options if present
                let options = extract_options(&buffer[start..]);

                // Clear buffer after detection to avoid duplicate detections
                buffer.clear();

                return Some(DetectedQuestion {
                    question,
                    context,
                    options,
                });
            }
        }

        None
    }

    /// Reset the text buffer
    pub async fn reset(&self) {
        let mut buffer = self.text_buffer.lock().await;
        buffer.clear();
    }
}

impl Default for QuestionDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for QuestionDetector {
    fn clone(&self) -> Self {
        Self {
            patterns: self.patterns.clone(),
            text_buffer: Arc::new(Mutex::new(String::new())),
            max_buffer_size: self.max_buffer_size,
        }
    }
}

/// Extract the sentence containing the match
fn extract_sentence(text: &str, start: usize, end: usize) -> String {
    // Find sentence boundaries
    let sentence_start = text[..start]
        .rfind(|c| c == '.' || c == '!' || c == '?' || c == '\n')
        .map(|i| i + 1)
        .unwrap_or(0);

    let sentence_end = text[end..]
        .find(|c| c == '.' || c == '!' || c == '?' || c == '\n')
        .map(|i| end + i + 1)
        .unwrap_or(text.len());

    text[sentence_start..sentence_end].trim().to_string()
}

/// Extract options from text (e.g., [Y/N], yes/no)
fn extract_options(text: &str) -> Vec<String> {
    let mut options = Vec::new();

    // Check for [Y/N] pattern
    if let Ok(re) = Regex::new(r"\[([^\]]+)\]") {
        if let Some(caps) = re.captures(text) {
            if let Some(m) = caps.get(1) {
                for opt in m.as_str().split('/') {
                    let opt = opt.trim().to_string();
                    if !opt.is_empty() {
                        options.push(opt);
                    }
                }
            }
        }
    }

    // Default options if none found
    if options.is_empty() {
        options.push("Yes".to_string());
        options.push("No".to_string());
    }

    options
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_detect_should_proceed() {
        let detector = QuestionDetector::new();
        let result = detector.process_text("I found the bug. Should I proceed with the fix?").await;
        assert!(result.is_some());
    }

    #[tokio::test]
    async fn test_detect_yn_pattern() {
        let detector = QuestionDetector::new();
        let result = detector.process_text("Press [Y/N] to continue").await;
        assert!(result.is_some());
        let q = result.unwrap();
        assert!(q.options.contains(&"Y".to_string()));
    }

    #[tokio::test]
    async fn test_no_question() {
        let detector = QuestionDetector::new();
        let result = detector.process_text("I completed the task successfully.").await;
        assert!(result.is_none());
    }
}
