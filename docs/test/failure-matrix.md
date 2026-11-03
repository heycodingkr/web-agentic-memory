# 실패·오류 케이스 통합 테스트 명세

상태: FAIL-01~40 교차 계층 인수 기준. FAIL-39/40은 로컬 SIGTERM·PostgreSQL 종료 회귀로 자동 검증된다. 각 행의 실행 증거와 환경 한계를 따르며, 로컬 통과를 운영·호스트 수용 완료로 간주하지 않는다.

## 1. 공통 실패 처리 원칙

1. **부분 저장 금지:** 트랜잭션 경계를 넘기 전 실패하면 contribution, queue job, memory version, score event를 일부만 만들지 않는다.
2. **멱등 재시도:** 응답 유실이나 네트워크 timeout 뒤 다시 제출할 때 같은 idempotency key를 사용한다. contribution/job/decision/receipt/outcome/score가 중복 생성되지 않는다.
3. **Fail closed:** 인증·동의·URL 범위·보안 검사·모델 출력·replay 결과를 확인할 수 없으면 publish나 점수 지급을 하지 않는다.
4. **읽기만 제한적으로 저하:** 서버/DB가 불가하면 [`client-cache.md`](../spec/client-cache.md)에 따라 cache 가능한 익명 결과만 최대 15분 stale fallback하고, 그 외에는 `offline_no_cache`를 반환한다. Authenticated recall, contribution, outcome, score는 offline fallback으로 성공 처리하지 않는다.
5. **정확한 실패 표시:** 빈 검색 결과와 서버 오류, 미인증과 권한 부족, 재시도 대기와 최종 실패를 서로 다른 상태와 reason code로 표시한다.
6. **오류 응답 최소화:** 모든 실패에서도 URL 원문, query, prompt, page content, OAuth/device token, private trace를 응답·로그·HTML에 echo하지 않는다. 진단에는 correlation ID와 공개 가능한 reason code만 남긴다.
7. **자동 반복 제한:** 위험한 browser click/save, contribution write, receipt outcome을 무제한 재시도하지 않는다. 상태를 확인하고 안전한 경우 같은 idempotency key로 제한적으로 재시도한다.

## 진단 로그와 로컬 배포 전 스모크 (구현됨)

| ID | 실패 주입 | 기대 결과·불변조건 | 실행 증거 |
|---|---|---|---|
| OBS-01 | HTTP 401/403/4xx와 malformed trace | 응답 reference와 DB trace 일치, raw header/body/query 미저장 | server diagnostics + Chrome smoke |
| OBS-02 | CLI HTTP 오류 / MCP malformed JSON | stderr 또는 framed JSON-RPC에 reference, source별 DB 행 일치 | actual wam/wam-mcp smoke |
| OBS-03 | 웹 unhandled promise rejection | closed error category만 DB 적재 | actual Chrome smoke |
| OBS-04 | 진단 DTO extra field/enum/ID/시간 상한 위반 | generic 거부, 입력 원문 echo와 client_error row 0 | diagnostics.rs |
| OBS-05 | 같은 event_id 재전송·DB 재시작 | 오류 event 하나, trace 보존 | SQLite + PostgreSQL diagnostics tests |
| OBS-06 | 로그 저장소 table/connection 장애 | business 응답 불변, bounded 재시도와 복구 후 저장 | SQLite recovery + PostgreSQL terminate test |
| OBS-07 | CLI/MCP offline 후 다음 성공 요청 | private spool 재생, 중복 방지, TTL/상한/opt-out 준수 | wam-client telemetry_spool tests |
| OBS-08 | 실제 웹에서 개선 동작 순서 변경 | 동일 memory ID, 새 immutable version, 최신 recall, 실제 UI 반영 | local-smoke.spec.ts |

실행법과 fixture 격리는 [local-smoke.md](local-smoke.md), 저장 스키마/보관기한은 [diagnostics.md](../spec/diagnostics.md)를 따른다.

## 2. 계층별 fault-injection matrix

