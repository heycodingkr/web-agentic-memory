# OUTAGE: 모델 provider 일시 장애 대기

상태: 구현됨(2026-11-01, 이슈 #158). 키·모델 설정을 거부하는 오류(401, 403, 404)를 같은 게이트로 기다리게 하는 확장(이슈 #168, R11~R14, `F-OUT-13~15`)은 명세만 쓴 단계이고 아직 구현하지 않았다. 검증할 항목, 합격 기준과 실행 결과는 [`../test/provider-outage.md`](../test/provider-outage.md)에 있고, 이 문서는 무엇을 만들었는지 정한다. 명세를 먼저 쓰고 구현해서 둘이 어긋난 곳은 아래 "명세와 달라진 점"에 적었다.

## 문제

서버 worker(`wam-server`의 `durable/worker.rs`)는 provider가 크레딧·쿼터 소진을 알릴 때만 job을 시도 소모 없이 기다리게 한다([`queue.md`](queue.md)의 크레딧 소진 절). 그 밖의 provider 실패는 전부 일반 실패다. claim마다 시도가 1씩 늘고, 재시도 간격은 10, 20, 40, 80초이며, 다섯 번째 시도가 실패하면 job은 `waiting_review`(수동 검토)로 간다. 분당 한도(429), 과부하·점검(503, `Retry-After` 60초), 시간 초과가 2~3분만 이어져도 그 사이 시도된 job은 사람에게 넘어간다. job은 서로 독립적으로 시도를 쌓으므로 대기열 전체가 같은 장애에 한꺼번에 당하고, 장애 중에도 job마다 다섯 번씩 provider를 두드린다. 코드를 읽어서 알아낸 것이고 재현은 하지 않았다.

키·설정 오류도 같은 구조에 걸린다(이슈 #168). API 키가 폐기·교체되었거나 키에 그 모델의 권한이 없거나 모델 이름·주소가 틀리면 provider는 401, 403, 404로 답하고, 이것도 일반 실패라서 대기열의 모든 job이 같은 오류로 시도를 쓰다가 약 2분 30초(10+20+40+80초) 안에 `waiting_review`로 간다. 키나 설정을 고쳐야 낫는 문제이고 job의 잘못이 아니라는 점은 장애와 같다. 다른 점은 둘이다. 저절로 낫지 않으므로 기다리는 시간이 길어야 하고, 사람이 고쳐야 하므로 사람이 알아야 한다. 이것도 코드를 읽어서 알아낸 것이고 재현은 하지 않았다.

장애는 job의 잘못이 아니다. 이 명세는 장애를 크레딧 소진처럼 **기다림**으로 다룬다. 다만 장애를 일으키는 job 하나가 대기열을 영원히 막거나 영원히 숨지는 못하게 한다.

## 목표와 범위 밖

목표

- 일시 장애를 만난 job은 시도를 쓰지 않고 기다린다. 장애가 몇 분이든 job이 수동 검토로 넘어가지 않는다(job마다 둔 한도 안에서).
- 장애 중 모델 호출은 job마다 다섯 번이 아니라 닫힌 시간마다 한 번의 탐침으로 줄어든다.
- provider가 다시 답하면 아무 조작 없이 이어서 처리한다.
- 한 job이 장애를 일으켜도 다른 job을 막지 못하고, 그 job도 한도 뒤에는 수동 검토로 간다.
- 운영자가 상태를 보고, 즉시 재개하고, 이전 동작으로 되돌릴 수 있다.
- 키·모델 설정을 거부하는 오류(401, 403, 404)를 만난 job도 시도를 쓰지 않고 기다린다. 장애보다 길게(기본 1시간부터) 기다리고, 운영자가 무엇을 고칠지 알려 준다.
- 장애는 카탈로그·점수·공개 상태를 바꾸지 않고, 재개 뒤 같은 기여는 정확히 한 번만 게시된다.

범위 밖 (이번에 바꾸지 않는다)

- 요청 오류(HTTP 400, 413, 422). 요청 내용이 원인일 수 있고 기다려도 낫지 않아서 계속 일반 실패로 둔다(401, 403, 404는 R11~R14가 다룬다).
- `wam-gatekeeper`의 독립 worker engine(`worker/engine.rs`)은 일시 장애를 지금처럼 일반 실패로 재시도한다. 이 문서는 애플리케이션 worker(서버)만 바꾼다.
- CLI provider(`claude-cli`, `codex-cli`)는 이 분류를 만들지 않는다. 개발용이다.
- 여러 worker가 같은 창에서 동시에 탐침하는 것은 막지 않는다. worker가 W개면 한 창에 최대 W건이 나간다.
- Meta의 실제 401·403·404·429·503 응답은 관찰하지 못했다. Meta 문서(dev.meta.ai/docs/error-handling)가 적은 형식을 재현한 fixture로 검증한다. 401(`authentication_error`, `invalid_api_key`)과 404(`model_not_found`)는 문서에 본문 예가 있고, 403은 본문 예가 없어서 본문 없음·HTML·JSON 세 형태로 상태 코드만으로 분류함을 보인다.

## 분류

HTTP provider(`OpenAiCompatProvider`)가 응답을 아래처럼 나눈다. 우선순위는 위에서 아래다.

| provider가 한 것 | 분류 | job 처리 |
|---|---|---|
| HTTP 402, `insufficient_quota`, 소진 문구 (429 포함) | 소진 (`QuotaExhausted`, 기존) | `waiting_budget`, 시도 미소모, 소진 게이트 |
| HTTP 429(소진 신호 없음), 408, 모든 5xx | **일시 장애** (`Unavailable`, 새로 추가) | `waiting_budget`, 시도 미소모(한도 안에서), 장애 게이트 |
| 연결 실패(DNS, 거부, TLS), 응답 전 끊김, 시간 초과(요청 180초), 본문 수신 중단 | **일시 장애** | 위와 같음 |
| HTTP 401, 403, 404 (소진 신호가 없을 때) | **거부** (`Rejected`, 새로 추가) | `waiting_budget`, 시도 미소모(한도 안에서), 거부 게이트(더 길게) |
| 그 밖의 4xx(400, 413, 422 등), 2xx인데 JSON이 아님·크기 초과 | 일반 실패 (`Failed`, 기존) | 시도 소모, 백오프, 다섯 번째 실패에서 `waiting_review` |
| 답했지만 결정에 이르지 못함(출력이 도구 호출이 아님, 턴 한도, 거부된 도구 호출) | 일반 실패 (`NoDecision`, 기존) | 위와 같다. 게이트는 이것을 "답했다"로 보고 연속 장애를 끝낸다 |

오류 문자열은 코드가 만든 고정 문구(`HTTP 503 from model provider`, `HTTP 401 from model provider`, `HTTP model provider is unreachable`, `HTTP model provider timed out`)이며 provider가 보낸 본문·헤더·키를 옮기지 않는다. `Retry-After`는 초 또는 HTTP 날짜로 읽고, 읽을 수 없으면 없는 것으로 본다. 오류 응답의 본문은 JSON이 아니어도 된다(502 HTML 등). 상태 코드가 먼저 분류하고, 거부는 본문(`authentication_error`, `model_not_found` 등)을 읽지 않는다. 다만 소진 신호는 상태 코드보다 우선한다(소진 문구가 있는 403은 소진이다). 거부는 `Retry-After`를 읽지 않는다: 기다리는 시간은 R12가 정한다.

curator는 `ProviderError::Unavailable { why, retry_after_secs }`를 `Outcome::Unavailable { why, retry_after_secs }`로 끝낸다. 결정이 없고 저장소는 바뀌지 않으며 감사 기록의 outcome은 `provider_unavailable: <why>`다. 그 전에 모델이 이미 답한 턴의 비용은 `JobReport`에 남는다.

`ProviderError::Rejected { why }`는 `Outcome::ProviderRejected { why }`로 끝난다. 결정이 없고 저장소는 바뀌지 않으며 감사 기록의 outcome은 `provider_rejected: <why>`다. 문자열로는 `the model provider rejected the credentials or the model setting: <why>`다.

## 동작 규칙

**R1. 대기.** 서버 worker가 `Outcome::Unavailable`을 받으면, 같은 transaction에서 lease(generation, holder)를 확인한 뒤 job을 `waiting_budget`으로 옮기고 holder·lease를 풀고 claim에서 올린 시도를 되돌리고(시도 미소모) `not_before`를 게이트가 다시 열리는 시각으로 둔다. 이벤트 `worker_budget_wait`(target은 기여 ID)를 남기고 `WorkResult::Retry`를 돌려준다. 접수한 기여의 상태는 계속 `queued`다. 장애 전에 모델이 이미 답한 턴의 비용은 일일 합계에 더한다. 결정·카탈로그·점수·감사는 바뀌지 않으며 job은 다음 claim에서 처음부터 다시 판단한다. 이 대기는 R5의 한도 안에서만 시도를 쓰지 않는다.

**R2. 게이트.** 같은 transaction에서 모델 예산 게이트([`queue.md`](queue.md)의 크레딧 소진 절과 같은 게이트)를 닫는다. 닫힌 이유는 `provider_outage`다. 닫힌 동안 어떤 worker도 job을 claim하지 않고 browser replay도 돌리지 않고 모델을 부르지 않는다(`Idle`). 그 사이 접수된 기여는 `queued`로 남는다. 시간이 지나 열리면 job 하나가 탐침으로 나간다(replay 1회와 모델 호출 1회). 성공하면 계속 처리하고 다시 장애면 게이트가 더 오래 닫힌다. recall은 이 게이트에 영향을 받지 않는다.

**R3. 대기 시간.** 게이트가 닫히는 시간은 `max(provider가 알린 Retry-After, 백오프)`이고 최소 30초다. 백오프는 `WAM_LLM_OUTAGE_PAUSE_SECS`(기본 30)에 연속 장애 횟수마다 두 배를 곱한 값이고 최대 `WAM_LLM_OUTAGE_PAUSE_MAX_SECS`(기본 600)다. provider가 알린 시간은 `WAM_LLM_QUOTA_PAUSE_MAX_SECS`(기본 3600)까지 따른다. 연속 장애는 답이 한 번도 없이 이어진 장애의 수이고 소진의 연속 거절 수와 따로 센다. 이미 정해진 더 늦은 재개 시각을 짧게 하지 않는다. 두 이유(소진, 장애)의 pause가 겹치면 더 늦게 끝나는 쪽이 닫힌 이유다.

| 연속 장애 | 1 | 2 | 3 | 4 | 5 | 6 | 7~ |
|---|---|---|---|---|---|---|---|
| 알린 시간이 없을 때 닫히는 초 | 30 | 60 | 120 | 240 | 480 | 600 | 600 |

**R4. 회복.** provider가 답하면(결정이든 아니든) 연속 장애 횟수, 연속 소진 횟수와 그 job의 장애 대기 횟수가 0으로 돌아가고 게이트가 열린다. 게이트가 닫힐 때마다 `worker_budget_paused`, 열릴 때마다 `worker_budget_resumed`(target `provider_outage`)가 한 번씩 남아서 회복 뒤에는 두 횟수가 같다. 대기하던 job은 R6의 순서로 처리된다. 장애를 만났던 job은 만나지 않은 job 뒤에 선다.

**R5. 장애 대기 한도.** job마다 장애 때문에 시도 없이 기다릴 수 있는 횟수는 `WAM_LLM_OUTAGE_FREE_WAITS`(기본 8)다. 한도를 다 쓴 job이 또 일시 장애를 만나면 그 시도는 일반 실패처럼 쓰인다: 시도가 늘고 `retry` 백오프가 붙고 다섯 번째에서 `waiting_review`로 가고, 이때 `review_required` 이벤트가 남는다(시도를 다 쓴 일반 실패가 `worker_retry`로만 기록되던 것도 함께 고쳤다). 게이트는 이때도 닫힌다(장애는 모두에게 장애다). 0이면 장애를 처음부터 일반 실패로 센다(이전 동작). 소진 대기는 이 한도에 포함하지 않으며 한도 없이 기다린다(크레딧이 며칠 비어도 job은 남는다).

**R6. 순서와 순환.** claim 순서는 장애 대기 횟수가 적은 job이 먼저이고, 같으면 먼저 접수된 job이다(`(대기 횟수, 접수 시각, id)`). 장애가 없으면 모든 횟수가 0이라 지금과 같은 선입선출이다. 따라서 (a) provider 전체 장애에서는 탐침이 job들 사이를 돌아가며 나가서 한 job의 한도가 먼저 바닥나지 않고, (b) 한 job만 장애를 일으키면 그 job은 횟수가 늘수록 뒤로 밀려 다른 job이 먼저 처리되며, 마지막에는 R5로 수동 검토에 간다.

**R7. 철회·재시작·운영자.** `waiting_budget`인 job의 동의 철회는 `canceled`로 만들고 모델에 가지 않는다. 게이트와 job별 장애 대기 횟수는 애플리케이션 상태에 저장되므로 재시작하거나 worker가 여럿이어도 같다. `wam-worker --resume-budget`은 장애 pause도 풀고 `waiting_budget` job을 `queued`로 되돌리며 연속 횟수와 장애 대기 횟수를 0으로 한다(일일 상한은 풀지 않는다).

**R8. 노출.** `wam-worker --budget-status`의 `blocked_by`는 `provider_outage`, `provider_rejected`(R13), `provider_quota`, `daily_cap` 중 하나다. `outage_strikes`(연속 장애 수)와 `reason`(마지막 장애의 고정 문구, 80자 이하, `A-Z a-z 0-9`와 공백 `._:()/-` 밖의 문자는 제거)을 더한다. 게이트가 닫혔다는 시작 알림은 이유에 맞게 말한다: 장애는 "worker가 스스로 다시 시도한다, `--resume-budget`으로 지금 시도할 수 있다", 소진은 "크레딧을 충전한 뒤 `--resume-budget`".

**R9. 비밀과 내용.** 상태, 이벤트, `--budget-status`, 오류 어디에도 provider 응답 본문, 헤더, API key가 나타나지 않는다.

**R10. 호환성.** 스키마 변경과 migration이 없다. job 상태는 기존 `waiting_budget`이고 새 게이트 필드는 모두 읽을 때 기본값이 있어(`serde(default)`) 이전 릴리스가 쓴 상태를 열린 게이트로 읽는다. 이전 릴리스로 되돌려도 새 필드는 무시되고 `waiting_budget` job은 `not_before` 뒤에 다시 claim된다. migration `0003` 이전으로 되돌릴 때의 절차는 [`../operations/deployment.md`](../operations/deployment.md)에 있는 그대로다.

다음 R11~R14는 키·모델 설정 거부(401, 403, 404, 이슈 #168)를 다룬다. R1~R10의 장치(게이트, job별 한도, 탐침 순환, 운영 명령)를 그대로 쓰고 다른 점만 적는다.

**R11. 거부 대기.** 서버 worker가 `Outcome::ProviderRejected`를 받으면 R1과 같이 같은 transaction에서 lease를 확인하고 job을 `waiting_budget`으로 옮기며 시도를 되돌리고(시도 미소모) `not_before`를 게이트가 열리는 시각으로 둔다. 이벤트는 `worker_budget_wait`다. 같은 게이트를 닫고 닫힌 이유는 `provider_rejected`다(R2: 닫힌 동안 claim·replay·모델 호출이 없고, 열리면 탐침 1건). job의 시도 없는 대기 횟수는 장애와 거부를 합쳐서 센다: job마다 둘을 합해 `WAM_LLM_OUTAGE_FREE_WAITS`번까지 시도 없이 기다린다(R5). 한도를 다 쓴 job이 또 거부를 만나면 일반 실패처럼 쓰인다: 시도가 늘고 `retry` 백오프가 붙고 다섯 번째에서 `waiting_review`로 가며 `review_required`가 남는다. 게이트는 이때도 닫힌다. claim 순서(R6)도 합친 횟수를 따른다. 0이면 거부를 처음부터 일반 실패로 센다(이전 동작).

**R12. 거부의 대기 시간.** 게이트가 닫히는 시간은 `WAM_LLM_REJECTED_PAUSE_SECS`(기본 3600)에 연속 거부 횟수마다 두 배를 곱한 값이고 최대 `WAM_LLM_REJECTED_PAUSE_MAX_SECS`(기본 21600)다. 연속 거부는 답이 한 번도 없이 이어진 거부의 수이고 연속 장애·소진과 따로 센다. 이미 정해진 더 늦은 재개 시각을 짧게 하지 않고, 이유가 겹치면 더 늦게 끝나는 쪽이 닫힌 이유다(R3).

| 연속 거부 | 1 | 2 | 3 | 4~ |
|---|---|---|---|---|
| 닫히는 초 | 3600 | 7200 | 14400 | 21600 |

**R13. 회복과 운영자.** provider가 답하면(결정이든 아니든) R4처럼 연속 거부 횟수와 그 job의 대기 횟수가 0으로 돌아가고 게이트가 열린다. 사람이 키나 모델 설정을 고치면 다음 탐침이 저절로 알아차린다. 기다리지 않으려면 `wam-worker --resume-budget`이 거부 pause도 풀고 연속 거부 횟수를 0으로 한다. worker를 다시 시작해도 거부 pause는 열리지 않는다: 상태가 저장되어 있고, `--once`를 주기적으로 부르는 배포에서 시작마다 열면 replay가 그 주기마다 돈다. 운영자가 알게 하는 장치는 셋이다.

- `wam-worker --budget-status`의 `blocked_by`는 `provider_rejected`이고 `rejected_strikes`(연속 거부 수)와 `reason`(마지막 거부의 고정 문구. 예 `HTTP 401 from model provider`, R8의 정제 규칙)을 보여 준다.
- 게이트가 닫혔다는 로그 한 줄은 이유를 말하고 무엇을 고칠지 알려 준다: 키(`MODEL_API_KEY`), 모델 이름(`WAM_LLM_MODEL`), 주소(`WAM_LLM_BASE_URL`)를 확인해 고친 뒤 `wam-worker --resume-budget`.
- `queue_health.alert`는 거부 pause가 도는 동안 true다. 기존 기준(활성 job 100건 이상 또는 가장 오래된 job이 1시간 초과)은 첫 pause가 끝나야 켜지므로 사람이 고쳐야 하는 문제를 그만큼 늦게 알린다. 장애·소진·일일 상한은 저절로 풀리므로 이 기준을 바꾸지 않는다.

**R14. 호환성.** R10을 그대로 지킨다. 스키마·migration 변경이 없고 job 상태는 기존 `waiting_budget`이다. 새 게이트 필드는 `rejected_strikes`(연속 거부 수)와 `pause_rejected`(지금 도는 pause가 거부로 정해졌는가)이고 둘 다 읽을 때 기본값이 있다(`serde(default)`). 거부 pause는 `pause_cause`에 기존 값 `outage`를 쓰고 `pause_rejected`를 켠다. 새 필드를 모르는 이전 릴리스는 이 pause를 장애 pause로 읽고 `paused_until`이 지나면 job을 다시 claim한다. `pause_cause`에 새 값을 쓰지 않는 까닭은 이전 릴리스가 모르는 값을 만나면 상태 문서 전체를 읽지 못하기 때문이다.

## job 상태 전이 (서버 worker)

| 현재 | 사건 | 다음 | 시도 |
|---|---|---|---|
| `queued`, `retry`, `waiting_budget` | claim(게이트가 열려 있고 `not_before` 지남) | `leased` | +1 |
| `leased` | 일시 장애, 장애 대기가 남음(R5) | `waiting_budget` | 되돌림(±0) |
| `leased` | 일시 장애, 장애 대기를 다 씀 | `retry`, 시도가 5이면 `waiting_review` | +1 유지 |
| `leased` | 거부(401, 403, 404), 대기가 남음(R11, 장애와 합친 한도) | `waiting_budget` | 되돌림(±0) |
| `leased` | 거부, 대기를 다 씀 | `retry`, 시도가 5이면 `waiting_review` | +1 유지 |
| `leased` | 소진(기존) | `waiting_budget` | 되돌림(±0) |
| `leased` | 일반 실패, 결정 못함(기존) | `retry`, 시도가 5이면 `waiting_review` | +1 유지 |
| `waiting_budget` | 동의 철회 | `canceled` | |
| `waiting_budget` | `--resume-budget` | `queued` | |

## 설정

| 이름 | 기본 | 범위 | 뜻 |
|---|---|---|---|
| `WAM_LLM_OUTAGE_PAUSE_SECS` | 30 | 30~86400 | 일시 장애가 시간을 알리지 않을 때 처음 닫는 시간. 연속 장애마다 두 배 |
| `WAM_LLM_OUTAGE_PAUSE_MAX_SECS` | 600 | 위 값~86400 | 백오프의 상한 |
| `WAM_LLM_OUTAGE_FREE_WAITS` | 8 | 0~1000 | job마다 장애와 거부로 시도 없이 기다릴 수 있는 횟수(합산). 0이면 이전 동작 |
| `WAM_LLM_REJECTED_PAUSE_SECS` | 3600 | 30~86400 | 키·모델 설정 거부 뒤 처음 닫는 시간. 연속 거부마다 두 배 |
| `WAM_LLM_REJECTED_PAUSE_MAX_SECS` | 21600 | 위 값~86400 | 거부 pause의 상한 |

비어 있는 값은 기본값이다. 읽을 수 없거나 범위 밖이면 worker가 시작하지 않는다(소진·일일 상한 설정과 같은 방식이고 오류에 값을 싣지 않는다). 기본값이면 장애가 약 45분(30+60+120+240+480+600+600+600초) 이어질 때까지 한 job은 시도를 쓰지 않는다. 그 뒤 job은 일반 실패처럼 다섯 번 더 쓰인다. 같은 장애를 여러 job이 겪으면 R6의 순환 때문에 이 시간이 job 수만큼 늘어난다. 거부는 기본값이면 하나뿐인 job이 약 37시간(1+2+4+6×5시간) 시도를 쓰지 않고 그 뒤 일반 실패처럼 쓰인다. 사람이 그 안에 고치지 않으면 job은 결국 수동 검토로 간다.

## 결정 기록

| 질문 | 선택 | 이유와 버린 안 |
|---|---|---|
| 새 job 상태(`waiting_provider`)를 만들까 | 아니오, `waiting_budget`과 같은 게이트를 쓴다 | 새 상태는 PostgreSQL CHECK 제약 migration, 큐 지표, 운영 명령을 모두 늘린다. 게이트는 이미 닫힌 이유·재개 시각·운영 명령·재시작 보존을 갖췄다. 이유는 `blocked_by`로 구분한다 |
| 장애를 job 단위로 세지 말고 worker 단위로만 셀까 (#158 제안 2) | 둘 다 한다 | 게이트(전역)가 호출을 줄이고, job별 한도(R5)가 장애를 일으키는 job이 큐를 영원히 막지 못하게 한다. 전역만 쓰면 그 job이 가장 오래된 job으로서 매 탐침을 독차지한다 |
| 한도 뒤에는 어떻게 할까 | 일반 실패처럼 센다 | 새 종착 상태가 필요 없고 기존 `waiting_review` 경로와 운영 절차를 그대로 쓴다 |
| 순서를 바꿔도 되나 | 장애 대기가 적은 job 먼저, 같으면 선입선출 | 장애가 없으면 순서가 변하지 않는다. 소진 대기는 세지 않아서 크레딧이 돌아온 뒤 순서도 변하지 않는다. 한 job이 탐침을 독차지하는 문제(위)를 새 필드 없이 푼다 |
| 장애 대기 횟수를 어디에 저장하나 | 게이트 문서 안의 job별 맵 | `Work` 구조와 PostgreSQL 행을 바꾸지 않는다(migration 없음). 끝난 job의 항목은 다음 기록 때 지운다 |
| 인증·권한·모델 이름 오류(401, 403, 404)도 기다릴까 | 이슈 #158에서는 아니오, #168에서 예. 400, 413, 422는 계속 아니오 | #158에서는 키·설정을 고치면 낫는 오류와 job 내용 때문인 오류를 가를 근거가 없었다. Meta 문서가 근거를 준다: 401은 키 누락·폐기·오타, 403은 키에 그 모델·기능의 권한이 없음, 404는 없는 모델·잘못된 경로이고 모두 요청 내용과 무관하다. 400, 413, 422는 요청 내용이 원인일 수 있고 기다려도 낫지 않아서 job의 시도를 계속 쓴다 |
| 거부를 장애와 같은 규칙으로 기다릴까 | 같은 게이트와 같은 job 한도, 다른 이유와 더 긴 pause | 새 job 상태·새 운영 명령 없이 `--budget-status`, `--resume-budget`, 탐침 순환, job별 한도를 그대로 쓴다. 장애는 몇 분 안에 저절로 풀리지만 거부는 사람이 고칠 때까지 풀리지 않아서, 30초부터 두 배로 두드리면 키가 잘못된 동안 의미 없는 replay와 호출이 쌓인다. 그래서 1시간부터 시작한다 |
| 거부 대기 횟수를 장애와 따로 셀까 | 아니오, 합산 | 새 필드와 설정이 늘고 한 job이 장애와 거부로 번갈아 대기해도 둘 다 한도가 없는 셈이 된다. 합산이면 "이 job은 provider가 답하지 못해 기다린 횟수가 한도에 닿았다" 하나로 설명된다 |
| 거부 pause를 새 `PauseCause` 값으로 표시할까 | 아니오, `pause_cause = outage`에 `pause_rejected` 플래그 | 새 값은 이전 릴리스가 상태 문서 전체를 못 읽게 한다(R10, R14). 모르는 필드는 무시되므로 플래그는 안전하다. 대가로 "거부이면서 소진" 같은 조합이 표현상 가능해지지만 기록하는 곳이 한 함수라서 항상 같이 갱신한다 |
| worker를 다시 시작하면 거부 pause를 풀까 | 아니오 | 키를 바꾸고 재시작하는 것이 흔한 순서지만, 주기적으로 `--once`를 부르는 배포에서는 시작마다 탐침이 나가 replay가 그 주기마다 돈다. 대신 로그와 `--budget-status`가 `--resume-budget`을 알려 준다 |
| 알림은 어떻게 켤까 | 거부 pause 중에는 `queue_health.alert`를 즉시 true로 | 1시간이 넘은 job을 기준으로 하면 첫 pause가 끝나는 1시간 동안 아무도 모른다. `queue_health`의 모양(`active_jobs`, `oldest_age_secs`, `alert`)은 바꾸지 않는다 |

## 요구사항과 테스트

| ID | 요구사항 | 합격 기준 | 관련 테스트 |
|---|---|---|---|
| F-OUT-01 | HTTP provider 분류 | 분류 표대로 `Unavailable`/`QuotaExhausted`/`Failed`로 나뉘고, `Retry-After`(초·HTTP 날짜)를 읽으며, 본문·키를 옮기지 않는다 | MODEL-15, MODEL-16 |
| F-OUT-02 | curator 결과 | 일시 장애는 결정·저장소 변경 없이 `Outcome::Unavailable`로 끝나고 다른 분류는 그대로다 | MODEL-17 |
| F-OUT-03 | 시도 없는 대기 | 장애를 만난 job은 `waiting_budget`, 시도 미소모, 검토행 없음, 기여 `queued` | QUE-25 |
| F-OUT-04 | 전역 게이트 | 닫힌 동안 claim·replay·모델 호출 0, 열릴 때 탐침 1건 | QUE-26 |
| F-OUT-05 | 대기 시간 | R3 표와 `Retry-After` 규칙 | QUE-27, 게이트 단위 |
| F-OUT-06 | 회복과 순서 | 답이 오면 초기화·재개하고, 대기 job은 R6의 순서(장애를 덜 만난 job 먼저)로 한 번씩 게시 | QUE-28 |
| F-OUT-07 | 한도와 순환 | 한 job만 장애를 일으켜도 다른 job은 먼저 처리되고 그 job은 한도 뒤 검토행 | QUE-29, 게이트 단위 |
| F-OUT-08 | 기존 계약과 되돌리기 | 일반 실패는 그대로 시도를 쓰고 `WAM_LLM_OUTAGE_FREE_WAITS=0`은 이전 동작 | QUE-30 |
| F-OUT-09 | 지속성·철회·운영자 | R7 | QUE-31, QUE-33, 게이트 단위 |
| F-OUT-10 | 실제 HTTP 경로 | provider 코드와 worker를 loopback 서버로 이어서 장애와 회복을 거친다 | QUE-32 |
| F-OUT-11 | 노출과 비밀 | R8, R9 | QUE-32, 게이트 단위 |
| F-OUT-12 | 호환성 | 이전 릴리스의 게이트 문서를 열린 게이트로 읽고 PostgreSQL 스키마를 바꾸지 않는다 | QUE-33, 게이트 단위 |
| F-OUT-13 | 거부 분류 | 401, 403, 404가 소진 신호 없이 오면 `Rejected`, 400·413·422는 `Failed`, 소진 신호는 상태 코드보다 우선하고, 본문·키를 옮기지 않으며 `Retry-After`를 읽지 않는다. curator는 `Outcome::ProviderRejected`로 끝낸다 | MODEL-18, MODEL-19 |
| F-OUT-14 | 거부 대기 | 시도 없는 대기, 1시간부터 두 배(최대 6시간), 한도·순환은 장애와 합산, 회복·`--resume-budget`·한도 0 | QUE-34~37, 게이트 단위 |
| F-OUT-15 | 운영자 신호와 호환성 | `blocked_by`·`rejected_strikes`·`reason`, 닫힘 로그의 안내, `queue_health.alert`, 실제 HTTP 경로, 이전 릴리스와의 문서 호환 | QUE-38, QUE-39, 게이트 단위 |

## 명세와 달라진 점

구현하면서 명세를 고친 곳이다.

- **`review_required` 이벤트.** 명세는 한도를 다 쓴 job이 `waiting_review`로 가면 이 이벤트가 남는다고 적었지만, 기존 `park`는 호출자가 `waiting_review`를 요청했을 때만 이 이벤트를 남겼고 시도를 다 써서 검토로 넘어간 job에는 `worker_retry`만 남겼다. 운영자가 검토 대기를 알아채는 신호라서 job이 실제로 검토 상태가 될 때 남도록 고쳤다(`QUE-30`이 일반 실패에서도 지킨다).
- **회복 뒤 순서.** 명세 초안은 회복 뒤 "먼저 온 순서"로 처리한다고 적었다. R6의 순서(장애 대기가 적은 job 먼저)를 따르면 장애를 만났던 job이 뒤로 선다. 한 job이 탐침을 독차지하지 못하게 하는 대가이고, 장애를 만난 적이 없는 job끼리의 순서는 그대로 선입선출이다. 테스트와 R4를 이 동작에 맞췄다.
- **상태 필드와 로그.** 명세한 `outage_strikes`, `reason` 외에 `wam-worker`와 서버 안 worker의 닫힘 안내를 이유별로 나눴다(소진은 충전 뒤 `--resume-budget`, 장애는 스스로 다시 시도, 일일 상한은 다음 UTC 날짜).
