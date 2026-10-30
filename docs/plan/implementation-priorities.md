# 구현 현황과 남은 우선순위

최신 출시 감사(2026-10-29)는 [`release-readiness.md`](../test/release-readiness.md)에 정리했다. 대시보드 인증/요청 경합, SIGTERM 종료, Linux ARM64 release/installer, MCP catalog budget을 보완했고 candidate 증거 판정기를 구현했다. 아래 2026-10-28 검증 수치는 당시 snapshot이며 현재 실행 결과·남은 gate는 최신 감사 문서를 따른다.

확인일: 2026-10-28. 코드, HTTP/MCP/CLI 표면, 웹 경로, 명세, 공유 이슈와 현재 실행 결과를 대조했다. 구현된 기능과 배포·실제 호스트 검증을 구분한다.

## 현재 구현

| 영역 | 구현된 기능 | 검증 근거 |
|---|---|---|
| Core/search | URL·route·task 정규화, 공용 recall ranking, PostgreSQL index 후보 선택, vocab/detail 조회 | core/unit tests, `postgres_recall_candidates_match_the_core_oracle_and_filter_routes_before_loading` |
| API/storage | PostgreSQL application migrations/repository와 local snapshot, OAuth/device state, contribution consent/review, account lifecycle, diagnostics, receipts와 immutable score/outbox transaction | server workspace tests, disposable PostgreSQL integration 10 + diagnostics 2 |
| Gatekeeper | PostgreSQL queue/store, scope lease/fencing, versioned batch, provider interfaces, replay-verified server publication | disposable PostgreSQL queue 3 + store 1, application lifecycle/publication tests |
| Abuse/usage | weighted IP/account quotas, Redis atomic adapter, 429/Retry-After, fail-closed 503, overload shedding, authenticated denied counts, owner usage and per-version analytics | router tests; disposable Redis concurrency, atomic buckets, denial aggregates, and erasure tests; dashboard implementation |
| Client cache | bounded SQLite anonymous recall cache shared by CLI and local stdio MCP; visible stale fallback; status/clear | client tests and MCP cache tool test |
| Web | English-first landing, memory pages, profile/rich bio, rank/leaderboard, contribution/review, usage and author analytics; shared install copy | `pnpm --dir web run test` (17), local Playwright smoke (1), Svelte check/build |
| CLI/MCP | reads/writes, stdin recall, link/unlink/doctor, native secure store, conflict-safe host install/uninstall, offline guide print/marker update; shared stdio/HTTP contracts | workspace tests, guide file-preservation tests (6), native macOS Keychain round-trip, isolated real Codex/Claude registration; [evidence](../test/cli-link.md) |
| Remote OAuth | protected-resource/AS discovery, DCR public clients, PKCE consent, audience/scope binding, refresh rotation/replay revocation, device/account revoke and browser MCP CORS | OAuth durable suite (16), real Chrome OAuth/CORS/frame suite (2), isolated native Codex OAuth login/private rank+usage/logout; [evidence](../test/remote-oauth.md) |

## 이번 감사에서 수정한 공백

- `wam usage` 사람용 출력에 API가 이미 제공하던 최근 30일 rate-limited request 수를 추가하고 회귀 테스트를 넣었다. API/CLI 명세의 denied-count 설명도 구현과 일치시켰다.
- PostgreSQL application 테스트 schema가 같은 process 안에서 timestamp 충돌을 낼 수 있어 random ID를 사용하도록 바꿨다. 충돌을 먼저 재현한 뒤 application PostgreSQL 통합 10개를 다시 통과시켰다.
- `wam-client` cache 테스트도 wall-clock timestamp만으로 임시 SQLite 경로를 만들고 있었다. 병렬 32개 경로 테스트에서 28개만 고유해 실패하는 것을 재현하고 OS random ID로 고쳤다. 캐시 테스트 묶음 20회 반복과 전체 workspace 테스트가 통과했다.
- 폰트 이슈 #38은 `@font-face`, WOFF2 정적 파일, production build 산출물을 확인해 resolved 처리했다.
- profile/web/core/MCP/Gatekeeper/score/analytics 명세와 환경·우선순위 문서에서 이미 구현된 기능을 미구현으로 표시한 내용을 현재 코드와 테스트 결과에 맞췄다.

