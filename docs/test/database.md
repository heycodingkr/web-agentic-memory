# DB 테스트 스펙: 계정 소유권, 메모리 버전, receipt, 점수 원장

상태: application DB 전체의 개발 인수 기준이다. Gatekeeper memory/queue adapter에 대해서는 ignored disposable PostgreSQL integration tests가 실행 가능하다. 테스트 대상은 일회용 migration schema이며 운영 DB는 사용하지 않는다.

이 문서는 [`../spec/database.md`](../spec/database.md)의 테이블과 무결성 요구사항을 SQL 수준에서 확인한다. API 응답이나 화면만 통과하는 것으로 합격 처리하지 않는다. 각 케이스에서 실제 row, constraint, transaction 결과를 조회하고 `functional-tests.md`의 API/SCORE/GK 테스트와 연결한다.

## 1. 고정 fixture와 관측 방법

테스트 전 migration을 빈 schema에 적용한다. Fixture에는 로그인 사용자 `user_author`, 서로 다른 `user_consumer`, 동의 없는 사용자 `user_no_consent`, 두 개 device, URL pattern `/editor/post/:id`, task `compose-draft`, memory record `memory_a`와 version `v1`을 넣는다. 모든 secret은 합성 sentinel이다. 테스트마다 namespace/schema를 새로 만들고 종료 후 테스트가 만든 schema만 삭제한다.

관측 oracle은 SQL row count와 FK/unique 위반, transaction commit/rollback 상태, score ledger 합계, audit digest다. 서버가 `200`이나 화면의 “성공”을 반환하더라도 저장 상태가 요구사항과 다르면 실패다. 동시성 케이스는 barrier 또는 PostgreSQL lock-wait 상태처럼 관찰 가능한 동기화를 사용하며, 고정 sleep으로 순서를 추측하지 않는다.

## 2. 케이스

