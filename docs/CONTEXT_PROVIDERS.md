# Context Providers

SwiftCast의 Context Provider 시스템을 사용하면 외부 서비스에서 프로젝트 지식을 가져와 Claude Code 세션에 자동으로 주입할 수 있습니다.

## 개요

Claude Code에서 대화가 컨텍스트 한계에 도달하면 `/compact` 명령으로 세션을 요약하고 새 세션으로 이어갑니다. 이때 Context Provider를 통해 외부 서비스(ThreadCast, Notion, 사내 Wiki 등)에서 프로젝트 관련 지식을 가져와 새 세션에 자동으로 주입합니다.

```
Claude Code 세션 컴팩션
       ↓
SwiftCast가 등록된 Provider들 호출
       ↓
각 Provider에서 Context 수집
       ↓
새 세션에 자동 주입
```

## 설정 위치

```
~/.config/swiftcast/context_providers/
├── threadcast.toml     # ThreadCast 연동
├── notion.toml         # Notion 연동
└── custom.toml         # 사용자 정의
```

## 설정 파일 형식

```toml
[provider]
name = "My Provider"        # 표시 이름
enabled = true              # 활성화 여부
type = "http"               # 현재 http만 지원

[http]
method = "GET"              # HTTP 메서드
url = "http://api.example.com/knowledge/${project_id}"
timeout_secs = 5            # 타임아웃 (초)

[http.headers]              # 요청 헤더
Authorization = "Bearer ${API_TOKEN}"

[response]
path = "data.knowledge"     # JSON 응답에서 추출할 경로

[output]
template = """              # 출력 템플릿
<knowledge>
{{data}}
</knowledge>
"""

[variables]                 # 변수 정의
project_id = "my-project"
```

## 변수 치환

URL, 헤더, 템플릿에서 `${variable}` 형식으로 변수를 사용할 수 있습니다:

1. **설정 파일 변수**: `[variables]` 섹션에 정의
2. **환경 변수**: 시스템 환경 변수 사용 가능

```toml
[http]
url = "http://api.example.com/${PROJECT_ID}/meta"

[http.headers]
Authorization = "Bearer ${API_TOKEN}"  # 환경 변수에서

[variables]
PROJECT_ID = "my-project"  # 설정 파일에서
```

## ThreadCast 연동 예시

```toml
# ~/.config/swiftcast/context_providers/threadcast.toml

[provider]
name = "ThreadCast Knowledge"
enabled = true
type = "http"

[http]
method = "GET"
url = "http://localhost:21000/api/workspaces/${workspace_id}/meta"
timeout_secs = 5

[response]
path = "data.knowledge"

[output]
template = """
<project-knowledge source="threadcast">
{{#each this}}
### {{@key}}
{{this.summary}}
{{/each}}
</project-knowledge>
"""

[variables]
workspace_id = "57c6b945-00de-4a0b-9b4c-b20d5369e833"
```

## 디버깅

SwiftCast 로그에서 Context Provider 동작을 확인할 수 있습니다:

```
[CompactionInjector] Detected compacted conversation, injecting context (static: true, providers: true)
Added context from provider: ThreadCast Knowledge
```

## 주입되는 형식 예시

```
This session is being continued from a previous conversation...

<project-knowledge source="threadcast">
### aws-deployment
ThreadCast AWS 배포: JAR 빌드 후 SCP로 EC2 전송

### coding-conventions
프로젝트 코딩 컨벤션 - conventional commits 사용
</project-knowledge>

## Persistent Context (Always Remember):
정적 컨텍스트 내용...
```

## 지원 예정 기능

- [ ] 파일 기반 Provider (`type = "file"`)
- [ ] 주기적 캐싱
- [ ] Provider별 조건부 활성화
- [ ] 템플릿 엔진 확장 (Handlebars 완전 지원)
