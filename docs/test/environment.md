# 테스트 환경 확인 결과

확인일: 2026-10-28. 저장소 상태와 실행 결과를 구분한다. 테스트를 시작하지 못한 경우 애플리케이션 실패로 분류하지 않는다.

## 상태 요약

| 대상 | 상태 | 확인 내용 |
|---|---|---|
| 현재 `web-agentic-memory` 저장소 | 핵심 로컬 slice 구현 | Rust workspace에 durable PostgreSQL repository/migrations, Redis rate-limit adapter, client/MCP/CLI cache, SvelteKit leaderboard/profile/rank/usage/analytics 화면이 있다. Production 모드는 `WAM_DATABASE_URL`과 `WAM_RATE_LIMIT_REDIS_URL`을 요구하고, local mode는 디렉터리 snapshot과 in-memory limiter를 쓴다. GitHub OAuth/device flow와 remote `POST /mcp`, 표준 MCP discovery/DCR/PKCE/refresh/revoke를 구현했다. 합성 계정의 실제 Chrome 동의·해지와 격리 native Codex OAuth login을 검증했으며, GET/POST SSE와 macOS Arise browser MCP bridge도 구현·검증했다. Live GitHub·Arise/Claude Hub remote 인증과 다른 OS runtime은 별도다. [OAuth 증거](remote-oauth.md). |
| PostgreSQL/Redis 통합 환경 | PostgreSQL 확인, disposable Redis 통합 통과 | 2026-10-27: 임시 격리 PostgreSQL DB 테스트가 통과했다. Homebrew Redis 8.10.3을 port 16379에서 persistence 없이 실행해 별도 연결 간 quota 경쟁/denial 집계, 다중 bucket 원자성, account-deletion fence를 확인했다 (Redis 테스트 3 pass). 저장소 오류 middleware 테스트는 Retry-After가 없음을 먼저 재현한 뒤 503/1초 재시도 신호를 확인했다 (1 pass). 실행 후 임시 Redis는 종료했다. `WAM_TEST_DATABASE_URL`/`WAM_TEST_REDIS_URL`은 셸에 영구 설정하지 않았고 Docker daemon은 연결되지 않았다. Redis outage/failover와 배포 proxy 검증은 미실행이다. |
| 참조 Rust 사본 | 실행 불가 | `reference/agent-browser`는 일부 crate와 스크립트만 복사되어 workspace 루트 `Cargo.toml`가 없음 |
| 전체 MeshCode API checkout | 테스트 가능 | sibling `meshcode/packages/billing`에서 web-memory 서버 테스트 통과 |
| 전체 agent-browser checkout | 집중 테스트 통과 | `browser_memory` 관련 integration 53개 통과; 전체 workspace 테스트는 실행하지 않음 |
| 브라우저/E2E | 로컬 lifecycle/OAuth/fixture 통과 | 격리 web·CLI·MCP·게시/교체·계정 수명주기 1개와 Chrome OAuth/프레임/해지 2개 통과. 독립 editor oracle 40/40, UI drift 6/6와 정리 검사도 통과했다. 실제 GitHub·Claude/Arise 운영 인증 및 agent 행동 검증과 구분한다 |

## 도구·설정

- `rustc 1.97.1`, `cargo 1.97.1`, stable `aarch64-apple-darwin`.
- `bun 1.4.0` (MeshCode package), `node 26.5.0`, `npm 11.17.0`, `pnpm 12.3.4`.
- `python3 3.14.7` 기록 기준; Python 하네스는 표준 라이브러리만으로 104개 테스트를 통과했다. pytest는 필요하지 않다.
- `.meshcode/mcp.json`은 `.gitignore` 대상인 로컬 설정이다. 2026-11-03 기준 이 작업 트리에는 `arise-browser` 프로젝트 항목이 있고, 프로젝트 항목은 사용자 설정(`~/.meshcode/mcp.json`)의 같은 이름 항목을 **통째로 덮어쓴다**(`google-slides`는 사용자 설정 그대로). 사용자 설정에는 `arise-browser`와 `google-slides` stdio 명령이 등록되어 있고 executable 확인을 마쳤다. 이는 Google OAuth 완료나 새 `wam-mcp` 등록·인증을 증명하지 않는다. 프로젝트 `arise-browser` 항목은 `TMPDIR`을 고정하고 실제 Arise 프로필을 공유하며, `command`는 agent-browser `master`(`2ed19b9`) 빌드 사본(`~/.cache/arise-wam-fixed/bin/agent_browser`)을 가리킨다(원래 서명 번들 바이너리는 `.meshcode/mcp.json.bak-20261103-before-arise-mcp-fix`). 그래서 벤치의 MCP 버전은 이 사본이다. 설치된 Arise가 떠 있을 때 구버전 MCP가 앱을 못 찾는 문제는 [Arise 호스트 문서의 함정 7](hosts/arise.md)을 본다.
- Workspace Cargo manifest/lockfile과 각 Rust crate manifest가 있다. SvelteKit 앱은 `web/`에 있으며 Vite proxy가 API를 127.0.0.1:3000으로 보낸다. 2026-10-30 사용자 요청으로 GitHub Actions 워크플로우를 제거했다. Rust/DB/Redis, Python, web/Chrome 검사는 각 테스트 문서의 명령으로 로컬에서 실행한다.

