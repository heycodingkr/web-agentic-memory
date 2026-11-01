# 모델 provider 일시 장애 대기 테스트 명세

상태: 선행 명세(2026-11-01). 아래 케이스는 모두 **미실행**이다. 구현 전에 먼저 실패하는 것을 확인하고, 구현 뒤 통과 결과를 [실행 기록](#실행-기록)에 적는다. 기능 정의는 [`../spec/provider-outage.md`](../spec/provider-outage.md)(요구사항 `F-OUT-01~12`)에 있고 이슈는 #158이다. 카탈로그 요약은 [`functional-tests.md`](functional-tests.md)의 `MODEL-15~17`, `QUE-25~33`이다.

## 검증 원칙

- 무엇이 일시 장애이고 무엇이 아닌지(분류), 장애가 job에게 무엇을 하는지(대기·한도·순서), 운영자에게 무엇이 보이는지를 따로 검증한다. 분류를 맞춰도 worker가 틀릴 수 있고 그 반대도 있어서, 한 계층만 보지 않는다.
- 시간은 모두 주입한다(`now` 인자). 실제로 기다리는 테스트는 없다. 단 provider 시간 초과 케이스(MODEL-16)만 300ms짜리 실제 타임아웃을 쓴다.
- 자동 테스트는 모델도 네트워크도 쓰지 않는다. provider 계층은 loopback HTTP fixture, worker 계층은 스크립트 provider와 loopback fixture를 쓴다. 실제 Meta 호출은 이 명세의 범위가 아니다([검증하지 않는 것](#검증하지-않는-것)).
- 새 동작을 정하는 케이스는 구현 전에 실패해야 한다. 이미 통과하는 케이스(회귀 보호)는 따로 표시한다.
- 합성 데이터만 쓴다. API key 자리에는 `fixture-secret` 같은 sentinel만 넣고, 결과 어디에도 나타나지 않는지 검사한다.

## 환경과 실행

```sh
# provider·curator 계층 (loopback fixture)
cargo test --locked -p wam-gatekeeper --test provider --test curator_loop
# 게이트 단위 테스트와 서버 worker 통합 테스트
cargo test --locked -p wam-server --lib budget
cargo test --locked -p wam-server --test provider_outage --test model_budget --test application_lifecycle
# PostgreSQL (일회용 DB, 기본 실행에서는 ignored)
WAM_TEST_DATABASE_URL='postgres://…@127.0.0.1:…/…' \
  cargo test --locked -p wam-server --test postgres_application -- --ignored
# 전체와 정적 검사
cargo test --locked -p wam-gatekeeper -p wam-server
cargo clippy --locked -p wam-gatekeeper -p wam-server --all-targets -- -D warnings
```

## A. provider 분류 (`crates/wam-gatekeeper/tests/provider.rs`)

### MODEL-15 — HTTP 상태와 본문 분류 (F-OUT-01)

fixture 서버가 한 번 응답한다. 요청에는 `Authorization: Bearer fixture-secret`이 붙는다.

| # | fixture 응답 | 기대 `ProviderError` |
|---|---|---|
| a | 429, `{"error":{"type":"rate_limit_error","code":"rate_limit_exceeded"}}`, `Retry-After: 7` | `Unavailable`, `retry_after_secs = Some(7)` |
| b | 429, 본문 없음, 헤더 없음 | `Unavailable`, `None` |
| c | 503, `service_overloaded`, `Retry-After: 60` | `Unavailable`, `Some(60)` |
| d | 503 `backend_unavailable`, 503 `server_shutting_down` | `Unavailable` |
| e | 500, 502, 504, 529, 408 | `Unavailable` |
| f | 502, HTML 본문 `<html>Bad Gateway</html>` | `Unavailable` (`invalid JSON` 실패가 아니다) |
| g | 503, `Retry-After`가 30초 뒤의 HTTP 날짜 | `Unavailable`, 25~30초 |
| h | 503, `Retry-After: soon` | `Unavailable`, `None` |
| i | 429, `{"error":{"code":"insufficient_quota"}}` | `QuotaExhausted` (기존, 회귀 보호) |
| j | 402, `type: billing_error`, `code: billing_not_configured` | `QuotaExhausted` (MODEL-14, 회귀 보호) |
| k | 400, 401, 403, 404, 413, 422 | `Failed`, 문구가 `HTTP <코드> from model provider` 하나뿐 (회귀 보호) |
| l | 500, 본문이 `{"error":{"message":"bad token fixture-secret <script>"}}` | `Unavailable`, 오류 문자열에 `fixture-secret`·본문 0건 |

자동화(예정): `http_provider_tells_an_outage_from_a_refusal`(표 구동), `http_provider_never_repeats_what_a_failing_server_sent`(l), 기존 `http_quota_errors_park_the_worker_and_preserve_retry_after`, `http_retry_after_accepts_an_http_date`, `the_billing_error_the_meta_api_documents_parks_the_job_instead_of_failing_it`, `http_error_messages_do_not_echo_provider_secrets`는 그대로 통과해야 한다. 상태: 미실행.

### MODEL-16 — 전송 실패 분류 (F-OUT-01)

| # | 상황 | 기대 |
|---|---|---|
| a | 닫힌 포트(연결 거부) | `Unavailable`, 문구 `HTTP model provider is unreachable` |
| b | 연결은 받지만 응답 없이 멈춤(클라이언트 timeout 300ms) | `Unavailable`, 문구 `HTTP model provider timed out`, 3초 안에 반환 |
| c | 헤더 뒤 본문 중 끊김(`Content-Length: 1000`, 10바이트 보내고 닫음) | `Unavailable` |
| d | 200, 본문이 JSON이 아님 | `Failed` (`invalid JSON`, 회귀 보호) |
| e | 200, 본문이 크기 한도 초과 | `Failed` (`size limit`, 회귀 보호) |
| f | 200, `status: incomplete`, 출력 텍스트 없음 | `Malformed` (MODEL-12, 회귀 보호) |

자동화(예정): `http_provider_treats_a_dead_connection_as_an_outage`(a~c), 기존 응답 형식 테스트는 그대로 통과해야 한다. 상태: 미실행.

### MODEL-17 — curator 결과 (F-OUT-02, `tests/curator_loop.rs`)

| # | 동작 | 기대 |
|---|---|---|
| a | provider가 첫 턴에 `Unavailable{why:"HTTP 503 from model provider", retry_after_secs:Some(60)}` | `Outcome::Unavailable{같은 값}`, 결정 없음, 저장소 변경 없음, 감사 outcome `provider_unavailable: HTTP 503 from model provider` |
| b | 읽기 도구 두 번이 정상 응답한 뒤 세 번째 턴에서 `Unavailable` | 같다. 앞의 두 턴 기록과 `meta`(비용)가 `JobReport.turns`에 남는다 |
| c | provider가 `Failed`, `Timeout(5)` | 기존대로 `Outcome::ProviderError` (회귀 보호) |
| d | provider가 `QuotaExhausted` | 기존대로 `Outcome::QuotaExhausted` (회귀 보호) |
| e | `ProviderError::Unavailable`을 문자열로 | `the model is temporarily unavailable: HTTP 503 from model provider` |

자동화(예정): `a_provider_that_is_down_ends_the_job_without_a_decision`(a, b, e), 기존 `curator_loop` 테스트는 그대로 통과해야 한다. 상태: 미실행.

## B. 게이트 계산 (`crates/wam-server/src/durable/budget.rs` 단위 테스트)

시간은 인자로 준다. 정책은 기본값(30, 600, 한도 8, 소진 pause 900/3600)이다.

| # | 테스트(예정) | 검증 | ID |
|---|---|---|---|
| B1 | `an_outage_backs_off_from_thirty_seconds_to_ten_minutes` | 알린 시간이 없으면 닫히는 시간이 30, 60, 120, 240, 480, 600, 600초다. 처음 닫을 때만 `worker_budget_paused`(target `provider_outage`)가 한 번 남고 닫혀 있는 동안은 더 남지 않는다 | QUE-27 |
| B2 | `retry_after_beats_a_shorter_backoff_and_is_bounded` | 5초는 30초로, 첫 장애의 45초는 45초로, 세 번째 장애(백오프 120)의 45초는 120초로, 100000초는 3600초로 닫는다. 나중의 짧은 pause가 이미 정한 재개 시각을 앞당기지 않는다 | QUE-27 |
| B3 | `an_answer_clears_both_counts_and_the_jobs_waits` | 답이 오면 연속 소진·연속 장애 횟수와 그 job의 장애 대기 횟수가 0이다. 소진이 아닌 일반 실패(`Other`)는 아무것도 바꾸지 않는다 | QUE-28 |
| B4 | `the_later_pause_names_the_cause` | 소진 pause(900초) 중 장애가 오면 재개 시각과 이유가 소진 그대로다. 장애 pause 뒤 소진 거절이 오면 소진이 이유가 된다. 닫힌 이유 이벤트는 열린 상태에서 닫힐 때만 남는다 | QUE-27 |
| B5 | `a_job_has_a_limited_number_of_free_waits` | 한 job은 처음 8번의 장애가 시도 없는 대기(`true`), 9번째부터 `false`다. 다른 job의 횟수는 독립이고, 한도 0이면 처음부터 `false`다. 답이 오면 그 job의 횟수가 0이 된다 | QUE-29 |
| B6 | `waits_of_jobs_that_are_gone_are_forgotten` | 끝났거나 없어진 job의 대기 횟수 항목은 다음 기록 때 지워진다. 진행 중인 job의 항목은 남는다 | QUE-29 |
| B7 | `status_names_an_outage_and_keeps_the_reason_clean` | 상태가 `blocked_by = provider_outage`, `outage_strikes`, `until`, `reason`을 보여 준다. 제어 문자와 허용 밖 문자는 제거되고 80자를 넘지 않는다 | QUE-31 |
| B8 | `the_outage_settings_are_read_strictly` | 기본값 30/600/8, 빈 값은 기본값이다. 숫자가 아님, 30 미만, 86400 초과, 최대가 처음 값보다 작음, 한도 1000 초과를 거부한다. 한도 0은 허용한다. 오류에 입력 값이 없다 | QUE-30 |
| B9 | `an_older_gate_document_is_read_as_an_open_gate` | 새 필드가 없는 JSON(이전 릴리스가 저장)을 읽으면 연속 장애 0, 대기 횟수 없음, 소진이 이유인 열린 게이트다. 다시 쓰면 새 필드가 들어간다 | QUE-33 |
| B10 | `resume_clears_the_outage_state` | `resume_model_budget`이 장애 pause, 두 연속 횟수, 장애 대기 횟수, 이유를 지우고 `waiting_budget` job 수를 돌려주며 `worker_budget_resumed`(target `manual`)를 남긴다 | QUE-31 |

상태: 모두 미실행.

## C. 서버 worker 통합 (`crates/wam-server/tests/provider_outage.rs`, 새 파일)

공용 helper(`tests/lifecycle_support/mod.rs`에 추가 예정): `down(retry_after_secs)`(항상 `Unavailable`), `Flaky`(앞의 n번만 장애), `Selective`(특정 route의 job에서만 장애 — 첫 턴에 `job_get`을 불러 job의 route를 읽고 두 번째 턴에서 정한다), loopback 다중 응답 서버(`scripted_server`).

| ID | 테스트(예정) | 사전조건 | 동작 | 기대 |
|---|---|---|---|---|
| QUE-25 | `an_outage_never_sends_a_job_to_review_however_long_it_lasts` | 기여 1건, `down(None)` | 호출 1~7번을 매번 게이트가 열리는 시각에 한다. 8번째는 정상 provider | 7번 모두 `Retry`, 기여 `queued`, `review_required` 이벤트 0, 게시·점수 0. 8번째에 `Published`, 점수 20 한 번. 닫힐 때마다 paused 1·열릴 때마다 resumed 1이고 회복 뒤 두 횟수가 같다. (이전 동작이면 5번째에 검토행이 되므로 구현 전에 실패해야 한다) |
| QUE-26 | `while_the_provider_is_down_only_one_probe_goes_out_per_window` | 기여 3건, `down(None)` | 첫 호출 뒤 +1초, +29초에 호출. 30초에 호출 | +1, +29는 `Idle`이고 호출 수·replay 수가 1에서 늘지 않는다. 3건 모두 `queued`, `queue_health`가 3건을 센다. +30에는 탐침 1건만 나가 호출 수 2 |
| QUE-27 | `the_pause_follows_retry_after_and_otherwise_backs_off` | 기여 1건 | (a) 힌트 없는 장애 7번, (b) 힌트 45초 장애, (c) 힌트 5초 장애 | (a) 재개 시각 간격이 30, 60, 120, 240, 480, 600, 600초이고 `until - 1`에는 `Idle`, `until`에는 호출. (b) 45초. (c) 30초. `--budget-status`의 `until`이 같은 값을 보인다 |
| QUE-28 | `when_the_provider_returns_everything_waiting_is_processed_in_order` | 서로 다른 route의 기여 3건(A, B, C 순서) | 장애 2번 뒤 정상 provider로 3번 처리 | A, B, C 순서로 `Published`(route가 맞지 않는 job을 먼저 가져가면 `Retry`가 되므로 순서가 검증된다). 게이트 열림, 연속 횟수 0, 점수 각 20, 공개 메모리 3건 |
| QUE-29 | `a_job_the_provider_cannot_answer_does_not_hold_up_the_others_and_ends_in_review` | 가장 오래된 기여 P(`/poison`), 그 뒤 H1, H2. `Selective` | P가 먼저 장애, 이후 계속 처리 | P의 첫 장애 뒤 H1, H2가 먼저 `Published`(P는 대기 횟수가 늘어 뒤로 밀린다). P는 장애 8번을 시도 없이 기다린 뒤 5번 시도가 쓰이고(총 13번 호출) `waiting_review`가 된다: `review_required` 이벤트 1건(target P), 이후 호출은 `Idle`. P의 점수 0. H1, H2 점수 각 20 |
| QUE-30 | `ordinary_failures_still_use_attempts_and_the_old_behaviour_can_be_restored` | 기여 1건 | (a) `Failed` 5번, (b) 모델이 답했지만 결정 못함 5번, (c) 한도 0 정책으로 `Unavailable` 5번 | 모두 5번째에 검토행(`review_required` 1건)이고 호출 수 5. (a), (b)는 게이트가 닫히지 않는다. (c)는 게이트가 닫히지만 job은 일반 실패처럼 쓰인다. (회귀 보호: QUE-06) |
| QUE-31 | `an_outage_wait_survives_a_restart_and_a_withdrawal_cancels_it` | 파일 저장소, 기여 2건 | 장애 뒤 저장소를 닫고 다시 연다. 한 건을 철회한다. 정상 provider로 처리한다 | 다시 열어도 `blocked_by = provider_outage`와 같은 `until`이고 그 전에는 `Idle`. 철회한 job은 취소되어 모델에 가지 않고 나머지 1건만 게시된다. `waiting_jobs`가 맞다. `wam-worker`의 운영 명령에 해당하는 `resume_model_budget`이 `waiting_budget` job을 `queued`로 돌리고 즉시 탐침이 나간다 |
| QUE-32 | `a_real_http_provider_outage_and_recovery_through_the_worker` | 실제 `OpenAiCompatProvider`(키 `fixture-secret`) + loopback 서버가 503(`Retry-After: 45`), 429(헤더 없음), 정상 응답(`decide_create` 도구 호출)을 차례로 준다 | worker를 세 번 구동한다 | 첫 호출 `Retry`, `until = t0 + 45`, `reason`이 `HTTP 503 from model provider`. `t0 + 44`에는 서버에 요청이 가지 않는다. 두 번째는 429로 60초(연속 2번째 백오프) 닫힌다. 세 번째에 `Published`, 점수 20, 서버가 받은 요청 3건. 상태 JSON·이벤트·outbox 어디에도 `fixture-secret`과 서버가 보낸 본문이 없다 |

상태: 모두 미실행.

## D. PostgreSQL (`crates/wam-server/tests/postgres_application.rs`, `#[ignore]`)

| ID | 테스트(예정) | 동작 | 기대 |
|---|---|---|---|
| QUE-33 | `the_outage_gate_is_shared_by_every_connection_and_survives_a_reconnect` | 연결 A에서 장애, 연결 B와 재연결에서 확인. 이전 릴리스 형식(새 필드 없는) 게이트 문서를 `wam_app_meta`에 직접 써 두고 연결 | 장애를 만난 job의 행이 `state = 'waiting_budget'`(기존 CHECK 제약이 허용, migration 없음)이고 다른 연결·재연결도 같은 게이트로 `Idle`이다. 이전 형식 문서는 열린 게이트로 읽혀 처리가 이어진다. 마이그레이션 개수는 그대로 3이다 |

상태: 미실행.

## E. 회귀 보호 (그대로 통과해야 하는 기존 테스트)

- `wam-gatekeeper`: `tests/provider.rs`(402·429 소진, 날짜 형식 `Retry-After`, 비밀 비에코, 추론 불완전, 비용 보고, `spent_quota_is_told_apart_from_passing_failures`), `tests/curator_loop.rs`, `tests/worker.rs`, `tests/budget.rs`.
- `wam-server`: `tests/model_budget.rs` 8개, `tests/application_lifecycle.rs`의 `provider_quota_wait_is_not_a_failed_attempt_and_does_not_call_early`, `tests/postgres_application.rs` 13개, 게이트 단위 테스트 기존 7개.
- 위 기존 테스트 가운데 어느 것도 수정하지 않는 것이 원칙이다. 수정이 필요하면 이유를 [실행 기록](#실행-기록)에 적는다.

## 요구사항 추적

| 요구사항 | 테스트 |
|---|---|
| F-OUT-01 분류 | MODEL-15, MODEL-16 |
| F-OUT-02 curator 결과 | MODEL-17 |
| F-OUT-03 시도 없는 대기 | QUE-25 |
| F-OUT-04 전역 게이트 | QUE-26 |
| F-OUT-05 대기 시간 | QUE-27, B1, B2, B4 |
| F-OUT-06 회복과 순서 | QUE-28, B3 |
| F-OUT-07 한도와 순환 | QUE-29, B5, B6 |
| F-OUT-08 기존 계약과 되돌리기 | QUE-30, B8 |
| F-OUT-09 지속성·철회·운영자 | QUE-31, QUE-33, B10 |
| F-OUT-10 실제 HTTP 경로 | QUE-32 |
| F-OUT-11 노출과 비밀 | QUE-32, B7, MODEL-15 l |
| F-OUT-12 호환성 | QUE-33, B9 |

## 종료 기준

1. 위 케이스가 모두 자동 테스트로 존재하고 통과한다. 새 동작을 정하는 케이스는 구현 전에 실패하는 것을 먼저 확인해 기록한다.
2. 기존 `cargo test --locked -p wam-gatekeeper -p wam-server`가 실패 0이다(직전 검증인 2026-11-01 커밋 `2a2f4f1` 기준 260 passed, 24 ignored. 다른 세션의 변경으로 숫자는 달라질 수 있다).
3. `cargo clippy --locked -p wam-gatekeeper -p wam-server --all-targets -- -D warnings`와 rustfmt가 내가 바꾼 파일에서 깨끗하다.
4. 일회용 PostgreSQL에서 `postgres_application`의 ignored 테스트가 통과한다.
5. 커밋된 트리만 별도 worktree에 체크아웃해 같은 검사를 다시 통과한다.
6. 운영 문서(`docs/operations/deployment.md`), 사용 문서(`docs/design/meta-model-usage*.md`), 검증 기록(`docs/test/meta-model.md`)이 새 동작과 맞고 이슈 #158에 근거(커밋, 테스트)를 남긴다.

## 검증하지 않는 것

- **Meta의 실제 429·503·점검 응답.** 문서가 적은 형식을 재현한 fixture만 쓴다. 실제 응답이 다르면(예: 429에 `Retry-After`가 없음) 분류는 같지만 대기 시간은 백오프를 따른다.
- **몇 시간 이어지는 실제 장애.** 시간은 주입한다. 장애 pause의 합계(기본 약 45분)와 한도 뒤 검토행은 계산으로만 검증한다.
- **여러 worker가 한 창에 동시에 탐침하는 것.** 보장하지 않는다(worker 수만큼 나간다). 같은 시각에 이어서 호출하는 순차 테스트(QUE-26)만 있고 진짜 경쟁은 검증하지 않는다.
- **인증·요청 오류의 대기.** 범위 밖이다. 이 오류는 계속 일반 실패이고 QUE-30이 그것을 고정한다.
- **독립 `wam-gatekeeper` worker engine.** 범위 밖이다. 일시 장애를 일반 실패로 재시도하는 기존 동작이 그대로이며 기존 `tests/worker.rs`가 지킨다.
- **replay 비용.** 탐침 1건은 browser replay 1회를 쓴다(모델 호출 전에 replay가 먼저 돈다). 장애 중 replay를 아끼는 것은 이번 범위가 아니다.

## 실행 기록

아직 실행하지 않았다. 구현 뒤 아래 표를 채운다.

| 날짜 | 범위 | 명령 | 결과 |
|---|---|---|---|
| | 구현 전 실패 확인 | | 미실행 |
| | 신규 케이스 | | 미실행 |
| | 전체 회귀 | | 미실행 |
| | PostgreSQL | | 미실행 |
| | 커밋된 트리 재검증 | | 미실행 |
