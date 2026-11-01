# Meta 모델 provider·예산 검증 기록

확인일: 2026-11-01. 대상은 Meta Model API(Muse Spark 1.3)를 쓰는 Gatekeeper provider, 비용 보고, 서버 worker의 모델 예산 게이트(크레딧이 떨어지면 기여를 큐에서 기다리게 하는 동작과 일일 지출 상한)다. 테스트 ID는 `MODEL-11~14`, `QUE-20~24`([`functional-tests.md`](functional-tests.md))이고 provider 일시 장애 대기(`MODEL-15~17`, `QUE-25~33`)는 [`provider-outage.md`](provider-outage.md)에 따로 있으며, 사용·비용 요약은 [`../design/meta-model-usage.ko.md`](../design/meta-model-usage.ko.md), 동작 정의는 [`../spec/queue.md`](../spec/queue.md)와 [`../spec/model.md`](../spec/model.md)에 있다. 자동 테스트와 실제 호출 평가를 구분해서 적고, 검증하지 못한 것은 [8절](#8-검증하지-못한-것)에 모았다.

## 1. 상태 요약

| 영역 | 상태 | 근거 |
|---|---|---|
| 추론 강도 설정, 불완전 응답 구분 (MODEL-11, MODEL-12) | 자동 테스트 통과, 실제 호출로 효과 측정 | 3절, 4.3절, 4.5절 |
| 비용 보고와 가격표 (MODEL-13) | 자동 테스트 통과, 실제 응답의 `usage`로 계산 확인 | 3절, 4.6절 |
| Meta가 문서화한 402 결제 오류 (MODEL-14) | 문서의 본문을 재현한 fixture로 통과, 실제 소진 응답은 관찰하지 못함 | 3절, 8절 |
| 소진 대기·재개·재시작 (QUE-20, QUE-21, QUE-23) | 단위·통합·PostgreSQL 통과 | 3절 |
| 일일 지출 상한 (QUE-22) | 단위·통합 통과, `wam-worker` 시작 검사 스모크 | 3절, 5절 |
| 이전 릴리스 DB 업그레이드 (QUE-24) | disposable PostgreSQL 통과 | 3절 |
| provider 일시 장애 대기 (MODEL-15~17, QUE-25~33) | 단위·통합·PostgreSQL 통과, 실제 Meta의 429·503은 관찰하지 못함 | [`provider-outage.md`](provider-outage.md) |
| 모델 품질 | 실제 호출 표본: 시드 5/5, `eval-60` 58/60 (1회) | 4절 |

## 2. 환경

- macOS 27.0 arm64, `rustc 1.97.1`, `cargo 1.97.1`, PostgreSQL 16.15(Homebrew)의 일회용 클러스터.
- 자동 테스트는 네트워크도 모델도 쓰지 않는다. loopback HTTP fixture와 스크립트 provider만 쓴다.
- 실제 호출은 Meta Model API(`https://api.meta.ai/v1/responses`)의 `muse-spark-1.3`이다. 키는 git에서 제외된 `.env`에서 환경변수로만 읽고 로그·문서에 남기지 않았다. 계정은 무료 크레딧을 쓴다.
- 결과 확인은 커밋된 트리만 별도 worktree에 체크아웃해서 다시 했다. 다른 세션의 미커밋 변경은 포함되지 않는다.

## 3. 자동 테스트

이번 작업에서 추가한 테스트는 25개다(provider 8, 예산 게이트 단위 7과 통합 8, PostgreSQL 2).

### provider (`crates/wam-gatekeeper`)

| 테스트 | ID | 검증 내용 |
|---|---|---|
| `http_provider_sends_reasoning_effort_only_when_asked` | MODEL-11 | 설정하면 요청 본문에 `reasoning.effort`가 들어가고(공백·대소문자 정리), 미설정이면 필드가 없다. 응답 메타에 사용한 값이 남는다 |
| `http_provider_refuses_an_unknown_reasoning_effort_without_echoing_it` | MODEL-11 | `none`, 빈 값과 공백, 오타(`turbo`), 구분자가 섞인 값(`low;drop`)은 요청 전에 설정 오류가 되고 오류에 입력값이 없다 |
| `http_provider_says_when_the_output_budget_went_to_reasoning` | MODEL-12 | `status: incomplete`와 `max_output_tokens`는 `incomplete (max_output_tokens)`로 보고한다. 모르는 reason 문자열(주입 시도 포함)은 `unspecified`로 바꿔 그대로 옮기지 않고, 평범한 빈 응답은 기존 문구를 유지한다 |
| `http_provider_reports_what_a_reply_cost_only_when_prices_are_configured` | MODEL-13 | 입력 1,000(캐시 400)·출력 500 토큰과 가격 1.25/0.15/4.25에서 $0.002935를 보고한다. 가격표가 없으면 `cost_usd`가 비고 `reports_cost`가 거짓이다 |
| `model_prices_must_be_sane_and_a_reply_without_token_counts_costs_nothing` | MODEL-13 | 음수·NaN·무한·상한(1000) 초과 가격을 거부한다. `usage`가 없는 응답은 비용이 없고, 캐시 토큰이 입력보다 많아도 비용이 캐시 단가 아래로 내려가지 않는다 |
| `the_billing_error_the_meta_api_documents_parks_the_job_instead_of_failing_it` | MODEL-14 | 402 `billing_error`(`billing_not_configured`)가 소진 문구 없이도 재시도 시간 없는 쿼터 소진으로 분류된다 |
| `prices_come_as_a_complete_list_or_not_at_all` (단위) | MODEL-13 | `WAM_LLM_PRICE_*` 셋 모두 또는 없음, 공백은 미설정, 일부만 있으면 거부 |
| `an_unreadable_price_is_refused_without_repeating_it` (단위) | MODEL-13 | 숫자가 아닌 가격은 거부하고 오류에 값이 없다 |

### 예산 게이트 단위 테스트 (`crates/wam-server/src/durable/budget.rs`)

| 테스트 | ID | 검증 내용 |
|---|---|---|
| `without_a_hint_each_consecutive_refusal_doubles_the_pause_up_to_the_maximum` | QUE-21 | 힌트가 없으면 15분, 30분, 1시간, 1시간이다. 시작과 해제 이벤트가 한 번씩 남는다 |
| `a_provider_hint_is_used_within_bounds_and_an_answer_ends_the_backoff` | QUE-21 | 힌트 5초는 30초, 120초는 그대로, 100,000초는 1시간으로 제한한다. 응답을 받으면 연속 거절 횟수가 0이 된다 |
| `a_failure_that_is_not_a_refusal_changes_nothing_and_a_pause_never_shortens` | QUE-21 | 소진이 아닌 실패는 게이트를 바꾸지 않고, 진행 중인 pause는 짧아지지 않는다 |
| `the_daily_ceiling_closes_the_gate_until_the_next_utc_day` | QUE-22 | 상한에 닿으면 다음 UTC 날짜 0시에 열리고 이벤트는 한 번씩이다 |
| `the_spending_day_only_moves_forward_when_clocks_disagree` | QUE-22 | worker 시계가 어긋나도 날짜와 합계가 되돌아가지 않는다 |
| `status_tells_an_operator_why_and_until_when` | QUE-23 | 닫힌 이유, 재개 시각, 오늘 지출, 상한을 보여 준다 |
| `the_settings_are_read_strictly` | QUE-22 | 0·음수·NaN·inf·문자·상한 초과·범위 밖 값, 최대값이 기본값보다 작은 설정을 거부한다 |

### 서버 worker 통합 테스트 (`crates/wam-server/tests/model_budget.rs`)

| 테스트 | ID | 검증 내용 |
|---|---|---|
| `a_refusal_closes_the_gate_for_every_queued_job_and_nothing_runs_while_it_is_closed` | QUE-20 | 기여 3건 중 첫 거절 뒤 899초 안의 호출이 모두 `Idle`이고, 모델 호출 1회·replay 1회에서 늘지 않는다. 3건 모두 `queued`이고 `queue_health`가 3건을 센다 |
| `the_pause_follows_the_providers_hint_and_otherwise_doubles_up_to_an_hour` | QUE-21 | 힌트 없는 연속 거절은 900, 1,800, 3,600, 3,600초이고 힌트는 30초~1시간 안에서 쓴다. 탐침은 pause마다 1회다 |
| `when_credit_returns_the_waiting_job_publishes_and_the_gate_resets` | QUE-21 | 대기 중 접수된 기여도 대기하고, 크레딧이 돌아오면 게시되며 게이트와 연속 거절 횟수가 초기화된다. 이벤트는 한 번씩이다 |
| `the_pause_survives_a_restart` | QUE-21 | 파일 저장소를 닫고 다시 열어도 pause가 유지되어 탐침이 나가지 않는다 |
| `the_daily_ceiling_stops_new_jobs_until_the_next_utc_day` | QUE-22 | 상한을 넘긴 첫 job은 끝나고, 이후 claim·replay·모델 호출이 없다. 다음 UTC 날짜에 합계 0으로 재개한다 |
| `an_exact_duplicate_is_settled_without_the_model_so_it_costs_nothing` | QUE-22 | 같은 경로의 같은 단계 기여는 규칙으로 처리되어 모델 호출도 비용도 없다 |
| `an_operator_can_reopen_the_gate_as_soon_as_credit_is_added` | QUE-23 | `resume_model_budget`이 대기 job을 풀어 즉시 시작하고, 열린 게이트에 다시 호출해도 해롭지 않다 |
| `withdrawing_consent_cancels_a_job_that_waits_for_budget` | QUE-23 | 동의를 철회한 `waiting_budget` job은 취소되어 모델에 가지 않는다 |

### PostgreSQL (`crates/wam-server/tests/postgres_application.rs`, `#[ignore]`, disposable DB 필요)

| 테스트 | ID | 검증 내용 |
|---|---|---|
| `the_model_budget_gate_is_shared_by_every_connection_and_survives_a_reconnect` | QUE-21, QUE-24 | `wam_work_items`에 `waiting_budget` 행이 생긴다. 다른 연결이 같은 닫힌 게이트를 읽고 아무것도 하지 않는다. 수동 재개 뒤 상태가 `queued`가 되고, 일일 지출이 연결 사이에 공유된다 |
| `a_database_from_before_the_budget_gate_is_upgraded_in_place` | QUE-24 | migration 1·2만 적용된 DB에 접속하면 `0003`이 적용되어 `waiting_budget`이 허용되고 그 밖의 상태는 계속 거부된다 |

기존 `migrations_profiles_intake_queue_and_outbox_survive_reopen`의 migration 개수 단언은 2에서 3으로 바꿨다. 기존 계약 `application_lifecycle::provider_quota_wait_is_not_a_failed_attempt_and_does_not_call_early`(QUE-08)는 수정 없이 통과한다.

### 결과

| 명령 | 결과 |
|---|---|
| `cargo test --locked -p wam-gatekeeper -p wam-server` | **260 pass / 0 fail / 24 ignored** (ignored는 DB·Redis 필요) |
| `cargo clippy --locked -p wam-gatekeeper -p wam-server --all-targets -- -D warnings` | 경고 없음 |
| `rustfmt --edition 2024 --check` (이번에 바꾼 Rust 파일) | 차이 없음 |
| `cargo test -p wam-server --test postgres_application -- --ignored` (PostgreSQL 16) | 13 pass (신규 2 포함) |
| `cargo test -p wam-server --test postgres_diagnostics -- --ignored` | 2 pass |

## 4. 실제 Meta Model API 평가

### 4.1 방법

- 운영과 같은 curator 코드 경로(`wam-curate --provider muse`)를 썼다. 정확 중복 규칙은 꺼서 모델이 직접 판단하게 했다. 기본 모델은 `muse-spark-1.3`이다.
- 시드 시나리오 5개(`duplicate`, `better`, `new`, `conflict`, `injection`)와 라벨 있는 60건 starter 코퍼스(`eval-60`, 다섯 유형 × 열두 사이트·작업 맥락)를 썼다.
- 어댑터에 추론 강도 옵션이 생기기 전의 비교는 loopback 전달기로 했다. 전달기는 요청에 `reasoning.effort`만 넣어 Meta로 보냈고, 실제 키는 전달기 프로세스에만 있었으며 `wam-curate`에는 더미 키를 줬다. 전달기는 토큰 수만 기록했고(프롬프트·응답·키 없음) 실험 뒤 종료해 삭제했다. 옵션을 구현한 뒤에는 `WAM_LLM_REASONING_EFFORT=low`로 직접 실행해 같은 결과를 다시 확인했다.
- 비용은 API가 돌려준 `usage` 토큰에 콘솔 정가(100만 토큰당 입력 1.25, 캐시 입력 0.15, 출력 4.25달러)를 곱했다. 콘솔이 단위를 표기하지 않아 100만 토큰당으로 해석했다.

### 4.2 시드 시나리오: 모델 비교 (기본 추론 강도)

| 모델 | 시드 통과 | 모델 호출 | 쓸 수 없는 응답 |
|---|---|---|---|
| `muse-spark-1.3` | 5/5 | 24 | 2 (출력 텍스트 없음) |
| `muse-spark-1.2` | 5/5 | 25 | 3 |
| `muse-spark-1.1` | 3/5 (`better`, `injection`이 판정 없이 종료) | 29 | 9 |

아래는 첫 번째 실행이고, `muse-spark-1.3`의 기본 강도 두 번째 실행은 4.3절에 있다(역시 5/5). 1.1의 두 실패는 "도구 호출이 아닌 응답이 너무 많다"(`NoDecision`)로 끝났다.

### 4.3 추론 강도 비교 (`muse-spark-1.3`, 시드 5개 = job 5건)

모든 실행이 5/5였다.

| 강도 | 실행 | 호출/실행 | 입력 토큰/실행 | 출력 토큰/실행 (추론) | 캐시 입력 비율 | 초/호출 | job당 비용 |
|---|---|---|---|---|---|---|---|
| 기본 | 1 | 22.0 | 125,728 | 14,677 (13,138) | 29% | 3.6 | $0.036 |
| `low` | 3 | 20.7 | 117,336 | 4,380 (3,109) | 43% | 1.8 | $0.022 |
| `minimal` | 3 | 20.3 | 115,130 | 2,405 (1,086) | 33% | 1.5 | $0.023 |

호출당 숨은 추론 토큰의 분포:

| 강도 | 호출 수 | 평균 | 중앙값 | p95 | 최대 | 1,500 이상 | 불완전(incomplete) |
|---|---|---|---|---|---|---|---|
| 기본 | 22 | 597 | 184 | 1,793 | 2,046 | 3 | 1 |
| `low` | 62 | 150 | 26 | 561 | 977 | 0 | 0 |
| `minimal` | 61 | 53 | 8 | 209 | 304 | 0 | 0 |

옵션을 구현한 뒤 `WAM_LLM_REASONING_EFFORT=low`로 직접 실행한 시드도 5/5였다(시나리오별 호출 수 5, 4, 3, 4, 2). `.env`만으로(플래그 없이) 돌린 `new`와 `injection`도 통과했다.

### 4.4 `eval-60` (`muse-spark-1.3`, `low`, 1회): 58/60

| 유형 | 통과 | 평균 호출 |
|---|---|---|
| duplicate | 11/12 | 5.2 |
| better | 12/12 | 5.0 |
| new | 11/12 | 3.0 |
| conflict | 12/12 | 4.1 |
| injection | 12/12 | 3.6 |

놓친 2건은 `new-blogger`와 `duplicate-notion`이고, 둘 다 `needs_human_review`로 에스컬레이션했다. 잘못 게시·교체·폐기한 사례는 없다. 이 코퍼스는 템플릿에서 파생된 회귀용이며 독립 라벨이 아니다(CUR-15 품질 게이트가 아님).

### 4.5 API 동작 탐침

| 요청 | 결과 |
|---|---|
| `max_output_tokens` 256, 기본 강도, 사소한 프롬프트 | HTTP 200이지만 `status: incomplete`. `output`에 `reasoning`만 있고 추론이 254/256 토큰이라 파서가 뽑은 텍스트가 비었다 |
| `max_output_tokens` 2048, 기본·`low`·`minimal` | 모두 `completed`. 추론 183·108·21 토큰 (1.2는 189, 1.1은 212) |
| `max_output_tokens` 512, `minimal`·`medium`·`high`·`xhigh` | 모두 `completed` (추론 33·277·225·107 토큰) |
| `max_output_tokens` 512, `low` | `incomplete` (추론 510 토큰): 강도는 상한이 아니라 경향이다 |
| `reasoning.effort` = `none`, 존재하지 않는 값 | 400. 허용 값은 `minimal`, `low`, `medium`, `high`, `xhigh`, `max` |
| `muse-wonder-1.0` | 404 `model_not_found` (우리 키에서 사용 불가) |
| `GET /v1/models` | 8개: `muse-spark-1.3`, `1.2`, `1.1`, `muse-spark-1.3-contributor`, `muse-spark-1.2-contributor`, `muse-image-1.0`, `muse-voice-transcribe-1.0`, `sam-3.1` |

호출마다 추론 길이 편차가 커서 출력 예산 2048에는 여유가 필요하다. `low`에서 관측한 최대는 977이다.

### 4.6 비용 보고 (실제 응답)

가격표(`WAM_LLM_PRICE_*`)를 설정한 `wam-curate --scenario new`(3회 호출)가 실제 Meta 응답의 `usage`로 계산한 `model cost reported by the provider: $0.0062`를 출력했다. 앞선 평균(job당 $0.022)보다 낮은데, 프롬프트 캐시가 잘 맞은 한 번의 표본으로 보인다. 평가 전체의 사용량은 정가 기준 $4 미만이었고 무료 크레딧으로 처리됐다.

## 5. 운영 명령 스모크 (`wam-worker`, 네트워크 없음)

| 실행 | 결과 |
|---|---|
| `--budget-status` | 상태 JSON (`open`, `blocked_by`, `until`, `strikes`, `spent_today_usd`, `daily_cap_usd`, `waiting_jobs`) |
| `WAM_LLM_DAILY_BUDGET_USD=2.5 --budget-status` | `daily_cap_usd: 2.5` |
| `--resume-budget --budget-status` | `model budget reopened; 0 parked job(s) released` 뒤 상태 JSON |
| `WAM_LLM_DAILY_BUDGET_USD=banana`, `WAM_LLM_QUOTA_PAUSE_SECS=5` | 설정 오류 메시지, 종료 코드 1 |
| 알 수 없는 인자 | 사용법 출력, 종료 코드 1 |
| 상한 설정 + 가격표 없는 `muse` provider, `--once` | 비용을 보고하는 provider가 필요하다는 메시지로 시작 거부, 종료 코드 1 |
| 상한 + 가격표 세 개, `--once` | 시작해서 할 일이 없으면 종료 코드 0 (모델 호출 없음) |
| 가격표 일부만 설정, `--once` | `model provider configuration is invalid`, 종료 코드 1 |

## 6. 검증 중 발견한 문제

1. **PostgreSQL이 `waiting_budget`을 거부했다.** `wam_work_items.state`의 CHECK 제약에 이 값이 없어서, 첫 크레딧 소진에서 worker가 `Unavailable`로 실패했다. 인메모리·파일 저장소는 이 제약이 없어 통과했고 disposable PostgreSQL 테스트만 잡아냈다. migration `0003`과 업그레이드 테스트(`QUE-24`)로 해결했다.
2. **숨은 추론이 출력 예산을 다 쓸 수 있다.** 기본 강도에서 호출의 일부가 2048 토큰을 추론에 쓰고 텍스트 없이 끝났다(4.3절, 4.5절). `WAM_LLM_REASONING_EFFORT`와 불완전 응답 구분으로 해결했다(#151).
3. **정확 중복은 모델을 부르지 않는다.** 일일 상한 테스트를 처음 쓸 때 같은 경로의 두 번째 기여가 모델 호출 0회로 끝나 기대와 달랐다. 규칙 처리가 의도대로 동작하는 것이어서, 경로를 다르게 한 테스트와 비용 0을 고정하는 테스트(`an_exact_duplicate_is_settled_without_the_model_so_it_costs_nothing`)로 나눴다.
4. **provider 장애는 시도를 소모했다. 해결(#158).** 429·5xx·시간 초과 같은 일시 장애가 약 2~3분 이어지면 대기 job이 수동 검토로 넘어갈 수 있었다. 코드 확인으로 알아냈고 재현은 하지 않았다. 스펙과 기존 테스트가 일부러 그렇게 정의한 동작이라 처음에는 이슈로만 남겼고, 이후 일시 장애를 소진과 같은 게이트로 기다리게 했다. 명세·테스트 계획·실행 결과는 [`provider-outage.md`](provider-outage.md)와 [`../spec/provider-outage.md`](../spec/provider-outage.md)에 있다. 그 과정에서 시도를 다 써서 검토로 넘어간 job이 `review_required` 대신 `worker_retry` 이벤트만 남기던 것도 찾아 고쳤다.

## 7. 재현 방법

```sh
# 자동 테스트: 네트워크도 모델도 쓰지 않는다
cargo test --locked -p wam-gatekeeper -p wam-server
cargo clippy --locked -p wam-gatekeeper -p wam-server --all-targets -- -D warnings

# PostgreSQL 테스트: 일회용 클러스터를 만들어 쓰고 지운다
PGROOT=$(mktemp -d /tmp/wam-pg-XXXXXX); PORT=55432
initdb -D "$PGROOT/data" -U wam_test --auth=trust -E UTF8 --locale=C
pg_ctl -D "$PGROOT/data" -o "-p $PORT -c listen_addresses=127.0.0.1 -c unix_socket_directories=$PGROOT" -l "$PGROOT/server.log" -w start
createdb -h 127.0.0.1 -p $PORT -U wam_test wam_test
export WAM_TEST_DATABASE_URL="postgres://wam_test@127.0.0.1:$PORT/wam_test"
cargo test -p wam-server --test postgres_application -- --ignored
pg_ctl -D "$PGROOT/data" -m fast -w stop; rm -rf "$PGROOT"

# 실제 호출: 비용이 든다
set -a; . ./.env; set +a    # MODEL_API_KEY, WAM_LLM_PROVIDER=muse, WAM_LLM_MODEL=muse-spark-1.3,
                            # WAM_LLM_REASONING_EFFORT=low, WAM_LLM_PRICE_* (git에서 제외됨)
cargo run -p wam-gatekeeper --bin wam-curate -- --scenario all       # 시드 5개, 약 $0.11
cargo run -p wam-gatekeeper --bin wam-curate -- --scenario eval-60   # 60건, 약 $1.3
```

## 8. 검증하지 못한 것

- **Meta의 실제 429·503 응답.** 장애 대기는 문서화된 형식을 재현한 fixture로만 검증했다. 자세한 한계는 [`provider-outage.md`](provider-outage.md)의 "검증하지 않는 것"에 있다.
- **Meta의 실제 소진 응답.** 크레딧을 0까지 쓰지 않았다. 402 본문은 [dev.meta.ai/docs/error-handling](https://dev.meta.ai/docs/error-handling)에 나온 형식을 재현한 fixture로만 검증했다.
- **서버 worker로 실제 Meta를 끝까지 돌리는 것.** 실제 호출은 같은 `run_job` 경로를 쓰는 `wam-curate`로 했고, `process_next_budgeted`는 스크립트 provider로 검증했다.
- **worker 프로세스 둘이 상한 근처에서 동시에 경쟁하는 경우.** 마지막 job이 상한을 넘을 수 있고 프로세스마다 한 건씩 더 넘을 수 있다는 것은 설계 설명이며 측정하지 않았다. 두 connection이 같은 게이트를 읽는 것만 검증했다.
- **migration 뒤 이전 binary로 되돌리는 절차.** `--resume-budget`으로 대기 job을 `queued`로 돌려놓는 방법을 문서화했지만 실행해 보지 않았다.
- **모델 품질의 통계적 의미.** `eval-60`은 1회 실행이고 템플릿에서 파생됐다. 기본·`minimal` 강도는 `eval-60`을 돌리지 않았고, 실행마다 결과가 달라질 수 있다.
- **실제 청구와 약관.** 계정이 무료 크레딧이라 청구서는 확인하지 못했고 콘솔 집계는 최대 1시간 늦다. 무료 크레딧 사용 중 콘텐츠가 제품 개선에 쓰일 수 있다는 콘솔 표시가 유료 전환 뒤 어떻게 바뀌는지도 확인하지 못했다.
- **Muse Wonder, 오픈 웨이트 모델, contributor 등급 모델.** 평가하지 않았다. contributor 등급은 입력·출력이 학습에 쓰이므로 의도적으로 쓰지 않았다.
