# Claude Code Terminal Display

## Overview

Claude Code가 API 응답을 터미널에 어떻게 렌더링하는지 설명합니다.

## Display Patterns

### 1. Text Response (text_delta)

**API 이벤트:**
```
event: content_block_delta
data: {"type": "content_block_delta", "delta": {"type": "text_delta", "text": "Hello"}}
```

**터미널 표시:**
```
Hello
```
→ 실시간으로 텍스트가 나타남 (타이핑 효과)

### 2. Thinking Animation (thinking_delta)

**API 이벤트:**
```
event: content_block_start
data: {"content_block": {"type": "thinking"}}

event: content_block_delta
data: {"delta": {"type": "thinking_delta", "thinking": "Let me analyze..."}}
```

**터미널 표시:**
```
⠋ Thinking...
⠙ Thinking...
⠹ Thinking...
```
→ 스피너 애니메이션 (thinking 블록이 완료될 때까지)

### 3. Tool Use Display

**API 이벤트:**
```json
{
  "type": "tool_use",
  "name": "Read",
  "input": {"file_path": "/path/to/file.rs"}
}
```

**터미널 표시:**
```
⏺ Read(/path/to/file.rs)
  ⎿ (파일 내용 또는 요약)
```

### 4. Different Tool Types

**Read:**
```
⏺ Read(src/main.rs)
  ⎿ 1: fn main() {
      2:     println!("Hello");
      3: }
```

**Edit:**
```
⏺ Edit(src/main.rs)
  ⎿ - old_line
    + new_line
```
→ Diff 형식 (빨간색 -, 초록색 +)

**Write:**
```
⏺ Write(new_file.rs)
  ⎿ Created new file (25 lines)
```

**Bash:**
```
⏺ Bash(ls -la)
  ⎿ total 64
    drwxr-xr-x  10 user  staff   320 Jan 27 16:00 .
    ...
```

**Glob:**
```
⏺ Glob(**/*.rs)
  ⎿ Found 15 files
    src/main.rs
    src/lib.rs
    ...
```

**Grep:**
```
⏺ Grep(pattern: "fn main")
  ⎿ src/main.rs:1: fn main() {
```

### 5. Task (Subagent)

**API 이벤트:**
```json
{
  "type": "tool_use",
  "name": "Task",
  "input": {
    "subagent_type": "Explore",
    "description": "Find hook files"
  }
}
```

**터미널 표시:**
```
⏺ Task(Find hook files)
  ├─ Explore agent started
  │  ⏺ Glob(**/*hook*.rs)
  │  ⏺ Read(src/hooks/mod.rs)
  │  ...
  ⎿ Found 5 hook-related files
```
→ 중첩된 도구 호출 표시

### 6. TodoWrite

**터미널 표시:**
```
┌─ Todo List ──────────────────────────────┐
│ ✓ Read existing files                     │
│ ● Creating new module                     │  ← 현재 진행 중
│ ○ Update tests                            │
│ ○ Build and verify                        │
└───────────────────────────────────────────┘
```

### 7. AskUserQuestion

**터미널 표시:**
```
┌─ Question ───────────────────────────────┐
│ Which approach do you prefer?            │
│                                          │
│ [1] Option A (Recommended)               │
│ [2] Option B                             │
│ [3] Other                                │
└───────────────────────────────────────────┘
> _
```

### 8. Error Display

**API 이벤트:**
```
event: error
data: {"error": {"type": "overloaded_error"}}
```

**터미널 표시:**
```
⚠ Error: API overloaded. Retrying...
```

## Response Flow Visualization

```
┌─────────────────────────────────────────────────────────────┐
│ User: "파일 읽고 수정해줘"                                      │
├─────────────────────────────────────────────────────────────┤
│ Claude:                                                      │
│   파일을 읽어보겠습니다.                          ← text_delta │
│                                                              │
│   ⏺ Read(src/main.rs)                           ← tool_use  │
│     ⎿ fn main() { ... }                                      │
│                                                              │
│   수정하겠습니다.                                  ← text_delta │
│                                                              │
│   ⏺ Edit(src/main.rs)                           ← tool_use  │
│     ⎿ - old_code                                             │
│       + new_code                                             │
│                                                              │
│   완료했습니다.                                    ← text_delta │
├─────────────────────────────────────────────────────────────┤
│ > _                                              ← 입력 대기  │
└─────────────────────────────────────────────────────────────┘
```

## Color Coding

| Element | Color |
|---------|-------|
| User input | White/Default |
| Claude text | White/Default |
| Tool name (⏺) | Cyan |
| Tool result (⎿) | Gray |
| Diff removed (-) | Red |
| Diff added (+) | Green |
| Error (⚠) | Yellow/Red |
| Success (✓) | Green |
| Progress (●) | Blue |
| Pending (○) | Gray |

## Status Line

화면 하단에 상태 표시:

```
───────────────────────────────────────────────────────────────
claude-opus-4-5 │ 1,234 in / 567 out │ $0.0234 │ session: abc123
```

## References

- https://github.com/Piebald-AI/claude-code-system-prompts
- https://code.claude.com/docs/en/cli-reference
