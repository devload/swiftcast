# SwiftCast

**Claude와 GLM을 자유롭게 스위칭하고 사용량을 모니터링하는 데스크톱 프로그램**

## ✨ 핵심 기능

1. **Provider 스위칭**: Claude ↔ GLM 간 한 번의 클릭으로 전환
2. **사용량 모니터링**: Provider별 토큰, 비용, 시간 추적
3. **프록시 서버**: Claude Code가 선택된 Provider 사용

## 🏗️ 기술 스택

- **Backend**: Rust (Tauri 2.x, axum, SQLite)
- **Frontend**: React + TypeScript + Vite + TailwindCSS
- **플랫폼**: Windows + macOS (크로스 플랫폼)

## 📋 요구사항

- ✅ **크로스 플랫폼**: Windows, macOS에서 동일하게 작동
- ✅ **런타임 독립**: JRE, Node.js 등 추가 설치 불필요
- ✅ **단독 실행**: 더블클릭으로 즉시 실행
- ✅ **작은 크기**: 5-10MB (Electron 대비 1/20)

## 🚀 사용 방법

### 1. 개발 모드 실행

```bash
npm install
npm run tauri:dev
```

### 2. 릴리스 빌드

```bash
npm run tauri:build
```

## 📝 핵심 작동 원리

### Claude Code 설정

**파일**:
- macOS: `~/.claude/settings.json`
- Windows: `%APPDATA%\Claude\settings.json`

```json
{
  "env": {
    "ANTHROPIC_BASE_URL": "http://localhost:32080"
  }
}
```

**참고**: Claude(Anthropic)로 전환 시 settings.json이 자동 삭제되어 공식 API를 직접 사용합니다.

### 전체 흐름

```
Claude Code
    ↓ (HTTP 요청)
SwiftCast Proxy (localhost:32080)
    ↓ (활성 계정 확인)
    ├─→ Anthropic API (OAuth 토큰 패스스루)
    └─→ GLM API (저장된 API 키 사용)
    ↓ (사용량 기록 - 토큰 추적)
    ↓ (응답 전달)
Claude Code
```

## 📁 프로젝트 구조

```
swiftcast/
├── src/                    # Frontend (React)
│   ├── components/
│   │   ├── Dashboard.tsx       # 메인 대시보드
│   │   ├── AccountManager.tsx  # 계정 관리
│   │   ├── UsageMonitor.tsx    # 사용량 모니터링 (탭: 개요/모델별/일별/로그)
│   │   └── Settings.tsx        # 설정 (포트, 자동시작)
│   ├── App.tsx
│   └── main.tsx
│
├── src-tauri/             # Backend (Rust)
│   ├── src/
│   │   ├── models/        # 데이터 모델
│   │   ├── storage/       # 데이터베이스 (SQLite)
│   │   ├── proxy/         # 프록시 서버 (axum + SSE)
│   │   ├── commands/      # Tauri commands
│   │   └── main.rs
│   └── Cargo.toml
│
├── 핵심_작동_원리.md       # 상세 설명
├── 핵심_미션_정리.md       # 프로젝트 목표
├── 프로젝트_목표.md        # 전체 비전
└── 개발현황.md             # 개발 상태
```

## 🎯 사용 시나리오

### 시나리오 1: 평소 Claude, 간단한 작업 GLM
```
1. Anthropic, GLM 계정 모두 등록
2. 평소: Anthropic 활성화
3. 간단한 작업: GLM으로 전환
4. 각 Provider별 사용량 확인
```

### 시나리오 2: GLM 주력, Claude 백업
```
1. GLM을 주 계정으로 설정
2. GLM 한도 도달 시 Anthropic으로 전환
3. 통계로 총 비용 확인
```

## 📚 문서

- [핵심 작동 원리](./핵심_작동_원리.md) - 프록시 작동 방식 상세 설명
- [핵심 미션 정리](./핵심_미션_정리.md) - 프로젝트 목표 요약
- [프로젝트 목표](./프로젝트_목표.md) - 전체 비전 및 로드맵
- [개발 현황](./개발현황.md) - 현재 개발 상태

## 🔧 개발 가이드

### 계정 추가 (개발용)

```rust
// src-tauri/examples/add_account.rs 참고
cargo run --example add_account "Account Name" "https://api.example.com" "api-key"
```

### 프록시 테스트

```bash
curl -X POST http://localhost:32080/v1/messages \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-sonnet-4-5-20250929",
    "max_tokens": 100,
    "messages": [{"role": "user", "content": "안녕하세요"}]
  }'
```

## 💡 핵심 가치

1. **유연성** - 상황에 맞는 Provider 선택
2. **투명성** - 명확한 사용량 추적
3. **편의성** - Desktop UI로 간단하게
4. **비용 최적화** - 선택을 통한 비용 절감

## 📄 라이선스

MIT License

## 🤝 기여

Issues와 Pull Requests를 환영합니다!
