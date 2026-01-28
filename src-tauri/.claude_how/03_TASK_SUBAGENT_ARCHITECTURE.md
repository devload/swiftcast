# Claude Code Task/Subagent Architecture

## Overview

Task는 **Claude Code 기능**입니다 (Anthropic API 기능 아님).
Claude가 Task 도구를 호출하면, Claude Code가 새로운 API 세션을 생성하여 Subagent를 실행합니다.

## 동작 흐름

```
Main Session (Opus)
    │
    ├─ Request 1: "코드베이스 분석해줘"
    │      ↓
    │  Response: tool_use(Task, subagent_type="Explore")
    │      ↓
    │  [Claude Code가 Task 실행]
    │      │
    │      └─── Subagent Session (Haiku) ← 별도 API 세션!
    │               ├─ Request A: system prompt + task prompt
    │               ├─ Response A: tool_use(Glob)
    │               ├─ Request B: tool_result + continue
    │               ├─ Response B: tool_use(Read)
    │               ├─ Request C: tool_result + continue
    │               └─ Response C: "분석 결과..." (end_turn)
    │      ↓
    │  tool_result: "Subagent 결과 요약"
    │      ↓
    ├─ Request 2: tool_result 포함해서 계속
    │      ↓
    └─ Response: "분석 결과를 보니..."
```

## Task Tool Definition

System Prompt에 포함되는 Task 도구 정의:

```json
{
  "name": "Task",
  "description": "Launch a new agent to handle complex, multi-step tasks autonomously...",
  "parameters": {
    "type": "object",
    "properties": {
      "description": {
        "type": "string",
        "description": "A short (3-5 word) description of the task"
      },
      "prompt": {
        "type": "string",
        "description": "The task for the agent to perform"
      },
      "subagent_type": {
        "type": "string",
        "enum": ["Explore", "Plan", "Bash", "general-purpose", "..."],
        "description": "The type of specialized agent to use"
      },
      "model": {
        "type": "string",
        "enum": ["sonnet", "opus", "haiku"],
        "description": "Optional model to use for this agent"
      },
      "run_in_background": {
        "type": "boolean",
        "description": "Set to true to run in background"
      },
      "resume": {
        "type": "string",
        "description": "Agent ID to resume from"
      }
    },
    "required": ["description", "prompt", "subagent_type"]
  }
}
```

## Built-in Subagent Types

| Type | Model | Tools | 용도 |
|------|-------|-------|------|
| **Explore** | Haiku | Read-only (Glob, Grep, Read) | 빠른 코드베이스 탐색 |
| **Plan** | Inherit | Read-only | 계획 모드 리서치 |
| **general-purpose** | Inherit | All (*) | 복잡한 멀티스텝 작업 |
| **Bash** | Inherit | Bash | 터미널 명령 전문 |
| **statusline-setup** | Sonnet | Read, Edit | 상태줄 설정 |
| **claude-code-guide** | Haiku | Read-only + Web | Claude Code 도움말 |

## Subagent 특성

### 독립 컨텍스트
- Main과 Subagent는 **컨텍스트가 격리**됨
- Subagent는 자신만의 system prompt를 받음
- 결과만 Main에 반환

### 중첩 불가
- **Subagent는 다른 Subagent를 생성할 수 없음**
- 무한 중첩 방지

### 병렬 실행
- 여러 Subagent 동시 실행 가능
- `run_in_background: true`로 백그라운드 실행

### Resume
- `resume` 파라미터로 이전 Subagent 계속 실행
- 전체 컨텍스트 유지

## Custom Subagent Definition

`.claude/agents/` 또는 `~/.claude/agents/`에 Markdown 파일로 정의:

```markdown
---
name: code-reviewer
description: Reviews code for quality and best practices
tools: Read, Glob, Grep
model: sonnet
permissionMode: default
---

You are a code reviewer. When invoked, analyze the code and provide
specific, actionable feedback on quality, security, and best practices.
```

### Frontmatter Fields

| Field | Required | Description |
|-------|----------|-------------|
| `name` | Yes | 고유 식별자 (소문자, 하이픈) |
| `description` | Yes | Claude가 위임 결정에 사용 |
| `tools` | No | 허용 도구 (기본: 전체 상속) |
| `disallowedTools` | No | 차단 도구 |
| `model` | No | sonnet/opus/haiku/inherit |
| `permissionMode` | No | default/acceptEdits/dontAsk/bypassPermissions/plan |
| `skills` | No | 주입할 스킬 |
| `hooks` | No | 라이프사이클 훅 |

## API 요청에서 Task 호출 예시

**Claude 응답:**
```json
{
  "content": [
    {"type": "text", "text": "코드베이스를 탐색해볼게요."},
    {
      "type": "tool_use",
      "id": "toolu_01xxx",
      "name": "Task",
      "input": {
        "subagent_type": "Explore",
        "description": "Find hook-related files",
        "prompt": "Search for all files related to hooks in the codebase. Look for hook definitions, implementations, and usages."
      }
    }
  ],
  "stop_reason": "tool_use"
}
```

**Claude Code 처리:**
1. Task tool_use 감지
2. 새 API 세션 생성 (Haiku 모델)
3. Explore 전용 system prompt 주입
4. Subagent가 여러 번 API 호출 (Glob, Read 등)
5. 완료 시 결과 요약을 tool_result로 반환

**tool_result:**
```json
{
  "role": "user",
  "content": [
    {
      "type": "tool_result",
      "tool_use_id": "toolu_01xxx",
      "content": "Found 5 hook-related files:\n- src/proxy/hooks/mod.rs\n- src/proxy/hooks/traits.rs\n..."
    }
  ]
}
```

## DB 로그로 확인

같은 session_id에서 다른 모델 요청 = Subagent 증거:

```
session_id                        | model                      | tokens
29dbd6d0ed4a418d982326d03cbcc29b | claude-opus-4-5-20251101   | 165    ← Main
29dbd6d0ed4a418d982326d03cbcc29b | claude-haiku-4-5-20251001  | 5      ← Subagent
29dbd6d0ed4a418d982326d03cbcc29b | claude-haiku-4-5-20251001  | 5      ← Subagent
29dbd6d0ed4a418d982326d03cbcc29b | claude-opus-4-5-20251101   | 157    ← Main
```

## References

- https://code.claude.com/docs/en/sub-agents
- https://github.com/Piebald-AI/claude-code-system-prompts
