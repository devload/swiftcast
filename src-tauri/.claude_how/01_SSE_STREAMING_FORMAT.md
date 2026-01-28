# Claude API SSE Streaming Format

## Overview

Claude API는 `stream: true` 설정 시 Server-Sent Events (SSE)로 응답을 스트리밍합니다.

## Event Flow

```
1. message_start      → 메시지 시작 (빈 content)
2. content_block_start → 콘텐츠 블록 시작
3. content_block_delta → 텍스트/도구 데이터 (반복)
4. content_block_stop  → 콘텐츠 블록 종료
5. message_delta      → 토큰 사용량 + stop_reason
6. message_stop       → 스트림 종료
```

## Event Types & JSON Examples

### 1. message_start

메시지 시작. 빈 content 배열과 초기 토큰 정보 포함.

```
event: message_start
data: {"type": "message_start", "message": {"id": "msg_xxx", "type": "message", "role": "assistant", "content": [], "model": "claude-sonnet-4-5-20250929", "stop_reason": null, "usage": {"input_tokens": 25, "output_tokens": 1}}}
```

### 2. content_block_start

새 콘텐츠 블록 시작. `index`는 최종 content 배열의 위치.

**텍스트 블록:**
```
event: content_block_start
data: {"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": ""}}
```

**Thinking 블록 (Extended Thinking):**
```
event: content_block_start
data: {"type": "content_block_start", "index": 0, "content_block": {"type": "thinking", "thinking": ""}}
```

**Tool Use 블록:**
```
event: content_block_start
data: {"type": "content_block_start", "index": 1, "content_block": {"type": "tool_use", "id": "toolu_01xxx", "name": "Read", "input": {}}}
```

### 3. content_block_delta

콘텐츠 증분 업데이트.

**text_delta (일반 텍스트):**
```
event: content_block_delta
data: {"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": "Hello"}}
```

**thinking_delta (Extended Thinking):**
```
event: content_block_delta
data: {"type": "content_block_delta", "index": 0, "delta": {"type": "thinking_delta", "thinking": "Let me analyze..."}}
```

**input_json_delta (Tool Use 파라미터):**
```
event: content_block_delta
data: {"type": "content_block_delta", "index": 1, "delta": {"type": "input_json_delta", "partial_json": "{\"path\": \"src/"}}
```

**signature_delta (Thinking 블록 서명):**
```
event: content_block_delta
data: {"type": "content_block_delta", "index": 0, "delta": {"type": "signature_delta", "signature": "EqQBCgIYAhIM..."}}
```

### 4. content_block_stop

콘텐츠 블록 종료.

```
event: content_block_stop
data: {"type": "content_block_stop", "index": 0}
```

### 5. message_delta

메시지 레벨 업데이트. stop_reason과 최종 토큰 수 포함.

```
event: message_delta
data: {"type": "message_delta", "delta": {"stop_reason": "end_turn", "stop_sequence": null}, "usage": {"output_tokens": 15}}
```

### 6. message_stop

스트림 종료.

```
event: message_stop
data: {"type": "message_stop"}
```

### 7. ping

Keep-alive 이벤트.

```
event: ping
data: {"type": "ping"}
```

### 8. error

에러 발생 시.

```
event: error
data: {"type": "error", "error": {"type": "overloaded_error", "message": "Overloaded"}}
```

## stop_reason Values

| Value | 의미 |
|-------|------|
| `end_turn` | 응답 완료, 사용자 입력 대기 |
| `tool_use` | 도구 실행 필요 |
| `max_tokens` | 토큰 한도 도달 |
| `stop_sequence` | Stop sequence 매칭 |

## Full Response Example

```
event: message_start
data: {"type": "message_start", "message": {"id": "msg_1nZdL29xx5MUA1yADyHTEsnR8uuvGzszyY", "type": "message", "role": "assistant", "content": [], "model": "claude-sonnet-4-5-20250929", "stop_reason": null, "stop_sequence": null, "usage": {"input_tokens": 25, "output_tokens": 1}}}

event: content_block_start
data: {"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": ""}}

event: ping
data: {"type": "ping"}

event: content_block_delta
data: {"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": "Hello"}}

event: content_block_delta
data: {"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": "!"}}

event: content_block_stop
data: {"type": "content_block_stop", "index": 0}

event: message_delta
data: {"type": "message_delta", "delta": {"stop_reason": "end_turn", "stop_sequence":null}, "usage": {"output_tokens": 15}}

event: message_stop
data: {"type": "message_stop"}
```

## References

- https://platform.claude.com/docs/en/api/messages-streaming
- https://platform.claude.com/docs/en/build-with-claude/streaming