## 실행 결과

### 현재 저장소 Rust workspace 테스트

2026-10-28 현재 작업 트리에서 전체 Rust workspace, disposable PostgreSQL/Redis, remote MCP 회귀, strict lint/format, SvelteKit 검사·테스트·빌드를 다시 확인했다. 아래 sibling checkout 결과는 이전 검증 기록이다.

저장소 루트에서 실행:

```sh
cargo test --workspace
```

`cargo test --workspace --locked --quiet`: **316 pass / 0 fail / 22 ignored** (새 synthetic security corpus 통합 케이스 1개 포함). strict workspace Clippy와 전체 rustfmt 검사도 통과했다. 이전 #25 인자/필드/포맷 불일치는 현재 검사에서 해소됐다. 별도 disposable PostgreSQL은 application/migration/search 10 + diagnostics 2 + Gatekeeper queue 3 + store 1로 **16 pass**, isolated Redis limiter/erasure는 **3 pass**였다. 테스트 DB와 Redis는 종료·제거했다. 웹 unit **17 pass**, lifecycle smoke **1 pass**, OAuth browser **2 pass**, check **0 errors/0 warnings**, build 성공을 이번 실행에서 확인했다. Redis failover, 운영 proxy와 live provider/host 인증은 이 결과에 포함하지 않는다.

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

`pnpm --dir web run check` 결과 **0 errors, 0 warnings**. `pnpm --dir web run build` 성공. 동적 `/u/{handle}` 라우트는 adapter fallback으로 제공하며 배포 서버의 fallback rewrite 설정은 별도다. 격리 Chrome lifecycle 1개와 OAuth 2개를 실행해 통과했다.

### Remote MCP 확인 (2026-10-28)

- 현재 MCP/OAuth suite를 포함한 전체 workspace와 strict Clippy/fmt가 통과했다. Protected-resource/AS discovery, DCR, PKCE, refresh/replay revoke, audience/scope binding과 browser CORS는 구현돼 있다.
- 실제 Chrome의 consent/프레임 거부/기기 revoke 2개와 web·CLI·MCP lifecycle 1개를 이번 실행에서 통과했다. 합성 provider와 계정만 썼다.
- 앞선 격리 native Codex OAuth login/private rank+usage/logout 증거는 [remote-oauth.md](remote-oauth.md)에 있다. 이번 실행이 live GitHub나 모든 host acceptance를 추가로 입증하지는 않는다.
- 전송은 stateless JSON 및 SSE다. GET idle SSE와 POST message SSE를 검증했으며 replay/서버 workflow push는 제공하지 않는다. Live GitHub·Claude/Arise remote 인증과 실제 agent 행동은 별도 잔여 범위다.

## 남은 구현 환경 게이트

1. 새 Web Agentic Memory 구현은 현재 저장소의 Cargo workspace를 canonical source로 사용한다. `reference/`는 read-only이고 sibling checkout은 기존 동작 회귀 기준으로만 사용한다.
2. MCP 호스트는 등록 → 프로세스 시작 → 인증 → tool call을 각각 확인한다. config의 존재만으로 연결된 것으로 치지 않는다.
3. API·UI·MCP E2E는 loopback, 합성 계정, 일회용 저장소, 독립 oracle을 사용한다.
4. Local PostgreSQL adapter, migration, query, transaction, queue/store integration tests have run against a disposable database. Redis 8.10.3 was installed and shared limiter/erasure integration tests ran against an isolated, non-persistent local service; the server is stopped and `WAM_TEST_REDIS_URL` is not persistent in the shell. Add Redis outage/failover and deployment-topology checks to CI.

이번 실행에서 전체 workspace test와 strict Clippy/fmt, PostgreSQL race/migration, Redis atomic/shared quota 통합 검증이 모두 통과했다. Python 104개(브라우저 네트워크 가드 7개, retrieval corpus 8개, 보안 corpus 재현성 1개 추가), Rust 316개(합성 악성 힌트 160건 검사 포함), Chrome editor oracle 40/40와 UI drift 6/6, 10회 TTS CV 1.18%/clock divergence 1ms, publish 요청 차단 및 Arise rich 7/7/runtime settings 경로 검증도 확인했다. 500-query synthetic retrieval fixture의 integrity와 scorer 계약은 통과했지만, 사람 검수 relevance와 대규모 DB 지연 측정은 아직 없다. [하네스 검증](harness-verification.md)과 [구현 우선순위](../plan/implementation-priorities.md)에 현재 증거와 모듈화를 기록했다. Redis outage/failover, live GitHub·Claude/Arise 운영 인증, Arise 장시간 자동화, quota/429 browser user-flow와 대규모 검색 부하는 남아 있다.

