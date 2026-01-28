# Claude API Tool Use Format

## Overview

Claude가 도구를 호출할 때의 요청/응답 형식입니다.

## Tool Definition (Request)

API 요청 시 `tools` 배열에 도구 정의를 포함합니다.

```json
{
  "model": "claude-sonnet-4-5-20250929",
  "max_tokens": 1024,
  "tools": [
    {
      "name": "Read",
      "description": "Reads a file from the local filesystem",
      "input_schema": {
        "type": "object",
        "properties": {
          "file_path": {
            "type": "string",
            "description": "The absolute path to the file to read"
          }
        },
        "required": ["file_path"]
      }
    },
    {
      "name": "Edit",
      "description": "Performs exact string replacements in files",
      "input_schema": {
        "type": "object",
        "properties": {
          "file_path": {"type": "string"},
          "old_string": {"type": "string"},
          "new_string": {"type": "string"}
        },
        "required": ["file_path", "old_string", "new_string"]
      }
    },
    {
      "name": "Bash",
      "description": "Executes a bash command",
      "input_schema": {
        "type": "object",
        "properties": {
          "command": {"type": "string"}
        },
        "required": ["command"]
      }
    }
  ],
  "messages": [...]
}
```

## Tool Use Response

Claude가 도구를 호출하면 `stop_reason: "tool_use"`와 함께 응답합니다.

```json
{
  "id": "msg_01XAbCDeFgHiJkLmNoPQrStU",
  "model": "claude-sonnet-4-5-20250929",
  "stop_reason": "tool_use",
  "role": "assistant",
  "content": [
    {
      "type": "text",
      "text": "파일을 읽어보겠습니다."
    },
    {
      "type": "tool_use",
      "id": "toolu_01AbCdEfGhIjKlMnOpQrStU",
      "name": "Read",
      "input": {
        "file_path": "/path/to/file.rs"
      }
    }
  ]
}
```

## Tool Result (다음 요청)

도구 실행 결과를 `tool_result`로 전송합니다.

```json
{
  "role": "user",
  "content": [
    {
      "type": "tool_result",
      "tool_use_id": "toolu_01AbCdEfGhIjKlMnOpQrStU",
      "content": "파일 내용..."
    }
  ]
}
```

**에러 발생 시:**
```json
{
  "type": "tool_result",
  "tool_use_id": "toolu_01AbCdEfGhIjKlMnOpQrStU",
  "content": "Error: File not found",
  "is_error": true
}
```

## Streaming Tool Use

스트리밍 시 tool_use 블록은 다음과 같이 전송됩니다:

```
event: content_block_start
data: {"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"toolu_01xxx","name":"Read","input":{}}}

event: content_block_delta
data: {"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":""}}

event: content_block_delta
data: {"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"file_path\":"}}

event: content_block_delta
data: {"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":" \"/path/to"}}

event: content_block_delta
data: {"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"/file.rs\"}"}}

event: content_block_stop
data: {"type":"content_block_stop","index":1}
```

**주의**: `input_json_delta`는 partial JSON string입니다. 누적해서 파싱해야 합니다.

## Claude Code Built-in Tools

| Tool | 토큰 수 | 용도 |
|------|--------|------|
| Bash | 1,067 | Shell 명령 실행 |
| Read | ~200 | 파일 읽기 |
| Write | ~200 | 파일 쓰기 |
| Edit | 278 | 파일 수정 (str_replace) |
| Glob | ~150 | 파일 패턴 검색 |
| Grep | ~200 | 콘텐츠 검색 |
| TodoWrite | 2,167 | 작업 목록 관리 |
| Task | ~300 | Subagent 실행 |
| WebFetch | ~150 | URL 콘텐츠 가져오기 |
| WebSearch | ~150 | 웹 검색 |

## Text Editor Tool (Anthropic 공식)

Anthropic에서 제공하는 텍스트 에디터 도구.

**정의:**
```json
{
  "type": "text_editor_20250728",
  "name": "str_replace_based_edit_tool"
}
```

**Commands:**
- `view`: 파일/디렉토리 보기
- `str_replace`: 텍스트 교체
- `create`: 새 파일 생성
- `insert`: 특정 라인에 삽입

**view 예시:**
```json
{
  "type": "tool_use",
  "name": "str_replace_based_edit_tool",
  "input": {
    "command": "view",
    "path": "src/main.rs"
  }
}
```

**str_replace 예시:**
```json
{
  "type": "tool_use",
  "name": "str_replace_based_edit_tool",
  "input": {
    "command": "str_replace",
    "path": "src/main.rs",
    "old_str": "fn old_function()",
    "new_str": "fn new_function()"
  }
}
```

## Multi-Tool Response

Claude는 한 응답에서 여러 도구를 호출할 수 있습니다:

```json
{
  "content": [
    {"type": "text", "text": "두 파일을 동시에 읽겠습니다."},
    {
      "type": "tool_use",
      "id": "toolu_01...",
      "name": "Read",
      "input": {"file_path": "/path/a.rs"}
    },
    {
      "type": "tool_use",
      "id": "toolu_02...",
      "name": "Read",
      "input": {"file_path": "/path/b.rs"}
    }
  ],
  "stop_reason": "tool_use"
}
```

**결과 전송:**
```json
{
  "role": "user",
  "content": [
    {"type": "tool_result", "tool_use_id": "toolu_01...", "content": "파일 A 내용"},
    {"type": "tool_result", "tool_use_id": "toolu_02...", "content": "파일 B 내용"}
  ]
}
```

## References

- https://platform.claude.com/docs/en/agents-and-tools/tool-use/overview
- https://platform.claude.com/docs/en/agents-and-tools/tool-use/text-editor-tool
- https://platform.claude.com/docs/en/agents-and-tools/tool-use/bash-tool