| ID | 계층/실패 주입 | 기대 응답·사용자 경험 | 상태 불변조건·복구 | 연결 테스트 |
|---|---|---|---|---|
| FAIL-01 | API에 malformed JSON, schema version 오류, 필수 필드 누락 | API 계약의 400과 stable error code | contribution/job row 0, 원문 body echo 0 | CORE-01~04, API-25, SEC-A06 |
| FAIL-02 | URL을 정규화할 수 없거나 내부 host/IP/userinfo 포함 | recall은 빈 결과+`scope_ambiguous` 또는 `invalid_url`; write는 명시적 400 | 원 URL을 서버/로그에 저장하지 않고 scope 밖 조회·게시 0 | CORE-22~26, RCL-11, API-07, SEC-A11 |
| FAIL-03 | payload byte/depth/record limit 초과 | 413 또는 문서화된 입력 오류와 다음 조치 | body 일부 저장 0; 전체 읽기 전에 크기 제한 적용 | CORE-04, API-24~25, SEC-E01 |
| FAIL-04 | credential 없음, 만료, 위조 또는 해지된 토큰으로 write | credential 없음은 정책상 익명 read만 가능; 제시된 invalid credential은 401이며 익명으로 조용히 downgrade하지 않음 | propose/outcome/device-write 0, receipt/score 0 | LINK-07~10, API-21/38, MCP-22, SEC-D01 |
| FAIL-05 | 본인이 아닌 contribution, dashboard, receipt 또는 관리 endpoint 접근 | 권한 정책에 따라 403 또는 IDOR를 숨기는 404 | 타인 데이터 응답·상태 변경 0; 존재 여부도 유추하기 어려움 | API-28/33, WEB-19, SEC-D02~04 |
| FAIL-06 | `expected_version`이 stale이거나 동시 교체 발생 | 409 `VERSION_CONFLICT`; 클라이언트가 현재 version을 다시 읽도록 안내 | active pointer·version·score는 그대로; 강제 last-write-wins 금지 | CUR-07, DEDUP-17, DB-05/06 |
| FAIL-07 | queue 한도 또는 per-user quota 초과 | 429 또는 503 + `Retry-After` | 새 제출은 접수하지 않고 이전에 202 받은 job은 보존 | GK-02, QUE-04/05, API-24 |
| FAIL-08 | contribution POST가 서버에 도달했으나 DB commit 전 connection/process 중단 | 5xx/연결 오류 | contribution과 job이 둘 다 없거나 둘 다 commit; 부분 상태 0 | QUE-01, DB-02/10 |
| FAIL-09 | DB commit 뒤 202 응답이 유실돼 client가 재전송 | 같은 idempotency key로 기존 contribution ID/상태 반환 | contribution/job은 각각 하나; score/publish 중복 0 | API-23, QUE-01, DB-04/11 |
| FAIL-10 | DB/API unavailable 중 recall | explicitly cacheable anonymous result 안에서만 last valid fresh/stale copy 사용; 그 외에는 `offline_no_cache` | authenticated recall/receipt, outcome, score는 cache fallback 0; cache는 실패 응답으로 덮어쓰지 않음 | API-09, RCL-12, CACHE-07/09/10, CLI-17/24, SEC-E13 |
| FAIL-11 | ETag 304, 잘못된 ETag, 손상되거나 오래된 cache | 304는 기존 유효 cache 사용; 손상 응답은 폐기하고 안전한 fallback | partial/malformed catalog를 정상으로 저장하지 않음 | API-02/09, RCL-12, MEM-10, SEC-H04~05 |
| FAIL-12 | Meta provider DNS/TLS/auth error, 429, quota 소진, 5xx, timeout | 큐는 `retry` 또는 `waiting_budget`; 사용자에게 처리 지연 상태 표시 | public catalog와 score ledger 불변; recall은 계속 가능; 최대 retry 뒤 dead-letter 및 alert | MODEL-05/06/15~19, QUE-06/08/09/25~39, SEC-I03 |
| FAIL-13 | Meta 응답 invalid JSON, 잘못된 enum, 과도한 길이, 누락된 evidence | structured error 처리 또는 `needs_human_review`; 성공 응답으로 바꾸지 않음 | active memory/version/points 변화 0; 원문 model output을 public response에 노출하지 않음 | MODEL-03/05/07, CUR-11, SEC-I06~07 |
| FAIL-14 | 모델이 duplicate scope create, stale version replace, scope 밖 id 또는 결정 도구 2회 호출 | `DUPLICATE_EXISTS`, `VERSION_CONFLICT`, `SCOPE_VIOLATION`, `ALREADY_DECIDED` 등 계약된 오류 | 도구 오류 뒤에도 invariant 유지; 모델이 한도 안에서 고치지 못하면 escalate/retry/dead 처리 | CUR-06~10/12/16, SEC-I01/04/05 |
| FAIL-15 | worker가 decision transaction 전, 도중, commit 뒤 ack 전에 강제 종료 | job lease가 만료된 뒤 재처리 | 상태 변화는 전부 commit 또는 rollback; 중복 version/author/score event 0 | GK-37~38, QUE-07, DB-10~12/17 |
| FAIL-16 | lease가 만료된 이전 worker가 늦게 돌아와 새 worker 뒤 commit 시도 | fencing token/lease generation 불일치로 이전 worker commit 거부 | 가장 최신 유효 lease만 결정 반영; 단일 active version | QUE-02/07, DB-10~11, CUR-07 |
| FAIL-17 | job queue 폭주, 한 account가 큐 점유, P0 reverify/rollback 대기 | backpressure와 라운드로빈 안내; P0 우선 | 이미 접수된 다른 account job 유실·기아 0; budget 소진이 recall을 막지 않음 | QUE-04/05/08/11/12, SEC-I03 |
| FAIL-18 | 작업 중 contribution consent 철회 | pending job은 canceled; leased job은 분석을 끝낼 수 있어도 commit 직전 최신 동의 상태를 재확인 | 동의 철회가 publish보다 먼저 commit되면 새 public version/author score를 만들지 않고 payload purge; publish가 먼저면 별도 withdrawal job 수행 | PRIV-03/04, QUE-10, DB-18, SEC-G02 |
| FAIL-19 | replay harness unavailable, fixture crash, timeout, oracle 불일치 | replay 결과 `inconclusive/failed`; 사용자 dashboard에 검증 보류 사유 | 모델 주장만으로 publish 금지; timeout을 success로 간주하지 않음; 제한 재시도 후 예외 큐 | GK-20~26/43~44, PERF-06, FLOW-04~05 |
| FAIL-20 | 사이트 동작 중 browser MCP disconnect, tab close, navigation mismatch, save error | 에이전트는 중단/복구 가능한 단계와 실패 원인을 알림 | oracle이 성공을 입증하지 않으면 `success` outcome·저자 점수 0; 반복 click/save 자동 금지 | BEH-03/05, SMK-03/05, FLOW-02, SCORE-06/17 |
| FAIL-21 | login provider가 deny, timeout, callback state/redirect mismatch | 로그인 오류와 다시 시도 방법 표시 | web session/device token 0; 이전 계정으로 묵시 연결하거나 동의 없이 제출하지 않음 | WEB-02/04/16, LINK-03~06, SEC-D08~10 |
| FAIL-22 | session이 contribution 전송 중 만료 또는 사용자가 두 번 submit | 재로그인 필요/접수 상태 확인을 표시; 같은 key로만 재확인 | 자동으로 새 contribution을 만들지 않음; 사용자 계정 소유권 유지 | API-23/34/35, WEB-16/17, DB-04 |
| FAIL-23 | dashboard/leaderboard API timeout 또는 5xx | 오류, stale snapshot, empty state를 구분하고 재시도 제공 | 실패를 0점·기여 삭제로 오인하지 않음; cached data면 마지막 갱신 시각 표시 | WEB-05/07/13/18/23/24, API-33, DB-16 |
| FAIL-24 | MCP initialize/tools/list 실패, 잘못된 capability, 예상 못한 tool result schema | host status에 initialize/list/call 단계를 구분해 표시 | 도구가 준비되지 않았으면 설치 완료로 표시하지 않고 write 호출 0; 설정 원본 보존 | MCP-01/05/10/13/15/19, HOST-01~04, INST-43/44/46/47, MCPF-21/23/24 |
| FAIL-25 | stdio MCP child process 즉시 종료, EOF, stderr 폭주, startup timeout | 호스트에서 재현 가능한 진단 코드와 로그 경로 안내 | 반복 재시도·write를 하지 않고 기존 host 설정/다른 MCP server 보존 | SMK-01/02/10/14, INST-43/46/47, MCPF-19/23, CLI-25/26 |
| FAIL-26 | remote MCP TLS/DNS/proxy/auth/HTTP 오류 | 구분된 진단; TLS 오류에서 평문 HTTP로 downgrade 금지 | eligible anonymous recall만 bounded cache fallback; authenticated recall/write requires server confirmation; no offline score | MCP-12/13/22/23, INST-29~33/44/45, MCPF-15~17/21, CACHE-07/09, SEC-F04/10 |
| FAIL-27 | 복사한 설정 syntax 오류, endpoint 오타, checksum 불일치, 설치 권한 없음 | `/guide`와 CLI가 실제 실패 원인 및 정확한 복구 안내 | 설치 완료 오표시 0; 설치 변경은 원자적 rollback, 다른 MCP 설정은 보존 | MCP-16~19, INST-06~12/20~25/43~45, HOST-06 |
| FAIL-28 | receipt expired/foreign/replayed, outcome enum 오류 | 안정된 auth/state error; 결과가 잘못됐다는 정보만 제공 | outcome과 score는 한 번도 추가되지 않음; user identity 추정 정보 미노출 | API-25~29, SCORE-16/20, DB-07~09, SEC-C04 |
| FAIL-29 | success outcome과 점수 원장 저장 사이 DB 장애 | 응답 불명확 시 동일 idempotency key로 다시 확인 가능 | outcome 처리와 score event는 원자적 또는 outbox 처리; 0회나 1회만 반영하고 중간 상태 노출 금지 | SCORE-11/14/20/23, DB-12/16/17 |
| FAIL-30 | publish 성공 뒤 cache invalidation/rollback/kill-switch 전파 실패 | record는 안전 우선으로 recall에서 숨기거나 stale로 표시하고 alert | 이전 version과 audit 보존; 복구 작업 재시도 멱등; 위험 version을 정상인 것처럼 제공하지 않음 | GK-34~38/47, DEDUP-17, MEM-10, SEC-F07/08 |
| FAIL-31 | privacy purge worker 실패, 재시도, retention 대상 일부만 삭제 | 운영 alert와 purge retry 상태 기록 | 삭제는 멱등; payload 잔존을 성공 처리하지 않음; score/audit 원장까지 cascade 삭제 금지 | PRIV-05/09, QUE-14, DB-14/15, SEC-G03 |
| FAIL-32 | 어느 계층에서든 예외/4xx/5xx/stack trace 발생 | 사용자 메시지는 correlation ID와 공개 reason code만 포함 | 센티널, 원 URL/query, token, private prompt/trace, model reasoning이 API·MCP·CLI·web·DB log 모두에서 0건 | SEC-A06~09/14/17, MODEL-07, DB-15 |
| FAIL-33 | shared Redis limiter unavailable, timeout, or failover during load | bounded retry then fixed retryable 503 for uncached/costly/write routes; never switch silently to per-process counters | no quota bypass or partial write; request body/IP/key is not exposed | RL-09, SEC-J05 |
| FAIL-34 | local SQLite cache lock, corruption, disk-full, origin/schema change, offline stale boundary | cache error is isolated; discard only derived cache; show eligible public stale result with age or explicit `offline_no_cache` | no credential/private entry loss, no stale auth result, no stale cache beyond limit | CACHE-04~10, SEC-H04~05, SEC-J04/06 |
| FAIL-35 | usage aggregate flush repeats, lags, or PostgreSQL is unavailable | dashboard marks latest successful timestamp and keeps previous aggregate snapshot | no double count, no false zero, no raw event/IP/URL persistence; recovery catches up within target | USAGE-01~06, DB-26, WEB-27~28 |
| FAIL-36 | hot-host search saturates DB pool or a query exceeds its statement timeout | bounded queue/timeout error and operator query fingerprint | no unbounded candidate scan, response does not expand scope, active version stays correct | DB-21, PERF-21, SEC-J03 |
| FAIL-37 | JSON/cache serializer sees malformed, deeply nested, oversized, or incompatible-version payload | reject within size/time bounds and preserve last valid entry | no panic, no partial decode/write, no data migration of authoritative DB rows | CACHE-04/06, PERF-22, SEC-H05 |
| FAIL-38 | transient DB conflict/timeout on read or idempotent/non-idempotent write | retry only classified reads or complete transaction restart; ambiguous writes checked by idempotency key | bounded attempts, at most one queue/publish/score change; terminal errors remain stable | PERF-25, FAIL-08/09/29 |
| FAIL-39 | API process receives SIGTERM while an idle MCP SSE is open | admission stops, existing SSE closes through the shutdown signal, diagnostics flush path completes, process exits successfully | accepted durable state is preserved; no indefinitely open stream or abrupt signal exit | actual `wam-server` process test in `tests/shutdown.rs`; production deployment load remains separate |
| FAIL-40 | actual API write waits on an owned PostgreSQL advisory lock when SIGTERM arrives | process stays alive until the admitted write drains; unlock yields HTTP 201 and successful exit | exactly one contribution and queue entry, matching 201 diagnostic flushed before exit; restart and same-nonce retry preserve the ID | `tests/env/verify_release_services.py`, local native run; staging/TLS/load remain separate |

