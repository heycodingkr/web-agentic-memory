# DB 테스트 스펙: 계정 소유권, 메모리 버전, receipt, 점수 원장

상태: 개발 전 인수 기준. 테스트 대상은 disposable PostgreSQL과 일회용 migration schema이며 운영 DB는 사용하지 않는다.

이 문서는 [`../spec/database.md`](../spec/database.md)의 테이블과 무결성 요구사항을 SQL 수준에서 확인한다. API 응답이나 화면만 통과하는 것으로 합격 처리하지 않는다. 각 케이스에서 실제 row, constraint, transaction 결과를 조회하고 `functional-tests.md`의 API/SCORE/GK 테스트와 연결한다.

## 1. 고정 fixture와 관측 방법

테스트 전 migration을 빈 schema에 적용한다. Fixture에는 로그인 사용자 `user_author`, 서로 다른 `user_consumer`, 동의 없는 사용자 `user_no_consent`, 두 개 device, URL pattern `/editor/post/:id`, task `compose-draft`, memory record `memory_a`와 version `v1`을 넣는다. 모든 secret은 합성 sentinel이다. 테스트마다 namespace/schema를 새로 만들고 종료 후 테스트가 만든 schema만 삭제한다.

관측 oracle은 SQL row count와 FK/unique 위반, transaction commit/rollback 상태, score ledger 합계, audit digest다. 서버가 `200`이나 화면의 “성공”을 반환하더라도 저장 상태가 요구사항과 다르면 실패다. 동시성 케이스는 barrier로 두 요청이 같은 시점에 실행되도록 하며 sleep 기반 타이밍 추측을 쓰지 않는다.

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

## 3. 트랜잭션 불변조건

기여 제출은 account ownership, consent snapshot, nonce, contribution row, gatekeeper job을 같은 트랜잭션으로 기록한다. 어느 하나가 실패하면 전부 rollback한다. gatekeeper decision과 memory version 교체는 `decision → immutable version → active pointer → contribution state`가 부분 적용되지 않아야 한다. 외부 LLM이나 replay 실행 중 DB transaction을 열린 채로 유지하지 않는다. 외부 작업 완료 후 job의 idempotency key와 evidence digest를 확인해 상태 변경을 commit한다.

점수 event는 receipt 성공 및 gatekeeper 정책을 근거로 서버에서만 생성한다. `consumer_user_id IS NULL`, author와 consumer 동일, receipt 만료/재사용, contribution 중복 승인인 경우에는 award event를 만들지 않는다. reversal은 기존 지급과 동일한 source event 및 memory version을 참조하고 별도 음수 delta로 기록한다. dashboard의 캐시 테이블이 있더라도 원장 재생 값과 달라지면 캐시를 다시 만든다.

## 4. 인덱스와 조회 계약

- memory 검색은 `(canonical_url_pattern, task_category, lifecycle_status)` 인덱스를 이용하고 inactive version을 기본 recall에서 제외한다.
- contribution dashboard는 `(contributor_user_id, received_at)`를 이용하며 현재 세션 user 범위로만 조회한다.
- receipt 검사와 점수 멱등성은 opaque receipt id 및 idempotency key에 unique index를 둔다.
- leaderboard 조회는 opt-in 사용자만 선택하고 공개 handle과 기간 점수만 반환한다. 내부 user id, 이메일, device id는 SELECT 결과에 포함되지 않는다.
- retention worker가 purge할 row는 status와 `purge_at`으로 찾을 수 있어야 하며 score/audit event를 cascade delete하지 않는다.

성능 숫자는 이 문서에서 임의 확정하지 않는다. 실제 row 수와 query plan을 [`metrics.md`](metrics.md)에 정한 데이터 세트로 측정하고, 보안·정합성 제약을 제거해 성능을 올리는 최적화는 허용하지 않는다.

## 5. 관련 케이스

API-level 연결은 `API-30~40`, `SCORE-15~24`, `GK-37~48`, `FLOW-04~06`이다. Commit 전/후 연결 단절, worker lease, 원장 commit, purge 재시도는 [`failure-matrix.md`](failure-matrix.md)의 `FAIL-08~09`, `FAIL-15~16`, `FAIL-29`, `FAIL-31`로 fault injection한다. 익명 사용이 score event로 이어지지 않는 것은 [`security-scenarios.md`](security-scenarios.md)의 인증 우회 및 리더보드 어뷰징 케이스에서도 별도로 확인한다. DB 스키마 합격은 MCP 호스트 설치나 웹 UX 합격을 대신하지 않는다.