## 하네스 실행 결과 (로컬 Chrome, Arise)

확인일: 2026-10-27. 제품 코드가 아니라 테스트 환경을 확인한 결과다 ([`harness-verification.md`](harness-verification.md)).

| 대상 | 결과 |
|---|---|
| 로컬 Chrome | 2026-10-28 재실행: editor 5종 40/40, UI drift v1/v2 6/6, 반복 TTS 10회 CV 1.18%, 최대 clock error 1ms. CDP Fetch guard가 publish 요청을 차단해 fixture `publish_count=0` 확인. 일회용 프로필/loopback만 사용 |
| 로컬 Arise 0.1.54 | 설치된 바이너리를 제자리에서 격리 실행(복사본은 `SIGKILL`로 종료됨). 기존 32개 시나리오 32/32, 이번 rich subset 7/7 및 XDG settings runtime read 통과, 잔존 0, 사용자 Arise 프로세스 불변. 창은 숨김 상태로 시작해 포커스 에뮬레이션이 필요하다 ([`hosts/arise.md`](hosts/arise.md)) |
| 하네스 단위 테스트 | `python3 -B -m unittest discover -s tests/lib/selftest -t tests/lib`: 104개 통과 (브라우저 없이) |
| Docker / DB | Docker daemon은 쓰지 않았다. native PostgreSQL에 일회용 DB를 만들고 integration 16개를 통과시킨 후 제거했다. Redis도 별도 임시 process로 3개를 통과시킨 후 종료했다 |
| 잔여 host 검증 | CLI/MCP 등록과 synthetic Codex OAuth 증거는 [cli-link.md](cli-link.md), [remote-oauth.md](remote-oauth.md)에 있다. 실제 모델-host trace adapter, live GitHub·Claude/Arise 인증과 agent behavior acceptance는 미완료 |

## 최신 회귀 및 커버리지 점검 (2026-11-01)

기존 관리자 기능·운영 확인과 별도로 Rust workspace, 웹 JS 단위 테스트, disposable PostgreSQL/Redis 서비스 검사를 다시 실행했다. 전 구간은 synthetic 계정·fixture와 격리된 로컬 서비스만 사용했다.

| 검사 | 결과 | 범위와 한계 |
|---|---|---|
| Rust workspace | `cargo llvm-cov --workspace --summary-only -- --test-threads=1`: 441 pass, 0 fail, 30 ignored; line 73.91%, branch 80.01%, function 77.36% | 직렬 실행. 앞서 병렬 실행에서 CLI loopback test의 `WouldBlock`가 한 번 발생했으나 격리 재실행과 직렬 전체 실행은 통과 |
| 웹 JS 단위 | `node --experimental-test-coverage --test web/tests/*.test.mjs`: 46 pass, 0 fail; line 99.75%, branch 87.05%, function 93.16% | 테스트 대상 plain JS 모듈만 집계한다. Svelte route/component 및 전체 브라우저 앱 커버리지로 해석하지 않는다 |
| 관리자 | `cargo test -p wam-server --test admin_auth`: 9 pass; disposable PostgreSQL admin/client 이메일 테스트 2 pass; Chrome desktop/mobile 4 pass | 이메일과 UI API는 fake. 실메일 받은편지함, 실제 Muse Gateway 판정과 trusted replay 게시 미통과 |
| PostgreSQL/Redis 서비스 | `tests/env/check_data_services.py`, `tests/env/verify_release_services.py` 모두 통과 후 fixture 정리 | 로컬 release rehearsal. 배포 TLS/topology 및 Redis promotion/reconnect/failover 수용은 별도 |

Coverage report에서 PostgreSQL durable/queue adapter와 Redis limiter 일부는 기본 workspace 계측상 0%로 나타난다. 별도 disposable service 테스트가 일부 통합 경로를 확인하지만 전체 adapter branch coverage를 대체하지 않는다. `wam-cli/src/browser.rs` 20.22%, `wam-server/src/contribution_store/memory.rs` 42.59%, `wam-client/src/error.rs` 47.37%도 후속 테스트 후보로 남긴다. 다음 보완은 실제 DB/Redis 오류·재연결·경합 경로를 계측 하네스에서 실행하고, Svelte 화면 상태/오류 분기와 관리자 가입자 목록의 자동 검사를 추가하는 것이다.
