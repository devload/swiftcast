# Claude Code Internals Documentation

Claude Code가 어떻게 동작하는지에 대한 내부 문서입니다.

## Contents

| File | Description |
|------|-------------|
| [01_SSE_STREAMING_FORMAT.md](./01_SSE_STREAMING_FORMAT.md) | SSE 스트리밍 이벤트 형식 |
| [02_TOOL_USE_FORMAT.md](./02_TOOL_USE_FORMAT.md) | Tool Use 요청/응답 형식 |
| [03_TASK_SUBAGENT_ARCHITECTURE.md](./03_TASK_SUBAGENT_ARCHITECTURE.md) | Task/Subagent 아키텍처 |
| [04_SYSTEM_PROMPT_STRUCTURE.md](./04_SYSTEM_PROMPT_STRUCTURE.md) | System Prompt 구조 |
| [05_TERMINAL_DISPLAY.md](./05_TERMINAL_DISPLAY.md) | 터미널 표시 방식 |
| [06_HOOK_CAPTURE_POINTS.md](./06_HOOK_CAPTURE_POINTS.md) | Hook 캡처 포인트 |

## Quick Summary

### API Response Flow

```
User Request
    ↓
Claude API (stream: true)
    ↓
SSE Events (message_start → content_block_delta → message_stop)
    ↓
Claude Code Terminal Rendering
```

### Key Concepts

1. **SSE Streaming**: 실시간 토큰 스트리밍 (text_delta, thinking_delta, input_json_delta)
2. **Tool Use**: stop_reason: "tool_use" → 도구 실행 → tool_result 전송 → 계속
3. **Task/Subagent**: 별도 API 세션 생성, 독립 컨텍스트, 결과만 반환
4. **System Prompt**: 110+ 모듈형 컴포넌트, 동적 주입

### stop_reason Values

| Value | Action |
|-------|--------|
| `end_turn` | 응답 완료, 사용자 입력 대기 |
| `tool_use` | 도구 실행 필요 |
| `max_tokens` | 토큰 한도 도달, 계속 요청 필요 |

### Built-in Subagents

| Type | Model | Purpose |
|------|-------|---------|
| Explore | Haiku | 빠른 코드베이스 탐색 |
| Plan | Inherit | 계획 모드 리서치 |
| general-purpose | Inherit | 복잡한 멀티스텝 |

## References

### Official Documentation
- https://platform.claude.com/docs/en/api/messages-streaming
- https://platform.claude.com/docs/en/agents-and-tools/tool-use/overview
- https://code.claude.com/docs/en/sub-agents

### Community Resources
- https://github.com/Piebald-AI/claude-code-system-prompts
- https://weaxsey.org/en/articles/2025-10-12/

## Related Files

- Hook 구현: `src/proxy/hooks/`
- SSE 파싱: `src/proxy/server.rs`

---

Last Updated: 2026-01-27
