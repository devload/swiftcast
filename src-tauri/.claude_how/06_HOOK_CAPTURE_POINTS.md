# SwiftCast Hook Capture Points

## Overview

SwiftCast Hook 시스템이 캡처해야 할 데이터 포인트입니다.

## Current Implementation

현재 구현된 캡처:

| Data | Captured | Location |
|------|----------|----------|
| Request body (full JSON) | ✅ | `request.body` |
| Response text (text_delta only) | ✅ | `response.response_text` |
| Token usage | ✅ | `response.input_tokens`, `output_tokens` |
| Status code | ✅ | `response.status_code` |
| Duration | ✅ | `response.duration_ms` |
| Session ID | ✅ | `request.session_id` |
| Model | ✅ | `request.model` |

## Missing Data Points

추가로 캡처하면 유용한 것들:

### 1. Extended Thinking Content

```rust
// thinking_delta 캡처
if json.get("type") == "content_block_start" {
    if let Some(block) = json.get("content_block") {
        if block.get("type") == "thinking" {
            // thinking 블록 시작 기록
        }
    }
}

if delta.get("type") == "thinking_delta" {
    if let Some(thinking) = delta.get("thinking") {
        // thinking 내용 누적
    }
}
```

### 2. Tool Use Information

```rust
// tool_use 블록 캡처
if content.get("type") == "tool_use" {
    let tool_info = ToolUseInfo {
        id: content.get("id"),
        name: content.get("name"),
        input: content.get("input"),
    };
    // 도구 호출 정보 저장
}
```

### 3. Stop Reason

```rust
// message_delta에서 stop_reason 추출
if json.get("type") == "message_delta" {
    if let Some(delta) = json.get("delta") {
        let stop_reason = delta.get("stop_reason");
        // stop_reason 저장
    }
}
```

## Enhanced Log Format

확장된 JSON 로그 형식:

```json
{
  "request_id": "uuid",
  "session_id": "abc123...",
  "request": {
    "timestamp": 1706369452,
    "timestamp_iso": "2026-01-27T14:30:52Z",
    "model": "claude-opus-4-5-20251101",
    "method": "POST",
    "path": "/v1/messages",
    "body": {
      "model": "claude-opus-4-5-20251101",
      "system": "You are Claude Code...",
      "tools": [...],
      "messages": [...]
    }
  },
  "response": {
    "timestamp": 1706369455,
    "status_code": 200,
    "duration_ms": 3200,
    "stop_reason": "tool_use",
    "input_tokens": 1500,
    "output_tokens": 800,
    "is_success": true,
    "content_blocks": [
      {
        "type": "text",
        "text": "파일을 읽어보겠습니다."
      },
      {
        "type": "thinking",
        "thinking": "Let me analyze the file structure..."
      },
      {
        "type": "tool_use",
        "id": "toolu_01xxx",
        "name": "Read",
        "input": {"file_path": "/path/to/file.rs"}
      }
    ],
    "response_text": "파일을 읽어보겠습니다."
  }
}
```

## SSE Parsing Enhancement

### Current Parser

```rust
fn parse_text_from_sse(data: &str) -> Option<String> {
    // content_block_delta에서 text_delta만 추출
    if json.get("type") == "content_block_delta" {
        if let Some(delta) = json.get("delta") {
            if let Some(t) = delta.get("text") {
                text.push_str(t);
            }
        }
    }
}
```

### Enhanced Parser

```rust
#[derive(Debug, Clone)]
struct ParsedSSE {
    text_content: String,
    thinking_content: String,
    tool_uses: Vec<ToolUseInfo>,
    stop_reason: Option<String>,
    usage: Option<UsageInfo>,
}

fn parse_sse_complete(data: &str) -> ParsedSSE {
    let mut result = ParsedSSE::default();

    for line in data.lines() {
        if !line.starts_with("data: ") { continue; }
        let json_str = &line[6..];
        let json: Value = serde_json::from_str(json_str)?;

        match json.get("type").and_then(|v| v.as_str()) {
            Some("content_block_start") => {
                // 블록 타입 기록
            }
            Some("content_block_delta") => {
                if let Some(delta) = json.get("delta") {
                    match delta.get("type").and_then(|v| v.as_str()) {
                        Some("text_delta") => {
                            result.text_content.push_str(
                                delta.get("text").and_then(|v| v.as_str()).unwrap_or("")
                            );
                        }
                        Some("thinking_delta") => {
                            result.thinking_content.push_str(
                                delta.get("thinking").and_then(|v| v.as_str()).unwrap_or("")
                            );
                        }
                        Some("input_json_delta") => {
                            // tool input 누적
                        }
                        _ => {}
                    }
                }
            }
            Some("message_delta") => {
                if let Some(delta) = json.get("delta") {
                    result.stop_reason = delta.get("stop_reason")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                }
                if let Some(usage) = json.get("usage") {
                    result.usage = Some(UsageInfo {
                        input_tokens: usage.get("input_tokens").and_then(|v| v.as_i64()).unwrap_or(0),
                        output_tokens: usage.get("output_tokens").and_then(|v| v.as_i64()).unwrap_or(0),
                    });
                }
            }
            _ => {}
        }
    }

    result
}
```

## Hook Trigger Points

```
Request Flow:
    │
    ├─ on_request_before()     ← 요청 시작 전
    │       └─ RequestContext 생성
    │
    ├─ [Upstream Request]
    │
    ├─ on_response_chunk()     ← 각 SSE 청크마다 (선택적)
    │       └─ 실시간 데이터 수집
    │
    ├─ [Usage detected in SSE]
    │
    ├─ on_response_complete()  ← 스트림 완료
    │       └─ ResponseContext 최종화
    │
    ├─ on_request_success() or on_request_failed()
    │
    └─ on_request_after()      ← 요청 완전 종료
            └─ 로그 파일 작성
```

## Use Cases for Enhanced Logging

### 1. Subagent Analysis
- Main vs Subagent 구분 (같은 session_id, 다른 model)
- Subagent 호출 빈도 분석
- Task 유형별 통계

### 2. Tool Usage Patterns
- 가장 많이 사용되는 도구
- 도구별 실패율
- 도구 체인 분석

### 3. Performance Monitoring
- Thinking 시간 vs 응답 시간
- 토큰 효율성
- 세션별 비용 추적

### 4. Debugging
- 실패한 요청의 전체 컨텍스트
- tool_result 분석
- 에러 패턴 파악

## References

- 현재 구현: `src/proxy/hooks/file_logger.rs`
- SSE 파싱: `src/proxy/server.rs` (parse_text_from_sse, parse_usage_from_sse)