2026-10-30 local native recovery evidence: `target/e2e-logs/release-services/20261030-025758-64bd/` passed 7/7, including FAIL-33/40, owned cleanup, PostgreSQL dump/restore and restart. Redis WAIT follows its replication barrier on the same connection; the promoted replica retains the previous quota, permits only the last available unit, then denies the next. This is manual promotion plus explicit API reconnect, not automatic cluster failover. The source is dirty and the provider/replay is synthetic.

## 3. 공통 HTTP 및 클라이언트 오류 표

| 실패 종류 | 외부 계약 | 서버 상태 판정 |
|---|---|---|
| malformed input/URL/schema | 400 | job/receipt/score 변경 없음 |
| credential 없음·invalid | 401 | invalid credential을 anonymous로 downgrade하지 않음 |
| 인증됐지만 권한/동의 없음 | 403 또는 privacy상 숨길 리소스는 404 | write와 score 변경 없음 |
| 없는 resource | 404 | 내부 식별자·타인 resource 존재 여부 미노출 |
| `expected_version`/state 충돌 | 409 | 다시 읽기 전까지 자동 overwrite 금지 |
| 크기 상한 초과 | 413 | body/queue에 부분 row 없음 |
| rate/quota/backpressure | 429 + `Retry-After` (필요 시 503) | 이미 accepted인 job 유지 |
| 일시적인 서버/DB/provider 장애 | 5xx 또는 provider-specific safe error | 가능한 읽기만 cache로 제한 저하, write는 미확정 성공으로 취급 금지 |
| CLI 호출 실패 | [`spec/cli.md`](../spec/cli.md) 종료 코드 1~6 | stdout JSON 계약과 secret redaction 유지 |
| MCP transport/tool 실패 | host status/tool error로 단계 구분 | 마지막 유효 read cache만 사용, write 결과는 서버 확인 전 pending |