| ID | 분류 | fixture / 동작 | 합격 판정과 DB oracle |
|---|---|---|---|
| DB-01 | schema | 모든 migration 적용, enum 밖 contribution state와 task 입력 | 유효한 schema만 생성되고 enum 밖 값은 constraint 또는 API 경계에서 거부된다. 실패한 transaction에는 일부 row가 남지 않는다. |
| DB-02 | identity | contribution에 존재하지 않는 user, device에 존재하지 않는 user를 지정해 insert | FK가 insert를 거부한다. 정상 contribution의 contributor는 OAuth 세션에서 정한 기존 user row를 가리킨다. |
| DB-03 | URL privacy | canonical pattern에 `?token=sentinel`, `#fragment`, email, 원본 계정 ID를 섞어 저장 시도 | DB에 원문이 보존되지 않는다. API 정규화가 불가능하면 row가 생성되지 않으며 저장 허용 시에는 안전한 `:id`/`:account` 패턴만 남는다. |
| DB-04 | idempotency | 같은 contributor + nonce로 contribution 두 번 접수, 다른 contributor가 같은 nonce 사용 | 첫 요청당 contribution/job은 하나다. 동일 사용자의 재시도는 같은 결과를 돌려준다. user scope가 다른 nonce는 정책상 서로 충돌하지 않는다. |
| DB-05 | scope uniqueness | 같은 canonical URL + task + intent를 두 worker가 동시에 신규 게시; 이후 다른 task로 게시 | 하나의 active record/version만 존재한다. 서로 다른 task는 별도 scope를 만들고, active uniqueness 제약을 위반하지 않는다. |
| DB-06 | version immutability | v1이 게시된 뒤 supersede v2, 이어서 v1 content/fingerprint를 UPDATE 시도 | v1은 변경 불가다. 새 version row가 추가되고 record의 active pointer만 v2로 원자 이동한다. 과거 digest가 그대로다. |
| DB-07 | receipt ownership | 로그인 recall receipt를 consumer A와 version v1에 연결; consumer B가 outcome 처리 시도 | receipt는 서버가 정한 계정/version에 고정된다. B의 처리 요청은 거부되며 outcome, score event 변화가 없다. |
| DB-08 | anonymous no-score | user identity와 연결되지 않은 익명 usage 집계를 생성하고 점수 event 발행 시도 | 익명 usage 집계는 운영 통계로만 저장할 수 있다. user-level receipt와 score_events는 생기지 않으며 beneficiary 점수와 leaderboard summary는 변하지 않는다. |
| DB-09 | self-use | author와 consumer가 같은 user인 유효 receipt로 success outcome 생성 | DB transaction invariant 또는 점수 발행 로직이 자기 사용 event를 거부한다. 다른 device를 통해도 같은 user_id면 동일 결과다. |
| DB-10 | queue concurrency/fencing | 여러 worker가 같은 job을 claim하고 worker A의 lease가 만료된 뒤 B가 새 lease generation으로 처리. A가 뒤늦게 commit 시도 | claim은 한 번에 하나다. 현재 generation을 가진 B만 commit할 수 있다. A의 stale commit은 거부되고 decision/version/score 중복은 없다. |
| DB-11 | worker retry | 외부 replay 결과를 저장하기 직전에 worker crash를 주입하고 job을 재처리 | job은 retryable 상태로 복구된다. 같은 idempotency key + evidence digest로 decision, publish, score가 두 번 생성되지 않는다. |
| DB-12 | score ledger | receipt success를 두 번 처리하고, 이후 해당 지급을 취소하는 reversal 발행 | 최초 event 한 건, 중복 요청의 새 event 0건, 연결된 반대 delta reversal 한 건이다. 원 event를 수정하거나 지우지 않는다. |
| DB-13 | rollback | v2를 active로 만든 뒤 관리자 kill switch로 v1 복구 | v1을 재사용하거나 새 immutable rollback revision으로 pointer를 복구한다. v2와 기존 감사 기록은 보존되고 관련 score reversal이 별도 event로 남는다. |
| DB-14 | retention | 대기/거절 contribution의 `purge_at` 이전과 이후를 실행 | 기한 전엔 정책상 최소 sanitized payload만 존재한다. 기한 후 원문/sanitized trace payload는 삭제되고, 비식별 상태 및 score/audit 근거만 보존된다. |
| DB-15 | secret sinks | URL query sentinel, OAuth token, device secret, 원문 trace를 테스트 요청에 포함하고 모든 테이블·audit·log를 검색 | 원문 secret, query, prompt, page content는 DB와 log에 0건이다. device token은 hash만 저장한다. |
| DB-16 | reconciliation | 기여 승인, success award, reversal을 포함한 원장을 삽입하고 dashboard/leaderboard 합계 재생성 | user별 표시 합계는 `SUM(points_delta)`와 같다. opt-out user는 public leaderboard 결과에서 제외되고 event 원장은 그대로 남는다. |
| DB-17 | version author attribution | A가 v1을 만들고 B가 개선 contribution으로 v2를 만든 뒤, v1/v2에 각각 consumer receipt 발행 | v1 receipt는 A, v2 receipt는 B를 beneficiary로 스냅샷한다. duplicate support만 제출한 C는 version author나 새 memory 승인 점수가 되지 않는다. request body author ID 변경도 귀속을 바꾸지 않는다. |
| DB-18 | consent race | worker가 contribution을 leased/분석 중일 때 사용자가 consent withdrawal을 commit하고, 이어 worker가 publish transaction을 시작 | withdrawal이 먼저 commit되면 worker publish/author award는 거부되고 job은 canceled 또는 withdrawal 상태로 종료된다. 이미 publish가 먼저 끝났으면 별도 withdrawal job이 생성된다. |
| DB-19 | memory usage analytics aggregate | authored version에 대한 authenticated recall/detail, distinct non-author account, outcome, anonymous count fixture를 생성해 daily/weekly aggregate와 dashboard scope를 조회 | version/bucket별 수치가 fixture와 일치한다. 같은 계정이 여러 bucket에서 recall해도 선택 기간 고유 계정 합계는 중복 계산하지 않는다. author self-use와 다른 version의 사용은 섞이지 않는다. aggregate table에는 reader ID, device ID, email, receipt ID, raw URL이 없고 타 계정은 해당 version aggregate를 조회할 수 없다. |
| DB-20 | memory_index projection | L0에 1~4 route와 2~8 keyword를 가진 version을 게시하고 동일 version 재색인, 중복 tuple 삽입, supersede/rollback을 수행 | row 집합이 L0 projection과 정확히 같다. tuple unique 제약이 중복을 거부하고 재시도는 멱등이다. 전환 transaction 뒤 이전 inactive projection은 기본 recall에서 반환되지 않는다. |
| DB-21 | 규모별 query plan | 고정 seed로 1k/10k/100k memory version(평균 약 12.5k/125k/1.25M index rows)을 만들고 exact/prefix/keyword/no-match를 `EXPLAIN (ANALYZE, BUFFERS)`로 측정 | host/route/task scope와 keyword 검색의 계획·실제 rows·p50/p95를 보고한다. scope가 다른 후보 0건, 별도 근거 없는 full scan은 성능 회귀로 기록한다. |
| DB-22 | Gatekeeper queue restart/fencing | 별도 프로세스 adapter 두 개로 같은 contributor nonce 제출, 동일 scope lease, 만료 전후 재시작 claim을 수행 | nonce는 한 queue ID를 반환한다. held scope의 중복 claim은 없고, 만료 job은 한 attempt를 소비한 뒤 새 generation으로 재lease된다. 실행 위치: `cargo test -p wam-gatekeeper --test postgres_queue -- --ignored`. |
| DB-23 | queue fence와 memory decision 원자성 | queue 활성화 후 fence 누락, 만료/교체된 generation, current owner의 decision commit을 각각 시도 | 앞의 두 commit은 memory/support/version와 decision journal을 바꾸지 않고 거부된다. current fence는 memory mutation과 decision journal을 같은 transaction에서 commit한다. 실행 위치: `tests/postgres_queue.rs`의 DB-22 integration test. |
| DB-24 | withdrawal, review resolution, retention, queue metrics | contribution/contributor 철회, leased job review resolve, terminal payload/nonce purge, queue stats를 실행 | 대기 작업은 취소되고 payload reference가 비워진다. leased attribution이 제거되고 review resolve는 lease generation을 올린다. purge 뒤 nonce replay는 새 ID가 되며 counts/oldest age는 남은 row와 일치한다. 실행 위치: `cargo test -p wam-gatekeeper --test postgres_queue -- --ignored`. |
| DB-25 | contribution 철회와 publish decision row-lock 경합 | withdrawal transaction이 leased queue row를 잠그고 `attribution_withdrawn`을 갱신한 채 유지한다. decision transaction이 같은 row를 잠그려 대기하는지 관찰한 뒤 withdrawal을 commit한다 | decision은 갱신된 철회 상태를 확인해 `Conflict`로 끝나며 memory/support/version과 decision journal 변경은 0건이다. 철회가 먼저인 순서를 DB lock wait로 확인한다. 실행 위치: DB-22와 같은 `postgres_queue.rs` 테스트. |
| DB-26 | account usage aggregate upsert and retention | same Redis minute-bucket snapshot flushes twice, then an advanced absolute counter flushes; expire/delete account fixture | replay leaves aggregate unchanged; higher counter replaces lower snapshot without double counting; owner/time/operation indexes serve bounded queries; account delete and 90-day purge remove linked aggregates |
| DB-27 | PostgreSQL recall query pushdown and index use | seed 100k versions/1.25M `memory_index` rows with skewed hosts; call the production repository candidate-query port for exact/prefix/keyword/no-match | candidate set equals exhaustive local-repository results; SQL applies host/route/task/kind/keyword and active-version predicates; `EXPLAIN` uses expected index paths and does not deserialize the full catalog; rows examined, p50/p95/p99 recorded |

