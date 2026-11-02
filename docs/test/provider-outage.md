# 모델 provider 일시 장애 대기 테스트 명세

상태: 구현과 검증 완료(2026-11-01). 이 문서를 선행 명세로 먼저 쓰고(커밋 `53cc28d`) 구현 전에 새 동작을 정하는 케이스가 실패하는 것을 확인한 뒤 구현했다. 아래 케이스는 모두 통과했고 실행 결과는 [실행 기록](#실행-기록)에 있다. 기능 정의는 [`../spec/provider-outage.md`](../spec/provider-outage.md)(요구사항 `F-OUT-01~12`)에 있고 이슈는 #158이다. 카탈로그 요약은 [`functional-tests.md`](functional-tests.md)의 `MODEL-15~17`, `QUE-25~33`이다. 키·모델 설정 거부(401, 403, 404, 이슈 #168, 요구사항 `F-OUT-13~15`)의 확장은 `MODEL-18~19`, `QUE-34~39`, 게이트 단위 `B12~B19`이고 같은 날 구현과 검증을 마쳤다. 구현 전 실패 확인과 결과는 [실행 기록](#키모델-설정-거부-이슈-168)에 있다.

## 검증 원칙

- 무엇이 일시 장애이고 무엇이 아닌지(분류), 장애가 job에게 무엇을 하는지(대기·한도·순서), 운영자에게 무엇이 보이는지를 따로 검증한다. 분류를 맞춰도 worker가 틀릴 수 있고 그 반대도 있어서, 한 계층만 보지 않는다.
- 시간은 모두 주입한다(`now` 인자). 실제로 기다리는 테스트는 없다. 단 provider 시간 초과 케이스(MODEL-16)만 300ms짜리 실제 타임아웃을 쓴다.
- 자동 테스트는 모델도 네트워크도 쓰지 않는다. provider 계층은 loopback HTTP fixture, worker 계층은 스크립트 provider와 loopback fixture를 쓴다. 실제 Meta 호출은 이 명세의 범위가 아니다([검증하지 않는 것](#검증하지-않는-것)).
- 새 동작을 정하는 케이스는 구현 전에 실패해야 한다. 이미 통과하는 케이스(회귀 보호)는 따로 표시한다. 구현 전 실패 확인의 결과는 실행 기록에 적었다.
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
| k | 400, 413, 422 | `Failed`, 문구가 `HTTP <코드> from model provider` 하나뿐 (회귀 보호. 401, 403, 404는 이슈 #168에서 `Rejected`가 되어 MODEL-18로 옮겼다) |
| l | 500, 본문이 `{"error":{"message":"bad token fixture-secret <script>"}}` | `Unavailable`, 오류 문자열에 `fixture-secret`·본문 0건 |

자동화: `http_provider_tells_an_outage_from_a_refusal`(a~f, i~k를 표로 돌리고 모든 행에서 provider 문구를 에코하지 않는지 본다), `an_outage_names_the_retry_time_as_seconds_or_a_date_and_ignores_what_it_cannot_read`(g, h와 `Retry-After: 0`, `-5`, 없음), `http_provider_never_repeats_what_a_failing_server_sent`(l). 기존 `http_quota_errors_park_the_worker_and_preserve_retry_after`, `http_retry_after_accepts_an_http_date`, `the_billing_error_the_meta_api_documents_parks_the_job_instead_of_failing_it`, `http_error_messages_do_not_echo_provider_secrets`는 그대로 통과한다. 상태: 통과.

### MODEL-16 — 전송 실패 분류 (F-OUT-01)

| # | 상황 | 기대 |
|---|---|---|
| a | 닫힌 포트(연결 거부) | `Unavailable`, 문구 `HTTP model provider is unreachable` |
| b | 연결은 받지만 응답 없이 멈춤(클라이언트 timeout 300ms) | `Unavailable`, 문구 `HTTP model provider timed out`, 3초 안에 반환 |
| c | 헤더 뒤 본문 중 끊김(`Content-Length: 1000`, 10바이트 보내고 닫음) | `Unavailable` |
| d | 200, 본문이 JSON이 아님 | `Failed` (`invalid JSON`, 회귀 보호) |
| e | 200, 본문이 크기 한도 초과 | `Failed` (`size limit`, 회귀 보호) |
| f | 200, `status: incomplete`, 출력 텍스트 없음 | `Malformed` (MODEL-12, 회귀 보호) |

자동화: `http_provider_treats_a_dead_connection_as_an_outage`(a~c), `http_provider_keeps_treating_unreadable_success_replies_as_failures`(d, e: 회귀 보호), f는 기존 `http_provider_says_when_the_output_budget_went_to_reasoning`. 상태: 통과.

### MODEL-17 — curator 결과 (F-OUT-02, `tests/curator_loop.rs`)

| # | 동작 | 기대 |
|---|---|---|
| a | provider가 첫 턴에 `Unavailable{why:"HTTP 503 from model provider", retry_after_secs:Some(60)}` | `Outcome::Unavailable{같은 값}`, 결정 없음, 저장소 변경 없음, 감사 outcome `provider_unavailable: HTTP 503 from model provider` |
| b | 읽기 도구 두 번이 정상 응답한 뒤 세 번째 턴에서 `Unavailable` | 같다. 앞의 두 턴 기록과 `meta`(비용)가 `JobReport.turns`에 남는다 |
| c | provider가 `Failed`, `Timeout(5)` | 기존대로 `Outcome::ProviderError` (회귀 보호) |
| d | provider가 `QuotaExhausted` | 기존대로 `Outcome::QuotaExhausted` (회귀 보호) |
| e | `ProviderError::Unavailable`을 문자열로 | `the model is temporarily unavailable: HTTP 503 from model provider` |

자동화: `a_provider_that_is_down_ends_the_job_without_a_decision`(a~c, `tests/curator_loop.rs`), `an_outage_says_what_happened_without_the_provider_text`(e, `tests/provider.rs`). d는 기존 `spent_quota_is_its_own_outcome_not_a_failed_attempt_and_changes_nothing`. 상태: 통과.

### MODEL-18 — 거부 분류 (F-OUT-13, 이슈 #168)

fixture 서버가 한 번 응답한다. 요청에는 `Authorization: Bearer fixture-secret`이 붙는다. 401과 404의 본문은 Meta 문서(dev.meta.ai/docs/error-handling)가 적은 형식이고, 403은 문서에 본문 예가 없어서 본문 없음·HTML·JSON 세 형태를 쓴다.

| # | fixture 응답 | 기대 `ProviderError` |
|---|---|---|
| a | 401, `{"error":{"message":"Unauthorized","type":"authentication_error","param":null,"code":"invalid_api_key"}}` | `Rejected`, 문구 `HTTP 401 from model provider` |
| b | 401, 본문 없음(키 누락) | `Rejected` |
| c | 403, 본문 없음 / HTML `<html>Forbidden</html>` / `{"error":{"message":"Forbidden","type":"invalid_request_error","param":null,"code":null}}` | 셋 모두 `Rejected`, 문구 `HTTP 403 from model provider` |
| d | 404, `{"error":{"message":"The model `bogus-model` does not exist or is not available","type":"invalid_request_error","param":null,"code":"model_not_found"}}` | `Rejected`, 문구 `HTTP 404 from model provider` |
| e | 404, HTML 본문(주소 오타) | `Rejected` |
| f | 403, `{"error":{"code":"insufficient_quota"}}` / 사용 한도 문구 본문 | `QuotaExhausted` (소진 신호가 상태 코드보다 우선) |
| g | 401, 본문이 `{"error":{"message":"bad key fixture-secret <script>"}}` | `Rejected`, 오류 문자열에 `fixture-secret`·본문 0건 |
| h | 400, 413, 422 | `Failed`, 문구가 `HTTP <코드> from model provider` 하나뿐 (회귀 보호, MODEL-15 k) |
| i | 402, 429(소진 신호 없음), 503 | 각각 `QuotaExhausted`, `Unavailable`, `Unavailable` (회귀 보호, MODEL-15) |

자동화: `http_provider_tells_a_rejection_from_a_failure`(a~e, h를 표로 돌린다. 400에 `rate_limit_error` 본문을 주는 행과 422에 `authentication_error` 본문을 주는 행으로 "본문이 아니라 상태가 정한다"도 본다), `a_refusal_wins_over_a_rejecting_status`(f), `a_rejection_never_repeats_what_the_server_sent`(g). i는 기존 `http_provider_tells_an_outage_from_a_refusal`이 지킨다. 상태: 통과.

### MODEL-19 — curator와 독립 worker가 거부를 만남 (F-OUT-13)

| # | 동작 | 기대 |
|---|---|---|
| a | provider가 첫 턴에 `Rejected{why:"HTTP 401 from model provider"}` | `Outcome::ProviderRejected{같은 값}`, 결정 없음, 저장소 변경 없음, 감사 outcome `provider_rejected: HTTP 401 from model provider` |
| b | 읽기 도구 두 번이 정상 응답한 뒤 세 번째 턴에서 `Rejected` | 같다. 앞의 두 턴 기록과 `meta`(비용)가 `JobReport.turns`에 남는다 |
| c | `ProviderError::Rejected`를 문자열로 | `the model provider rejected the credentials or the model setting: HTTP 401 from model provider` |
| d | 독립 `wam-gatekeeper` worker engine이 `ProviderRejected`를 만남 | 일반 실패와 같다: 배치가 재시도 대기(`Step::Retry`)가 되고 시도가 쓰인다(범위 밖 동작을 그대로 고정) |

자동화: `a_provider_that_rejects_the_key_ends_the_job_without_a_decision`(a, b, `tests/curator_loop.rs`), `a_rejection_says_what_happened_without_the_provider_text`(c, `tests/provider.rs`), `the_standalone_worker_retries_a_rejection_like_any_failure`(d, `tests/worker.rs`). 상태: 통과.

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
| B11 | `the_log_line_says_why_and_what_to_do` | 운영자 로그 한 줄이 닫힌 이유(장애는 고정 문구 포함), 재개 시각, 대기 job 수와 할 일을 말한다. 장애는 "스스로 다시 시도", 소진은 "충전 뒤 `--resume-budget`", 일일 상한은 "다음 UTC 날짜"이고 열려 있으면 없다 | QUE-31 |

상태: 모두 통과(기존 7개와 함께 18개).

### 거부(이슈 #168): 게이트 단위 테스트

시간은 인자로 준다. 정책은 기본값(거부 pause 3600/21600, 장애 한도 8)이다.

| # | 테스트 | 검증 | ID |
|---|---|---|---|
| B12 | `a_rejection_backs_off_from_an_hour_to_six_hours` | 연속 거부의 pause가 3600, 7200, 14400, 21600, 21600초다. 처음 닫을 때만 `worker_budget_paused`(target `provider_rejected`)가 한 번 남고 열릴 때 `worker_budget_resumed`가 한 번 남는다. 상태가 `blocked_by = provider_rejected`, `rejected_strikes`, `until`, 고정 문구 `reason`을 보여 준다 | QUE-34, QUE-35 |
| B13 | `the_rejected_pause_settings_are_read_strictly` | 기본값 3600/21600, 빈 값은 기본값이다. 숫자가 아님, 30 미만, 86400 초과, 최대가 처음 값보다 작음을 거부하고 오류에 입력 값이 없다. 사용자 지정 값이 pause에 반영된다 | QUE-35 |
| B14 | `an_answer_clears_rejections_and_the_waits_too` | 답이 오면 연속 거부 횟수와 그 job의 대기 횟수가 0이다. 소진이 아닌 일반 실패(`Other`)는 아무것도 바꾸지 않는다 | QUE-36 |
| B15 | `rejections_and_outages_share_a_jobs_free_waits` | 한 job의 장애 대기와 거부 대기가 합쳐서 한도(기본 8)를 쓴다. 한도 0이면 거부도 처음부터 `false`다. 다른 job의 횟수는 독립이다 | QUE-37 |
| B16 | `the_later_pause_names_the_cause_with_rejections` | 거부 pause 중 장애가 오면 재개 시각과 이유가 거부 그대로다. 장애 pause 뒤 더 늦게 끝나는 거부가 오면 거부가 이유가 되고, 거부 pause 뒤 더 늦게 끝나는 소진이 오면 소진이 이유가 된다(거부 표시가 꺼진다) | QUE-34 |
| B17 | `the_log_line_says_to_check_the_key_or_the_model_setting` | 거부로 닫힌 로그 한 줄이 이유(고정 문구 포함), 재개 시각, 대기 job 수와 키(`MODEL_API_KEY`)·모델(`WAM_LLM_MODEL`)·주소(`WAM_LLM_BASE_URL`)를 확인하라는 말과 `--resume-budget`을 말한다. 장애·소진 문구는 그대로다 | QUE-38 |
| B18 | `a_rejected_pause_is_read_by_the_previous_release_as_an_outage` | 거부 pause의 상태 문서는 `pause_cause`가 기존 값 `outage`이고 새 필드 `pause_rejected`, `rejected_strikes`를 더한다. 새 필드가 없는 문서(이전 릴리스가 저장)는 거부 없는 게이트로 읽히고, 새 필드를 지운 문서는 장애 pause로 읽힌다 | QUE-38 |
| B19 | `resume_clears_a_rejection` | `resume_model_budget`이 거부 pause, 거부 표시, 연속 거부 횟수, 이유를 지우고 `waiting_budget` job을 `queued`로 돌려 `worker_budget_resumed`(target `manual`)를 남긴다 | QUE-36 |

상태: 모두 통과(기존 18개와 함께 26개).

## C. 서버 worker 통합 (`crates/wam-server/tests/provider_outage.rs`, 새 파일)

공용 helper(`tests/lifecycle_support/mod.rs`에 추가 예정): `down(retry_after_secs)`(항상 `Unavailable`), `Flaky`(앞의 n번만 장애), `Selective`(특정 route의 job에서만 장애 — 첫 턴에 `job_get`을 불러 job의 route를 읽고 두 번째 턴에서 정한다), loopback 다중 응답 서버(`scripted_server`).

| ID | 테스트(예정) | 사전조건 | 동작 | 기대 |
|---|---|---|---|---|
| QUE-25 | `an_outage_never_sends_a_job_to_review_however_long_it_lasts` | 기여 1건, `down(None)` | 호출 1~7번을 매번 게이트가 열리는 시각에 한다. 8번째는 정상 provider | 7번 모두 `Retry`, 기여 `queued`, `review_required` 이벤트 0, 게시·점수 0. 8번째에 `Published`, 점수 20 한 번. 닫힐 때마다 paused 1·열릴 때마다 resumed 1이고 회복 뒤 두 횟수가 같다. (이전 동작이면 5번째에 검토행이 되므로 구현 전에 실패해야 한다) |
| QUE-26 | `while_the_provider_is_down_only_one_probe_goes_out_per_window` | 기여 3건, `down(None)` | 첫 호출 뒤 +1초, +29초에 호출. 30초에 호출 | +1, +29는 `Idle`이고 호출 수·replay 수가 1에서 늘지 않는다. 3건 모두 `queued`, `queue_health`가 3건을 센다. +30에는 탐침 1건만 나가 호출 수 2 |
| QUE-27 | `the_pause_follows_retry_after_and_otherwise_backs_off` | 기여 1건 | (a) 힌트 없는 장애 7번, (b) 힌트 45초가 계속되는 장애, (c) 힌트 5초, (d) 힌트 100000초 | (a) 재개 시각 간격이 30, 60, 120, 240, 480, 600, 600초이고 `until - 1`에는 `Idle`, `until`에는 호출. (b) 45, 60, 120, 240초(백오프가 힌트를 따라잡는다). (c) 30, 60초. (d) 3600초. `--budget-status`의 `until`이 같은 값을 보인다 |
| QUE-28 | `when_the_provider_returns_everything_waiting_is_processed_and_the_gate_resets` | 서로 다른 route의 기여 3건(A, B, C 순서) | A가 장애를 한 번 만난 뒤 정상 provider로 3번 처리 | A가 장애를 만났으므로 B, C, A 순서로 `Published`(route가 맞지 않는 job을 먼저 가져가면 `Retry`가 되므로 순서가 검증된다). 게이트 열림, 연속 소진 횟수 0, 점수 각 20, 공개 메모리 3건, 닫힘·열림 이벤트 한 번씩 |
| QUE-29 | `a_job_the_provider_cannot_answer_does_not_hold_up_the_others_and_ends_in_review` | 가장 오래된 기여 P(`/poison`), 그 뒤 H1, H2. `Selective` | P가 먼저 장애, 이후 계속 처리 | P의 첫 장애 뒤 H1, H2가 먼저 `Published`(P는 대기 횟수가 늘어 뒤로 밀린다). P는 장애 8번을 시도 없이 기다린 뒤 5번 시도가 쓰이고(총 13번 호출) `waiting_review`가 된다: `review_required` 이벤트 1건(target P), 이후 호출은 `Idle`. P의 점수 0. H1, H2 점수 각 20 |
| QUE-30 | `ordinary_failures_still_use_attempts_and_the_old_behaviour_can_be_restored` | 기여 1건 | (a) `Failed` 5번, (b) 모델이 답했지만 결정 못함 5번, (c) 한도 0 정책으로 `Unavailable` 5번 | 모두 5번째에 검토행(`review_required` 1건)이고 호출 수 5. (a), (b)는 게이트가 닫히지 않는다. (c)는 게이트가 닫히지만 job은 일반 실패처럼 쓰인다. (회귀 보호: QUE-06) |
| QUE-31 | `an_outage_wait_survives_a_restart_and_a_withdrawal_cancels_it` | 파일 저장소, 기여 2건 | 장애 뒤 저장소를 닫고 다시 연다. 한 건을 철회한다. 정상 provider로 처리한다 | 다시 열어도 `blocked_by = provider_outage`와 같은 `until`이고 그 전에는 `Idle`. 철회한 job은 취소되어 모델에 가지 않고 나머지 1건만 게시된다. `waiting_jobs`가 맞다. `wam-worker`의 운영 명령에 해당하는 `resume_model_budget`이 `waiting_budget` job을 `queued`로 돌리고 즉시 탐침이 나간다 |
| QUE-32 | `a_real_http_provider_outage_and_recovery_through_the_worker` | 실제 `OpenAiCompatProvider`(키 `fixture-secret`) + loopback 서버가 503(`Retry-After: 45`), 429(헤더 없음), 정상 응답(`decide_create` 도구 호출)을 차례로 준다 | worker를 세 번 구동한다 | 첫 호출 `Retry`, `until = t0 + 45`, `reason`이 `HTTP 503 from model provider`. `t0 + 44`에는 서버에 요청이 가지 않는다. 두 번째는 429로 60초(연속 2번째 백오프) 닫힌다. 세 번째에 `Published`, 점수 20, 서버가 받은 요청 3건. 상태 JSON·이벤트·outbox 어디에도 `fixture-secret`과 서버가 보낸 본문이 없다 |

상태: 모두 통과.

### 거부(이슈 #168): 서버 worker 통합 (`crates/wam-server/tests/provider_rejected.rs`, 새 파일)

공용 helper(`tests/lifecycle_support/mod.rs`): `rejecting()`(호출 수를 세고 항상 `Rejected{"HTTP 401 from model provider"}`). 회복은 호출마다 provider를 바꿔서 보인다(거부하는 provider, 그 뒤 `provider("high")`).

| ID | 테스트(예정) | 사전조건 | 동작 | 기대 |
|---|---|---|---|---|
| QUE-34 | `a_rejected_key_parks_the_job_without_using_an_attempt_and_closes_the_gate_for_an_hour` | 기여 1건, `rejecting()` | 호출 1번, 같은 시각 +1초, 재개 시각 -1초에 호출 | 첫 호출 `Retry`, 기여 `queued`, 검토행·`review_required` 0, 시도 미소모(`waiting_budget`). `blocked_by = provider_rejected`, `until = t0 + 3600`, `reason = "HTTP 401 from model provider"`. 닫힌 동안은 `Idle`이고 호출·replay 수가 1에서 늘지 않는다. 이벤트 `worker_budget_paused`(target `provider_rejected`) 1, `worker_budget_wait` 1 |
| QUE-35 | `the_pause_for_a_rejection_doubles_from_an_hour_up_to_six_hours` | 기여 1건, `rejecting()` | (a) 기본 정책으로 재개 시각마다 6번 호출, (b) `with_rejection(60, 120)` 정책으로 3번 | (a) 재개 시각 간격 3600, 7200, 14400, 21600, 21600, 21600초. 매번 `Retry`이고 job은 검토로 가지 않는다. (b) 60, 120, 120초 |
| QUE-36 | `fixing_the_key_recovers_by_itself_and_resume_reopens_at_once` | 기여 1건 | (a) 거부 뒤 재개 시각에 정상 provider. (b) 거부 직후 `resume_model_budget`, 그 시각에 정상 provider | (a) `Published`, 점수 20 한 번, `blocked_by = None`, 닫힘·열림 이벤트 한 번씩. (b) `resume_model_budget`이 1건을 풀고 호출이 `Published`다. 둘 다 연속 거부 횟수 0 |
| QUE-37 | `a_job_rejected_too_often_ends_in_review_like_any_failure` | 기여 1건, `rejecting()`, 정책 `with_outage(30, 600, 2)` | 재개 시각마다 호출 | 처음 2번은 시도 없이 대기(`waiting_budget`). 3번째부터 일반 실패처럼 시도가 쓰여 `retry`가 되고 다섯 번째에서 `waiting_review`, `review_required` 1건이다. 게이트는 내내 닫힌다. 한도 0이면 첫 거부부터 시도가 쓰인다. 장애 1번 + 거부 1번이 한도 2를 다 쓴다(합산) |
| QUE-38 | `a_rejection_tells_the_operator_what_to_fix_and_survives_a_restart` | 파일 저장소, 기여 1건 | 거부 뒤 저장소를 닫고 다시 연다. 비교로 장애도 한 번 낸다 | 거부 뒤 `queue_health.alert`가 true(접수 나이가 0이어도), 장애 뒤에는 false다. 다시 열어도 `blocked_by`, `until`, `rejected_strikes`, `reason`이 같고 그 전에는 `Idle`이다. `closed_notice`가 키·모델·주소와 `--resume-budget`을 말한다. 복구 뒤 alert가 false다 |
| QUE-39 | `a_real_http_provider_rejection_and_recovery_through_the_worker` | 실제 `OpenAiCompatProvider`(키 `fixture-secret`) + loopback 서버가 401(Meta 문서의 `invalid_api_key` 본문에 서버 고유 문구를 더함), 정상 응답(`decide_create`)을 차례로 준다 | worker를 네 번 구동한다(거부, 닫힌 동안 `Idle`, 재개 시각 -1초 `Idle`, 재개 시각) | 첫 호출 `Retry`, `until = t0 + 3600`, `reason = "HTTP 401 from model provider"`. `t0 + 3599`에는 서버에 요청이 가지 않는다. 재개 시각에 `Published`. 상태·이벤트·`closed_notice`에 키와 서버가 보낸 본문이 없다 |

상태: 모두 통과.

## D. PostgreSQL (`crates/wam-server/tests/postgres_application.rs`, `#[ignore]`)

| ID | 테스트(예정) | 동작 | 기대 |
|---|---|---|---|
| QUE-33 | `the_outage_gate_is_shared_by_every_connection_and_survives_a_reconnect` | 연결 A에서 장애, 연결 B와 재연결에서 확인 | 장애를 만난 job의 행이 `state = 'waiting_budget'`(기존 CHECK 제약이 허용, migration 없음)이고 다른 연결·재연결도 같은 게이트로 `Idle`이며 `blocked_by`, `until`, `outage_strikes`, `reason`이 같다. job의 대기 횟수가 `wam_app_meta`에 저장된다. 이후 처리가 게시되고 게이트가 깨끗하다. 마이그레이션 개수는 그대로 3이다 |
| QUE-33 | `a_gate_stored_by_the_previous_release_is_read_as_open_and_then_extended` | 이전 릴리스 형식(새 필드 없는) 게이트 문서를 `wam_app_meta`에 직접 써 두고 연결 | 열린 게이트로 읽히고, 장애가 오면 새 필드(`outage_strikes`, `pause_cause`)가 같은 문서에 더해진다 |

상태: 통과(일회용 PostgreSQL 16).

## E. 회귀 보호 (그대로 통과해야 하는 기존 테스트)

- `wam-gatekeeper`: `tests/provider.rs`(402·429 소진, 날짜 형식 `Retry-After`, 비밀 비에코, 추론 불완전, 비용 보고, `spent_quota_is_told_apart_from_passing_failures`), `tests/curator_loop.rs`, `tests/worker.rs`, `tests/budget.rs`.
- `wam-server`: `tests/model_budget.rs` 8개, `tests/application_lifecycle.rs`의 `provider_quota_wait_is_not_a_failed_attempt_and_does_not_call_early`, `tests/postgres_application.rs` 13개, 게이트 단위 테스트 기존 7개.
- 위 기존 테스트 가운데 어느 것도 수정하지 않는 것이 원칙이다. 수정이 필요하면 이유를 [실행 기록](#실행-기록)에 적는다. 이슈 #168에서 수정한 기존 테스트는 둘이다. (1) `http_provider_tells_an_outage_from_a_refusal`의 k 행(400, 401, 403, 404, 413, 422가 `Failed`)에서 401, 403, 404가 `Rejected`로 바뀌므로 그 세 코드를 MODEL-18의 새 테스트로 옮기고 이 표는 400, 413, 422만 남겼다. 이 변경이 명세가 바꾼 동작이다. (2) 일반 실패를 내는 test double의 이름과 문구: `lifecycle_support`의 `Rejected`(HTTP 401)를 `Failing`(HTTP 400)으로, `curator_loop`의 `Raises::Failed`의 문구를 401에서 400으로 바꿨다. 둘 다 `ProviderError::Failed`를 돌려주는 double이라 동작은 같고 "401은 일반 실패"라는 낡은 설명만 없앴다.

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
| F-OUT-13 거부 분류 | MODEL-18, MODEL-19 |
| F-OUT-14 거부 대기 | QUE-34~37, B12~B16, B19 |
| F-OUT-15 운영자 신호와 호환성 | QUE-38, QUE-39, B17, B18 |

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
- **요청 오류(400, 413, 422)의 대기.** 범위 밖이다. 이 오류는 계속 일반 실패이고 MODEL-18 h와 QUE-30이 그것을 고정한다.
- **Meta의 실제 401·403·404 응답과 키를 실제로 폐기했을 때의 동작.** 문서가 적은 형식(401 `invalid_api_key`, 404 `model_not_found`)과 본문 없음·HTML·JSON 세 형태의 403 fixture만 쓴다. 403의 실제 본문은 문서에 없어서 분류가 상태 코드에만 기대는 것으로 안전을 확보했다.
- **PostgreSQL에서의 거부 게이트.** 새 필드는 같은 상태 JSON 문서에 들어가고 migration이 없다. 문서의 직렬화·이전 형식 호환은 B18이 보고, 저장과 연결 사이 공유는 기존 QUE-33이 장애 게이트로 보인다. 거부 전용 PostgreSQL 케이스는 두지 않았다. 기존 PostgreSQL ignored 테스트가 이 변경 뒤에도 통과하는 것은 실행 기록에 있다.
- **독립 `wam-gatekeeper` worker engine.** 범위 밖이다. 일시 장애를 일반 실패로 재시도하는 기존 동작이 그대로이며 기존 `tests/worker.rs`가 지킨다.
- **replay 비용.** 탐침 1건은 browser replay 1회를 쓴다(모델 호출 전에 replay가 먼저 돈다). 장애 중 replay를 아끼는 것은 이번 범위가 아니다.

## 실행 기록

2026-11-01, macOS 27.0 arm64, `rustc 1.97.1`, PostgreSQL 16.15(Homebrew 일회용 클러스터). 작업 트리에는 다른 세션의 미커밋 변경이 함께 있어서 전체 회귀의 개수에 그 테스트가 들어 있다.

| 범위 | 명령 | 결과 |
|---|---|---|
| 구현 전 실패 확인: provider·curator | 새 오류·결과 변형(variant)만 넣고 분류와 매핑은 두지 않은 상태에서 `cargo test --locked -p wam-gatekeeper --test provider --test curator_loop` | curator 17개 중 1개 실패(`a_provider_that_is_down_ends_the_job_without_a_decision`: 일시 장애가 `ProviderError`로 끝남). provider 25개 중 3개 실패(`http_provider_tells_an_outage_from_a_refusal`: 429가 `Failed(429)`, `an_outage_names_the_retry_time_…`, `http_provider_treats_a_dead_connection_as_an_outage`). 나머지 새 테스트 3개는 구현 전에도 통과한다: `…never_repeats_what_a_failing_server_sent`와 `…keeps_treating_unreadable_success_replies_as_failures`는 기존 동작을 고정하는 회귀 보호이고, `an_outage_says_what_happened_…`는 변형과 함께 생긴 문구를 확인한다 |
| 구현 전 실패 확인: 서버 worker | provider 분류를 넣고 worker는 그대로 둔 상태에서 `cargo test --locked -p wam-server --test provider_outage` | 6개 모두 실패(QUE-25, 26, 27, 28, 29, 32). 예: QUE-32는 503 + `Retry-After: 45` 뒤에도 게이트가 열린 채(`(true, None, None)`), QUE-26은 장애 1초 뒤 두 번째 호출이 `Idle`이 아니라 `Retry`로 나갔다 |
| 구현 전에 쓰지 않은 케이스 | | QUE-30, QUE-31, 게이트 단위 B1~B11, PostgreSQL 두 케이스는 구현과 함께 썼다. 구현 전 실패 기록이 없다 |
| 신규 케이스 | `cargo test --locked -p wam-gatekeeper --no-fail-fast`, `cargo test --locked -p wam-server --test provider_outage`, `cargo test --locked -p wam-server --lib budget` | 통과: gatekeeper 전체(`provider` 25개, `curator_loop` 17개), `provider_outage` 8개, 게이트 단위 18개(기존 7개 포함) |
| 전체 회귀 | `cargo test --locked -p wam-gatekeeper -p wam-server --no-fail-fast` | 테스트 대상 42개, 통과 315, 실패 0, ignored 27 |
| 정적 검사 | `cargo clippy --locked -p wam-gatekeeper -p wam-server --all-targets -- -D warnings`, 내 파일만 `rustfmt --edition 2024 --check` | 경고 0, 서식 깨끗 |
| PostgreSQL | 일회용 클러스터에서 `WAM_TEST_DATABASE_URL=… cargo test --locked -p wam-server --test postgres_application -- --ignored --test-threads=1` | 15개 통과(기존 13개 + 새 2개) |
| 커밋된 트리 재검증 | 커밋한 트리만(다른 세션의 미커밋 변경 없이) 별도 worktree에 체크아웃해 서식 검사, clippy, 전체 테스트, 일회용 PostgreSQL의 ignored 테스트를 다시 실행 | 서식·clippy 깨끗. 테스트 대상 38개, 통과 286(기존 260 + 새 26), 실패 0, ignored 26. PostgreSQL `postgres_application` 15개와 `postgres_diagnostics` 2개 통과 |

커밋된 트리 재검증이 한 번 문제를 잡았다. 첫 시도에서 `postgres_application.rs`에 다른 세션의 미커밋 변경(그 세션의 새 migration에 맞춰 마이그레이션 개수 단언을 3에서 4로 바꾼 것)이 내 테스트와 함께 들어가 PostgreSQL 테스트 3개가 실패했다. 내 두 테스트만 담도록 고쳤고, 내 테스트는 마이그레이션 개수에 기대지 않는다("스키마 변경 없음"은 `waiting_budget` 상태 점검이 기존 제약에서 통과하는 것으로 확인한다).

### 키·모델 설정 거부 (이슈 #168)

2026-11-01, 같은 환경(macOS 27.0 arm64, `rustc 1.97.1`). 격리 worktree(`origin/main` 기준)에서 실행해 다른 세션의 미커밋 변경이 섞이지 않았다.

| 범위 | 명령 | 결과 |
|---|---|---|
| 구현 전 실패 확인: provider | 새 변형(`ProviderError::Rejected`, `Outcome::ProviderRejected`)과 curator·독립 engine 매핑만 넣고 HTTP 상태 분류는 두지 않은 상태에서 `cargo test --locked -p wam-gatekeeper --no-fail-fast --test provider --test curator_loop --test worker` | provider 29개 중 1개 실패(`http_provider_tells_a_rejection_from_a_failure`: `Failed(401)`이 나옴, 기대는 `Rejected(401)`). 나머지 새 테스트(`a_refusal_wins_over_a_rejecting_status`, `a_rejection_never_repeats_what_the_server_sent`, `a_rejection_says_what_happened_without_the_provider_text`, curator 1개, 독립 worker 1개)는 변형과 매핑이 이미 있어서 통과했다. 이 케이스들은 회귀 보호이거나 변형만으로 정해지는 동작이다 |
| 구현 전 실패 확인: 서버 worker | 설정 API(`with_rejection`)와 게이트·상태의 필드만 넣고 worker·게이트 동작은 그대로 둔 상태에서 `cargo test --locked -p wam-server --test provider_rejected` | 6개 모두 실패(QUE-34~39). 예: QUE-34는 거부 뒤에도 게이트가 열린 채(`(true, None, None)`), QUE-35는 `until`이 `None`, QUE-37은 `worker_budget_wait` 이벤트가 0(기대 1) |
| 구현 전에 쓰지 않은 케이스 | | 게이트 단위 B12~B19는 구현과 함께 썼다. 구현 전 실패 기록이 없다 |
| 신규 케이스 | `cargo test --locked -p wam-gatekeeper --no-fail-fast`, `cargo test --locked -p wam-server --test provider_rejected`, `cargo test --locked -p wam-server --lib budget` | 통과: gatekeeper `provider` 29개, `curator_loop` 18개, `worker` 16개, `provider_rejected` 6개, 게이트 단위 26개(기존 18개 포함) |
| 전체 회귀 | `cargo test --workspace --locked --no-fail-fast` | 통과 473, 실패 0, ignored 30(같은 날 직전 기준 453 + 새 20) |
| 정적 검사 | `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo fmt --all -- --check` | 경고 0, 서식 깨끗 |
| PostgreSQL | 일회용 PostgreSQL 16.15 클러스터에서 `WAM_TEST_DATABASE_URL=… cargo test --locked -p wam-server --test postgres_application --test postgres_diagnostics -- --ignored --test-threads=1` | `postgres_application` 16개, `postgres_diagnostics` 2개 통과. 거부 전용 케이스는 없고, 게이트 문서 호환(`a_gate_stored_by_the_previous_release_is_read_as_open_and_then_extended`)과 연결 간 공유는 기존 케이스가 본다 |
