# Claude Code System Prompt Structure

## Overview

Claude Code는 단일 거대한 system prompt가 아닌 **110+ 모듈형 컴포넌트**로 구성됩니다.
상황에 따라 동적으로 주입됩니다.

## Component Categories

### 1. Main System Prompt (269 tokens)

Claude Code의 기본 정체성 정의:

```
You are Claude Code, Anthropic's official CLI for Claude.
You are an interactive CLI tool that helps users with software engineering tasks.
...
```

### 2. Tool Descriptions (22개)

| Tool | Tokens | 설명 |
|------|--------|------|
| Bash | 1,067 | Shell 명령 실행 |
| TodoWrite | 2,167 | 작업 목록 관리 |
| TeammateTool | 2,221 | 팀원 협업 |
| Task | ~300 | Subagent 실행 |
| Read | ~200 | 파일 읽기 |
| Write | ~200 | 파일 쓰기 |
| Edit | 278 | 파일 수정 |
| Glob | ~150 | 파일 검색 |
| Grep | ~200 | 콘텐츠 검색 |
| WebFetch | ~150 | URL 가져오기 |
| WebSearch | ~150 | 웹 검색 |
| AskUserQuestion | 194 | 사용자 질문 |
| EnterPlanMode | 970 | 계획 모드 진입 |
| ExitPlanMode | ~200 | 계획 모드 종료 |
| NotebookEdit | ~200 | Jupyter 노트북 |
| KillShell | ~100 | 셸 종료 |
| Computer | 161 | 브라우저 자동화 |

### 3. Agent Prompts (Subagent용)

| Agent | Tokens | 용도 |
|-------|--------|------|
| Explore | 516 | 코드베이스 탐색 |
| Plan Mode Enhanced | 633 | 전략적 계획 |
| Task Tool | 294 | 특화 작업 실행 |

### 4. Creation Assistants

| Assistant | Tokens | 용도 |
|-----------|--------|------|
| Agent Creation Architect | 1,110 | 커스텀 Agent 생성 |
| CLAUDE.md Creation | 384 | 문서 생성 |
| Status Line Setup | 1,460 | 상태줄 설정 |

### 5. Slash Commands

| Command | Tokens | 용도 |
|---------|--------|------|
| /security-review | 2,610 | 보안 취약점 분석 |
| /review-pr | 243 | PR 리뷰 |
| /pr-comments | 402 | GitHub 코멘트 |

### 6. System Reminders (~40개)

컨텍스트별 알림. 18~1,348 토큰.

- 파일 수정 알림
- Plan mode 상태
- 토큰 사용량 경고
- Hook 실행 피드백
- Git 상태 정보

### 7. Utility Prompts

| Utility | Tokens | 용도 |
|---------|--------|------|
| Conversation Summarization | 1,121 | 컨텍스트 압축 |
| Bash Command Processing | ~300 | 명령 처리 |
| WebFetch Summarization | ~200 | URL 요약 |

## Dynamic Injection

컴포넌트는 환경과 설정에 따라 **조건부로 주입**됩니다:

```
├─ 항상 포함
│   ├─ Main System Prompt
│   ├─ Tool Descriptions (활성화된 것만)
│   └─ 기본 System Reminders
│
├─ 조건부 포함
│   ├─ Plan Mode 프롬프트 (plan mode일 때)
│   ├─ CLAUDE.md 내용 (존재할 때)
│   ├─ Git 상태 (git repo일 때)
│   └─ Slash Command 프롬프트 (호출 시)
│
└─ Subagent 전용
    └─ Agent-specific System Prompt
```

## Token Interpolation

도구 이름, 컨텍스트 변수 등이 런타임에 삽입됨 (±20 토큰 변동).

## Example: Full Request Structure

```json
{
  "model": "claude-opus-4-5-20251101",
  "system": "You are Claude Code, Anthropic's official CLI...\n\n# Tone and style\n...\n\n# Tool usage policy\n...\n\nAvailable agent types:\n- Explore: Fast agent...\n- Plan: Software architect...\n\n<env>\nWorking directory: /Users/...\nPlatform: darwin\n</env>\n\ngitStatus: ...",
  "tools": [
    {"name": "Task", "description": "...", "input_schema": {...}},
    {"name": "Read", "description": "...", "input_schema": {...}},
    {"name": "Edit", "description": "...", "input_schema": {...}},
    {"name": "Bash", "description": "...", "input_schema": {...}},
    {"name": "Glob", "description": "...", "input_schema": {...}},
    {"name": "Grep", "description": "...", "input_schema": {...}},
    {"name": "TodoWrite", "description": "...", "input_schema": {...}},
    {"name": "WebSearch", "description": "...", "input_schema": {...}},
    {"name": "WebFetch", "description": "...", "input_schema": {...}},
    {"name": "AskUserQuestion", "description": "...", "input_schema": {...}},
    {"name": "EnterPlanMode", "description": "...", "input_schema": {...}},
    {"name": "ExitPlanMode", "description": "...", "input_schema": {...}}
  ],
  "messages": [
    {"role": "user", "content": "..."},
    {"role": "assistant", "content": "..."},
    ...
  ]
}
```

## Subagent System Prompt

Subagent는 Main과 다른 system prompt를 받습니다:

```
# Explore Agent Example

You are a fast, read-only agent specialized for exploring codebases.

Your goal is to quickly find files, search code, and answer questions
about the codebase without making any modifications.

Thoroughness level: [quick|medium|very thorough]

Available tools: Glob, Grep, Read

You cannot spawn other subagents.
```

## References

- https://github.com/Piebald-AI/claude-code-system-prompts
- https://weaxsey.org/en/articles/2025-10-12/
