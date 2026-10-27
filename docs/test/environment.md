# 테스트 환경 확인 결과

확인일: 2026-10-27. 저장소 상태와 실행 결과를 구분한다. 테스트를 시작하지 못한 경우 애플리케이션 실패로 분류하지 않는다.

## 상태 요약

| 대상 | 상태 | 확인 내용 |
|---|---|---|
| 현재 `web-agentic-memory` 저장소 | core crate 실행 가능 | 루트 Cargo workspace에 `wam-core`가 있고 순수 도메인 규칙(URL/recall/profile/leaderboard)과 integration tests를 실행할 수 있음. HTTP server, MCP adapter, CLI, web app은 아직 scaffold |
| 참조 Rust 사본 | 실행 불가 | `reference/agent-browser`는 일부 crate와 스크립트만 복사되어 workspace 루트 `Cargo.toml`가 없음 |
| 전체 MeshCode API checkout | 테스트 가능 | sibling `meshcode/packages/billing`에서 web-memory 서버 테스트 통과 |
| 전체 agent-browser checkout | 집중 테스트 통과 | `browser_memory` 관련 integration 53개 통과; 전체 workspace 테스트는 실행하지 않음 |
| 브라우저/E2E | 미실행 | 앱·계정이 필요한 E2E 대신 이번에는 서버/API 회귀 범위만 실행 |

## 도구·설정

- `rustc 1.97.1`, `cargo 1.97.1`, stable `aarch64-apple-darwin`.
- `bun 1.4.0` (MeshCode package), `node 26.5.0`, `npm 11.17.0`, `pnpm 12.3.4`.
- `python3 3.14.7`; `pytest` 미설치. Python harness dependency 여부는 별도 확인 전이다.
- 이 저장소에 `.meshcode/mcp.json`은 없다. 사용자 설정에는 `arise-browser`와 `google-slides` stdio 명령이 등록되어 있고 executable 확인을 마쳤다. 이는 Google OAuth 완료나 새 `wam-mcp` 등록·인증을 증명하지 않는다.
- `wam-core`용 Cargo manifest와 lockfile은 생겼다. CI workflow, server/MCP/CLI crate manifest, web package manifest는 아직 없다.

## 실행 결과

### 현재 저장소 Rust core 테스트

저장소 루트에서 실행:

```sh
cargo test --workspace
```

결과: **24 pass, 0 fail** — leaderboard 3, memory index 2, profile 6, recall 3, URL scope 7, vocabulary 3. 순위·프로필 입력 검증·URL 범위·비밀값 제거와 closed vocabulary를 외부 DB/HTTP 없이 검증한다.

### 웹/API 테스트

원본 `/Users/jason2/projects/ai-coding/meshcode/packages/billing`에서 실행:

```sh
BILLING_SCRYPT_N=1024 bun test test/web-memory.test.ts
```

결과: **11 pass, 0 fail, 104 assertions, 0.424s**. 승인 catalog, quarantine report, 입력 상한/속도 제한, 관리자 인증, evidence digest, 게시 및 rollback을 확인했다.

### Rust focused 테스트

참조 사본에서 `reference/agent-browser/scripts/test-browser-memory.sh`를 실행했을 때 workspace `Cargo.toml` 부재로 종료 코드 101을 반환했다.

전체 `/Users/jason2/projects/ai-coding/agent-browser` checkout에서 아래 집중 명령을 완료했다.

```sh
cd /Users/jason2/projects/ai-coding/agent-browser
CARGO_BUILD_JOBS=4 ./scripts/test-browser-memory.sh
```

결과: **53 pass, 0 fail** — account fixture 4, membership 4, memory store 18, tool contract 7, turn context 4, web catalog 10, workflow reports 6. 첫 빌드는 의존성 compile에 약 8분 33초 걸렸다. 이 스크립트는 테스트에만 `AGENT_BROWSER_NO_KEYCHAIN=1`을 적용한다. 전체 agent-browser 작업 트리에는 이 요청과 무관한 `scripts/r2-prune*` 미커밋 변경이 있어 수정하지 않았다. 이 결과는 집중 crate의 테스트이지 전체 앱/workspace 통과를 뜻하지 않는다.

## 남은 구현 환경 게이트

1. 새 Web Agentic Memory 구현은 현재 저장소의 Cargo workspace를 canonical source로 사용한다. `reference/`는 read-only이고 sibling checkout은 기존 동작 회귀 기준으로만 사용한다.
2. MCP 호스트는 등록 → 프로세스 시작 → 인증 → tool call을 각각 확인한다. config의 존재만으로 연결된 것으로 치지 않는다.
3. API·UI·MCP E2E는 loopback, 합성 계정, 일회용 저장소, 독립 oracle을 사용한다.
4. server, MCP, CLI, web adapter가 추가될 때 crate/package manifest와 CI 명령을 해당 실행 경계에 맞춰 추가한다.

현재 core 계약 테스트(24개)는 이 저장소에서 실행 가능하다. 독립 API server, MCP smoke, SvelteKit UI, 브라우저 user flow는 아직 실행할 앱이 없어 미실행이다.

## 하네스 실행 결과 (로컬 Chrome, Arise)

확인일: 2026-10-27. 제품 코드가 아니라 테스트 환경을 확인한 결과다 ([`harness-verification.md`](harness-verification.md)).

| 대상 | 결과 |
|---|---|
| 로컬 Chrome 156.0.8078.48 | 일회용 프로필과 임시 포트로 헤드리스 실행. fixture 4종 × 행동 32개에서 oracle 판정이 기대와 32/32 일치, 정리 후 잔존 0 |
| 로컬 Arise 0.1.54 | 설치된 바이너리를 제자리에서 격리 실행(복사본은 `SIGKILL`로 종료됨). 같은 32개 시나리오 32/32 일치, 잔존 0, 실행 중이던 사용자 Arise 프로세스 불변. 창이 숨김 상태로 시작해 포커스 에뮬레이션이 필요하다 ([`hosts/arise.md`](hosts/arise.md)) |
| 하네스 단위 테스트 | `python3 -B -m unittest discover -s tests/lib/selftest -t tests/lib`: 62개 통과 (브라우저 없이) |
| Docker | CLI는 있으나 데몬(Colima)이 꺼져 있다. 일회용 Postgres가 필요한 `DB-*`, `QUE-*` 통합 테스트를 돌리기 전에 `colima start`가 필요하다. 시작하지 않았다 |
| 미확인 | Claude Code, Codex, Meta Muse 호스트 어댑터와 `wam-mcp`가 아직 없어 SMK와 BEH는 실행하지 않았다 |