숫자 status와 종료 코드는 이미 합의된 API/CLI 계약을 따른다. 이 표에서 새로 정하지 않은 경우 구현 PR에서 OpenAPI, CLI help, MCP error code를 같은 contract source에서 생성하고 테스트해야 한다.

## 4. 오류 테스트 실행 방법과 증거

각 fault는 disposable loopback 환경에서 한 번에 하나씩 주입하고, 해당되는 경우 같은 결함을 복구한 뒤 재시도한다. 입력 조건은 순서대로 유지한다: 정상 baseline → fault 주입 → 상태/응답 수집 → fault 제거 → 동일 idempotency key 재시도 → 최종 상태 비교. fault 테스트가 운영 Meta API/서비스를 고의로 다운시키거나 실제 고객 데이터를 쓰지 않는다. DB connection terminate, fake OAuth/provider, fake clock, programmable HTTP proxy, fake browser MCP, fixture oracle를 사용한다.

case evidence에는 FAIL ID, fault type, build/schema/model/policy versions, request correlation ID, API/MCP/CLI/web 관측, DB row counts, active version pointer, queue state, score ledger delta, cache timestamp를 저장한다. 사용자 입력이나 secret 원문은 evidence로 복사하지 않고 digest/sentinel hit count만 남긴다. 재시도 가능한 오류와 terminal 오류를 분리하고, retry exhaustion 뒤 `dead` 또는 예외 큐로 이동한 작업은 사람이 조회·복구할 수 있어야 한다.

## 5. 합격 게이트

- FAIL-01~40에 지정된 상태 불변조건 위반이 한 건이라도 나오면 실패다.
- 202/성공 응답 이후 저장이 유실되는 경우 0건, 재시도에 따른 중복 score/publish도 0건이어야 한다.
- 캐시 fallback은 recall에만 허용하며 contribution, outcome, score를 온라인 확정처럼 만들면 실패다.
- provider·DB·browser·host 장애가 발생해도 false success, score award, publish가 생기면 실패다.
- 관련 SEC 센티널 검색에서 민감값 한 건이라도 발견되면 다른 결과와 무관하게 실패다.
- 기존 상세 테스트 ID는 그대로 유지한다. FAIL ID는 여러 컴포넌트를 잇는 장애 주입과 복구 계약을 추적한다.