- CLI account linking, secure storage와 host 등록 도우미를 구현하고 #45 stdin/exit code 및 #46 endpoint/redirect 문제를 수정했다. read:dashboard 기기의 profile 쓰기는 manage:account로 차단한다. host 등록의 상대 binary/config 경로와 Cargo 제거 패키지명도 failing regression 후 수정했다.

- Remote MCP OAuth와 웹 consent/device 관리 경로를 구현했다. Codex의 명시 oauth_resource가 동일 resource를 반복하는 호환성 문제를 failing regression 후 수정했다. private MCP 사용량 미연결(#51), browser preflight 부재 및 quota middleware의 challenge exposure 덮어쓰기(#52)도 재현·수정했다.
- `wam guide print/--write`를 구현했다. 출력은 API·credential 없이 동작하고, 명시 write만 현재 프로젝트의 host 파일 마커를 atomic 갱신한다. CRLF/외부 본문/권한 보존, 멱등성, 잘못된 마커·symlink·oversize·lock 거부를 검증했다.

### 2026-10-28 추가 점검과 구현

- 공용 `wam-client/src/lib.rs`의 1,277줄을 API 계약/endpoint binding, 전송, URL 검증, 오류, 익명 recall 캐시, 진단, 테스트 모듈로 분리했다. public imports와 HTTP 동작은 보존했고 unit·CLI/MCP·실제 Chrome lifecycle 회귀로 확인했다.
- 공용 benchmark 실행기를 추가했다. 실행 전 plan/hash 저장, run별 격리, 가드 callback, 명시적 종료 검증, 실패·무효 보존, 통계/증거 출력을 연결했다. CLI trace replay는 controlled 모드를 거부한다. 실제 Codex/Claude trace adapter와 browser interception은 아직 별도다.
- `ui-drift` v1/v2 fixture를 구현했다. v2는 오래된 저장 selector를 제거하고 새 버튼을 이동한다. 독립 oracle로 실패·복구·503·native input을 검사했고 Chrome 시나리오 6 + 정리 1을 통과했다.
- #53: 미등록 task/block가 계획 검사를 통과하고 무효 제외 후 성공 횟수를 잘못 비교하던 결함을 실패 테스트로 재현·수정했다. 제출 전 실패와 0 기준 지표에서의 통계 crash도 수정했다.
- #55: nested/percent-encoded 발행 경로 및 비 HTTP URL이 가드를 통과하던 결함을 실패 테스트로 재현·수정했다.
- #56: 전체 Python 검사에서 기존 curator 한도 키/지침 불일치 두 건을 재현했다. 현재 scope-batch 한도와 STORAGE_UNAVAILABLE의 stop/retry 지침으로 맞췄다.
- Rust·PostgreSQL·Redis, Python, web/Chrome 검사 명령을 로컬에서 확인했다. GitHub Actions 워크플로우는 2026-10-30 사용자 요청으로 제거했으며 자동 실행을 제공하지 않는다. 사전 등록 템플릿과 테스트 환경 문서의 오래된 구현 상태도 갱신했다.

## 남은 작업 우선순위

### 2026-10-29 host adapter 구현 추가

- `tests/lib/wamtest/bench/host.py`/`host_trace.py`와 `tests/bench/run_host.py`에 Claude Code/Codex/generic MCP 실행·정규화 어댑터를 구현했다. 일회용 설정·작업 디렉터리, 명시 도구 grant, stdin prompt, 출력/시간 한도, 별도 oracle, private raw opt-in과 자체 process group 정리를 연결했다.
- 설치된 Codex 0.165.0과 Claude Code 2.1.312를 loopback synthetic model API·owned MCP로 실행해 성공 경로와 401 분류, MCP 자식 종료를 확인했다. 유료 모델/실계정 호출은 0이다. Native metadata warning을 fatal로 오분류하던 경우와 Claude의 auth retry가 timeout으로 바뀌던 경우를 실패 테스트로 재현하고 수정했다. 실행·증거는 [`tests/bench/README.md`](../../tests/bench/README.md#native-cli-and-generic-mcp-hosts)를 따른다.
- 실제 모델/browser/WAM 캡처, 독립 라벨, live account/OS/release acceptance와 모든 sink 수집은 여전히 별도다. Native adapter는 interception 검증이 없으면 controlled 성능 비교를 거부한다.

### P0 — 배포 전 abuse·availability 검증

1. **Redis rate-limit/usage 경계**: disposable Redis 8.10.3에서 두 store 연결의 동시 quota 경계, 원자적 multi-bucket, denial aggregate, account deletion fence를 확인했다. fail-closed middleware는 injected storage error에서 `503 + Retry-After: 1`을 반환한다. disposable PostgreSQL 16 + Redis 3 검사를 로컬에서 다시 통과했다. GitHub Actions 자동 실행은 제거했으며 Redis outage/failover, 90일 TTL 만료, production trusted-proxy hop과 부하 측정은 남아 있다.
2. **429 user flow**: dashboard의 429/`Retry-After`, 503 구분, 자동 재시도 없음과 401 개인정보 정리·요청 경합을 WEB-30~32 Playwright로 검증했다. 실제 운영 limiter topology의 outage/failover·부하와 대기 후 실호출은 REL-08/10에서 별도 확인한다.

### P1 — 검색 규모와 실제 통합

3. **검색 품질·속도 benchmark**: synthetic 1k/10k/100k corpus와 별도 1M stress에서 p50/p95/p99, throughput, RSS, DB query count, `EXPLAIN (ANALYZE, BUFFERS)`를 기록한다. 동일 query set의 core/API/Postgres/CLI/MCP 결과 순서도 비교한다. 현재 SQL은 URL/task/kind 후보를 줄이고 core ranker parity 테스트를 통과했지만, 대규모에서 일정한 지연을 보인다는 증거는 없다. BM25/RRF 계산 전에 임의 `LIMIT`을 적용하지 말고 held-out quality를 함께 평가한다.
4. **실제 curator/replay 평가**: local fake/provider fixture와 server publication 경로는 테스트됐지만 실제 Meta/Muse 모델 품질 및 운영 browser replay executable은 검증하지 않았다. template-derived 60-case corpus를 독립 라벨 query/job과 held-out set으로 보강하고, 비용·model/tool/policy hash를 남긴다. post-publication withdrawal 재평가 절차도 완성한다.
5. **Host 인증 잔여 (#25)**: CLI link/unlink/doctor/install/uninstall, secure store, stdin, guide print/--write와 remote OAuth discovery/DCR/PKCE/refresh/revoke를 구현·검증했다. 실제 macOS Keychain, 격리된 Codex/Claude 등록 및 합성 계정의 native Codex OAuth login/private rank+usage/logout도 통과했다. 남은 것은 실제 GitHub 승인부터 private CLI/MCP call·회전·해지까지의 운영 여정, Claude/Arise remote OAuth, Windows/Linux native runtime, signed release와 agent behavior acceptance다.
6. **Cache·DB portability**: 여러 CLI process 경합/손상 복구를 stress하고 macOS/Linux/Windows release build와 secure-store 동작을 확인한다. local SQLite cache는 구현됐지만 이 OS별 증거는 없다.

### P2 — UX·운영 마감

7. **브라우저/호스트 QA**: local mock-auth smoke와 dashboard 401/429/503·재조회 경합은 통과했다. 전체 mobile/keyboard/accessibility 확인, profile external link 경로, 실제 Arise 장시간 target-tab 재선택은 남아 있다. Arise 자동화 문제는 이슈 #27이다.
8. **실제 agent demo**: landing page가 scripted replay를 사용한다. Arise와 WAM adapter가 통합되고 사용 동의/비용 기록을 갖춘 실제 agent run으로 바꾸는 작업이 이슈 #39다.
9. **Serializer 비교**: Serde JSON은 API/MCP canonical contract로 유지한다. release payload의 시간·allocation·bytes·RSS benchmark가 없으므로 Postcard/rkyv 같은 local-only 형식은 측정 후 결정한다.

## 테스트 환경과 현재 증거

- 최종 `cargo test --workspace --locked --quiet`: **315 pass / 0 fail / 22 ignored**. 외부 DB/Redis/provider/OS 환경용 ignored tests는 별도 실행이다. macOS Keychain round-trip 1개는 명시적으로 실행해 통과했다.
- disposable local PostgreSQL: application/migration/search 10, diagnostics 2, Gatekeeper queue 3, store 1로 **16 pass, 0 fail**. 테스트 DB는 이번 검증 뒤 제거했다.
- Python 하네스 **82 pass**, Chrome fixture oracle **40/40** + 분리/정리 2, UI drift **6/6** + 정리 1을 통과했다. scripted QA이며 실제 agent 성능 측정은 아니다.
- 전체 strict Clippy, `cargo fmt --all -- --check`, `git diff --check`를 통과했다. OAuth 변경 초기의 임시 인자/필드·포맷 오류는 해결됐다.
- `pnpm --dir web run test`: **17 pass**. `pnpm --dir web run test:smoke`: 격리 mock OAuth, 임시 저장소, web/CLI/MCP 및 게시·교체·계정 lifecycle 1 pass. 이는 live GitHub/host 인증 검증이 아니다.
- `pnpm --dir web run check`: 0 errors/0 warnings. `pnpm --dir web run build`: 성공, CSS가 참조한 4개 WOFF2 파일이 build output에 포함됐다.
- PostgreSQL local socket은 사용 가능했지만 Docker daemon 연결은 안 됐다. Redis 8.10.3은 설치되어 있으나 테스트용 service를 종료했고 `WAM_TEST_REDIS_URL`은 영구 설정하지 않았다. `WAM_TEST_DATABASE_URL`도 영구 설정되어 있지 않다.
- 실제 Codex/Claude 등록은 일회용 HOME/CODEX_HOME/CLAUDE_CONFIG_DIR에서 검증했다. 추가 native Codex OAuth는 임시 file credential store와 합성 provider로 login→private rank/usage→logout을 확인했다. Chrome OAuth 전용 2개는 동의/PKCE/revoke, 실제 cross-origin fetch 및 frame 거부를 검증한다. 검사 명령을 로컬에서 검증했다. GitHub Actions 자동 실행은 제거했으며 production DB/Redis 배포 측정, 실제 GitHub 승인과 live provider/host 여정은 남아 있다.

## 공유 이슈 현황

- **in review**: #34 rate limits, #36 PostgreSQL recall 후보 쿼리, #24 version analytics dashboard, #35 shared client cache. 로컬 동작과 현재 통합 범위는 확인했으며, Redis/live scale/platform evidence는 위 항목에 남아 있다.
- **open**: #10 환경/host adapters, #25 live GitHub·다른 OS·Arise acceptance와 signed release, #39 실제 agent demo.
- **in progress (다른 세션 소유)**: #15 Gatekeeper 잔여 운영/품질 검증, #27 Arise target-tab QA. 소유권을 바꾸지 않았다.
- **resolved 추가 점검**: #53 benchmark 계획·성공률·실패 지표, #55 가드 경로 우회, #56 curator 테스트/지침 계약 불일치.
- **resolved 이번 감사**: #38 폰트 파일 로드, #45 CLI stdin/input, #46 endpoint/redirect 보안, #48 cache temp-path collision, #51 remote MCP usage, #52 browser MCP CORS/challenge exposure. #25는 local 기능 완료 후 잔여 범위를 명시해 open/unassigned로 돌렸다.

End-to-end release criterion: 계정 A가 동의해 기여하고, worker가 replay/consent/lease 검증 뒤 immutable version을 게시하며 +20을 한 번 기록한다. 다른 계정 B가 그 version을 recall해 실제 사용을 선택 보고하면 B가 아닌 author A에게 +2가 한 번 기록되고, dashboard/leaderboard/analytics가 같은 authoritative state를 반영해야 한다. DB restart와 retry 이후에도 결과가 같아야 한다. 해당 흐름은 local smoke와 PostgreSQL integration에서 핵심 경계를 검증했지만 live provider/host 배포 증거는 남아 있다.