## 3. 트랜잭션 불변조건

기여 제출은 account ownership, consent snapshot, nonce, contribution row, gatekeeper job을 같은 트랜잭션으로 기록한다. 어느 하나가 실패하면 전부 rollback한다. gatekeeper decision과 memory version 교체는 `decision → immutable version → active pointer → contribution state`가 부분 적용되지 않아야 한다. 외부 LLM이나 replay 실행 중 DB transaction을 열린 채로 유지하지 않는다. 외부 작업 완료 후 job의 idempotency key와 evidence digest를 확인해 상태 변경을 commit한다.

점수 event는 receipt 성공 및 gatekeeper 정책을 근거로 서버에서만 생성한다. `consumer_user_id IS NULL`, author와 consumer 동일, receipt 만료/재사용, contribution 중복 승인인 경우에는 award event를 만들지 않는다. reversal은 기존 지급과 동일한 source event 및 memory version을 참조하고 별도 음수 delta로 기록한다. dashboard의 캐시 테이블이 있더라도 원장 재생 값과 달라지면 캐시를 다시 만든다.

## 4. 인덱스와 조회 계약

- memory 검색은 `memory_index`의 유일 tuple `(version_id, site_host, url_pattern, kind, task_category, keyword)`와 site/route/task/kind 및 site/keyword 조회 인덱스를 이용한다. active version status는 join으로 검사하고 inactive version은 기본 recall에서 제외한다.
- contribution dashboard는 `(contributor_user_id, received_at)`를 이용하며 현재 세션 user 범위로만 조회한다.
- receipt 검사와 점수 멱등성은 opaque receipt id 및 idempotency key에 unique index를 둔다.
- leaderboard 조회는 opt-in 사용자만 선택하고 공개 handle과 기간 점수만 반환한다. 내부 user id, 이메일, device id는 SELECT 결과에 포함되지 않는다.
- retention worker가 purge할 row는 status와 `purge_at`으로 찾을 수 있어야 하며 score/audit event를 cascade delete하지 않는다.

성능 수치와 전용 runner 조건은 [`metrics.md`](metrics.md), 재현 가능한 fixture와 반복 방법은 [`retrieval-benchmark.md`](retrieval-benchmark.md) 및 [`service-abuse-performance.md`](service-abuse-performance.md)에서 정한다. 보안·정합성 제약을 제거해 성능을 올리는 최적화는 허용하지 않는다.

## 5. 관련 케이스

API-level 연결은 `API-30~46`, `SCORE-15~24`, `GK-37~48`, `FLOW-04~06`, `USAGE-01~05`이다. Commit 전/후 연결 단절, worker lease, 철회와 publish row-lock race, 원장 commit, purge 재시도, usage aggregate flush는 [`failure-matrix.md`](failure-matrix.md)의 `FAIL-08~09`, `FAIL-15~16`, `FAIL-29`, `FAIL-31`, `FAIL-35`로 fault injection한다. 익명 사용이 score event로 이어지지 않는 것은 [`security-scenarios.md`](security-scenarios.md)의 인증 우회 및 리더보드 어뷰징 케이스에서도 별도로 확인한다. DB 스키마 합격은 MCP 호스트 설치나 웹 UX 합격을 대신하지 않는다.
