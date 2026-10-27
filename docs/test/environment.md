# 테스트 환경 확인 결과

확인일: 2026-10-27. 저장소 상태와 실행 결과를 구분한다. 테스트를 시작하지 못한 경우 애플리케이션 실패로 분류하지 않는다.

## 상태 요약

| 대상 | 상태 | 확인 내용 |
|---|---|---|
| 현재 `web-agentic-memory` 저장소 | local vertical slices 실행 가능 | Rust workspace에 core/contracts/server/client/MCP/CLI가 있고, seeded in-memory catalog/profile API와 SvelteKit leaderboard/profile/rank/contribution 화면이 있다. Contribution/review, owner-bound usage receipt, successful-use score ledger는 개발 서버의 JSON file adapter를 사용한다. 서버에는 GitHub OAuth/device-link state-machine 경로와 remote `POST /mcp` Streamable HTTP가 있지만 production credential, OAuth discovery, host authentication smoke, GET/SSE, PostgreSQL, Gatekeeper publish는 남아 있다. |
| 참조 Rust 사본 | 실행 불가 | `reference/agent-browser`는 일부 crate와 스크립트만 복사되어 workspace 루트 `Cargo.toml`가 없음 |
| 전체 MeshCode API checkout | 테스트 가능 | sibling `meshcode/packages/billing`에서 web-memory 서버 테스트 통과 |
| 전체 agent-browser checkout | 집중 테스트 통과 | `browser_memory` 관련 integration 53개 통과; 전체 workspace 테스트는 실행하지 않음 |
| 브라우저/E2E | 미실행 | 앱·계정이 필요한 E2E 대신 이번에는 서버/API 회귀 범위만 실행 |

## 도구·설정

- `rustc 1.97.1`, `cargo 1.97.1`, stable `aarch64-apple-darwin`.
- `bun 1.4.0` (MeshCode package), `node 26.5.0`, `npm 11.17.0`, `pnpm 12.3.4`.
- `python3 3.14.7`; `pytest` 미설치. Python harness dependency 여부는 별도 확인 전이다.
- 이 저장소에 `.meshcode/mcp.json`은 없다. 사용자 설정에는 `arise-browser`와 `google-slides` stdio 명령이 등록되어 있고 executable 확인을 마쳤다. 이는 Google OAuth 완료나 새 `wam-mcp` 등록·인증을 증명하지 않는다.
- Workspace Cargo manifest/lockfile과 각 Rust crate manifest가 있다. SvelteKit 앱은 `web/`에 있으며 Vite proxy가 API를 127.0.0.1:3000으로 보낸다. CI workflow는 아직 없다.

## 실행 결과

### 현재 저장소 Rust workspace 테스트

2026-10-27 현재 작업 트리에서 전체 Rust workspace, remote MCP 회귀, strict lint/format, SvelteKit 검사와 빌드를 확인했다.

저장소 루트에서 실행:

```sh
cargo test --workspace
```

`cargo test --workspace --quiet`: **152 pass, 0 fail, 2 ignored** — CLI 5, HTTP client 3, core 24, Gatekeeper 87, MCP 5, server 28. 이전 remote MCP scope 실패는 수정된 뒤 full workspace run에서 통과했다. ignored 2건은 실제 Claude/Codex CLI 로그인과 모델 호출이 필요한 provider smoke다. `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check`, `pnpm --dir web run check` (0 errors/0 warnings), `pnpm --dir web run build`도 통과했다. Docker daemon에 연결할 수 없어 PostgreSQL 통합 테스트는 실행하지 않았다.

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

### 웹 static app check

`pnpm --dir web run check` 결과 **0 errors, 0 warnings**. `pnpm --dir web run build` 성공. 동적 `/u/{handle}` 라우트는 adapter fallback으로 제공하며 배포 서버의 fallback rewrite 설정은 별도다. 실제 브라우저 user-flow는 실행하지 않았다.

### Remote MCP 확인 (2026-10-27)

- `cargo test -p wam-server --test mcp`: 3 pass. 전체 workspace 테스트에서도 remote MCP를 포함해 통과했다.
- `cargo clippy --workspace --all-targets -- -D warnings` 및 `cargo fmt --all -- --check`: 통과.
- `pnpm --dir web run check`와 `pnpm --dir web run build`: 통과. 실제 MCP Inspector나 Claude/Codex/Arise host 등록 및 로그인 smoke는 실행하지 않았다.
- 전송은 POST 기반 stateless JSON이다. GET/SSE와 표준 OAuth discovery는 미구현이다.

## 남은 구현 환경 게이트

1. 새 Web Agentic Memory 구현은 현재 저장소의 Cargo workspace를 canonical source로 사용한다. `reference/`는 read-only이고 sibling checkout은 기존 동작 회귀 기준으로만 사용한다.
2. MCP 호스트는 등록 → 프로세스 시작 → 인증 → tool call을 각각 확인한다. config의 존재만으로 연결된 것으로 치지 않는다.
3. API·UI·MCP E2E는 loopback, 합성 계정, 일회용 저장소, 독립 oracle을 사용한다.
4. PostgreSQL adapter, production auth wiring, Gatekeeper publish와 score/outbox transaction이 추가되면 해당 경계의 manifest, disposable integration test, CI 명령을 더한다. 현재 contribution submit/list/detail/withdraw, receipt/outcome, local score ledger는 demo token과 단일 개발 서버의 로컬 JSON file 저장을 제공한다.

현재 workspace test, strict Clippy, fmt, SvelteKit type/build 검사가 모두 통과한다. 아직 Docker daemon이 없어 PostgreSQL race/migration 검증을 못 했고, live GitHub OAuth 및 MCP host 등록·인증 smoke, 브라우저 end-to-end user-flow는 실행하지 않았다.

## 하네스 실행 결과 (로컬 Chrome, Arise)

확인일: 2026-10-27. 제품 코드가 아니라 테스트 환경을 확인한 결과다 ([`harness-verification.md`](harness-verification.md)).

| 대상 | 결과 |
|---|---|
| 로컬 Chrome 156.0.8078.48 | 일회용 프로필과 임시 포트로 헤드리스 실행. fixture 4종 × 행동 32개에서 oracle 판정이 기대와 32/32 일치, 정리 후 잔존 0 |
| 로컬 Arise 0.1.54 | 설치된 바이너리를 제자리에서 격리 실행(복사본은 `SIGKILL`로 종료됨). 같은 32개 시나리오 32/32 일치, 잔존 0, 실행 중이던 사용자 Arise 프로세스 불변. 창이 숨김 상태로 시작해 포커스 에뮬레이션이 필요하다 ([`hosts/arise.md`](hosts/arise.md)) |
| 하네스 단위 테스트 | `python3 -B -m unittest discover -s tests/lib/selftest -t tests/lib`: 62개 통과 (브라우저 없이) |
| Docker | CLI는 있으나 데몬(Colima)이 꺼져 있다. 일회용 Postgres가 필요한 `DB-*`, `QUE-*` 통합 테스트를 돌리기 전에 `colima start`가 필요하다. 시작하지 않았다 |
| 미확인 | `wam-mcp` 바이너리는 있으나 Claude Code/Codex의 등록·인증·실제 tool call 경로를 확인하지 않았다. Chrome tab-aware WAM adapter, Arise MCP Hub adapter, Meta Muse provider도 미구현/미검증이라 SMK와 BEH는 실행하지 않았다 |
