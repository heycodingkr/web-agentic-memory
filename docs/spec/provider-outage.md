# OUTAGE: 모델 provider 일시 장애 대기

상태: 명세(2026-11-01). 구현 전이다. 이슈 #158. 구현하면 이 줄과 [`../test/provider-outage.md`](../test/provider-outage.md)의 상태 열을 실행 결과로 바꾼다. 검증할 항목과 합격 기준은 위 테스트 명세에 있고, 이 문서는 무엇을 만들지 정한다.

## 문제

서버 worker(`wam-server`의 `durable/worker.rs`)는 provider가 크레딧·쿼터 소진을 알릴 때만 job을 시도 소모 없이 기다리게 한다([`queue.md`](queue.md)의 크레딧 소진 절). 그 밖의 provider 실패는 전부 일반 실패다. claim마다 시도가 1씩 늘고, 재시도 간격은 10, 20, 40, 80초이며, 다섯 번째 시도가 실패하면 job은 `waiting_review`(수동 검토)로 간다. 분당 한도(429), 과부하·점검(503, `Retry-After` 60초), 시간 초과가 2~3분만 이어져도 그 사이 시도된 job은 사람에게 넘어간다. job은 서로 독립적으로 시도를 쌓으므로 대기열 전체가 같은 장애에 한꺼번에 당하고, 장애 중에도 job마다 다섯 번씩 provider를 두드린다. 코드를 읽어서 알아낸 것이고 재현은 하지 않았다.

장애는 job의 잘못이 아니다. 이 명세는 장애를 크레딧 소진처럼 **기다림**으로 다룬다. 다만 장애를 일으키는 job 하나가 대기열을 영원히 막거나 영원히 숨지는 못하게 한다.

## 목표와 범위 밖

목표

- 일시 장애를 만난 job은 시도를 쓰지 않고 기다린다. 장애가 몇 분이든 job이 수동 검토로 넘어가지 않는다(job마다 둔 한도 안에서).
- 장애 중 모델 호출은 job마다 다섯 번이 아니라 닫힌 시간마다 한 번의 탐침으로 줄어든다.
- provider가 다시 답하면 아무 조작 없이 이어서 처리한다.
- 한 job이 장애를 일으켜도 다른 job을 막지 못하고, 그 job도 한도 뒤에는 수동 검토로 간다.
- 운영자가 상태를 보고, 즉시 재개하고, 이전 동작으로 되돌릴 수 있다.
- 장애는 카탈로그·점수·공개 상태를 바꾸지 않고, 재개 뒤 같은 기여는 정확히 한 번만 게시된다.

범위 밖 (이번에 바꾸지 않는다)

- 인증·권한 오류(HTTP 401, 403), 모델 이름 오류(404), 요청 오류(400, 413, 422). 기다려도 낫지 않거나(운영자가 키·설정을 고쳐야 한다) job 내용 때문일 수 있어서 일반 실패로 둔다. 키를 잘못 바꾸면 대기 job이 같은 방식으로 검토로 넘어가는 문제는 남는다. 후속 이슈로 다룬다.
- `wam-gatekeeper`의 독립 worker engine(`worker/engine.rs`)은 일시 장애를 지금처럼 일반 실패로 재시도한다. 이 문서는 애플리케이션 worker(서버)만 바꾼다.
- CLI provider(`claude-cli`, `codex-cli`)는 이 분류를 만들지 않는다. 개발용이다.
- 여러 worker가 같은 창에서 동시에 탐침하는 것은 막지 않는다. worker가 W개면 한 창에 최대 W건이 나간다.
- Meta의 실제 429·503 응답은 관찰하지 못했다. Meta 문서(dev.meta.ai/docs/error-handling)가 적은 형식을 재현한 fixture로 검증한다.

## 분류

HTTP provider(`OpenAiCompatProvider`)가 응답을 아래처럼 나눈다. 우선순위는 위에서 아래다.

| provider가 한 것 | 분류 | job 처리 |
|---|---|---|
| HTTP 402, `insufficient_quota`, 소진 문구 (429 포함) | 소진 (`QuotaExhausted`, 기존) | `waiting_budget`, 시도 미소모, 소진 게이트 |
| HTTP 429(소진 신호 없음), 408, 모든 5xx | **일시 장애** (`Unavailable`, 새로 추가) | `waiting_budget`, 시도 미소모(한도 안에서), 장애 게이트 |
| 연결 실패(DNS, 거부, TLS), 응답 전 끊김, 시간 초과(요청 180초), 본문 수신 중단 | **일시 장애** | 위와 같음 |
| 그 밖의 4xx(400, 401, 403, 404, 413, 422 등), 2xx인데 JSON이 아님·크기 초과 | 일반 실패 (`Failed`, 기존) | 시도 소모, 백오프, 다섯 번째 실패에서 `waiting_review` |
| 답했지만 결정에 이르지 못함(출력이 도구 호출이 아님, 턴 한도, 거부된 도구 호출) | 일반 실패 (`NoDecision`, 기존) | 위와 같다. 게이트는 이것을 "답했다"로 보고 연속 장애를 끝낸다 |

오류 문자열은 코드가 만든 고정 문구(`HTTP 503 from model provider`, `HTTP model provider is unreachable`, `HTTP model provider timed out`)이며 provider가 보낸 본문·헤더·키를 옮기지 않는다. `Retry-After`는 초 또는 HTTP 날짜로 읽고, 읽을 수 없으면 없는 것으로 본다. 오류 응답의 본문은 JSON이 아니어도 된다(502 HTML 등). 상태 코드가 먼저 분류한다.

curator는 `ProviderError::Unavailable { why, retry_after_secs }`를 `Outcome::Unavailable { why, retry_after_secs }`로 끝낸다. 결정이 없고 저장소는 바뀌지 않으며 감사 기록의 outcome은 `provider_unavailable: <why>`다. 그 전에 모델이 이미 답한 턴의 비용은 `JobReport`에 남는다.

## 동작 규칙

**R1. 대기.** 서버 worker가 `Outcome::Unavailable`을 받으면, 같은 transaction에서 lease(generation, holder)를 확인한 뒤 job을 `waiting_budget`으로 옮기고 holder·lease를 풀고 claim에서 올린 시도를 되돌리고(시도 미소모) `not_before`를 게이트가 다시 열리는 시각으로 둔다. 이벤트 `worker_budget_wait`(target은 기여 ID)를 남기고 `WorkResult::Retry`를 돌려준다. 접수한 기여의 상태는 계속 `queued`다. 장애 전에 모델이 이미 답한 턴의 비용은 일일 합계에 더한다. 결정·카탈로그·점수·감사는 바뀌지 않으며 job은 다음 claim에서 처음부터 다시 판단한다. 이 대기는 R5의 한도 안에서만 시도를 쓰지 않는다.

**R2. 게이트.** 같은 transaction에서 모델 예산 게이트([`queue.md`](queue.md)의 크레딧 소진 절과 같은 게이트)를 닫는다. 닫힌 이유는 `provider_outage`다. 닫힌 동안 어떤 worker도 job을 claim하지 않고 browser replay도 돌리지 않고 모델을 부르지 않는다(`Idle`). 그 사이 접수된 기여는 `queued`로 남는다. 시간이 지나 열리면 job 하나가 탐침으로 나간다(replay 1회와 모델 호출 1회). 성공하면 계속 처리하고 다시 장애면 게이트가 더 오래 닫힌다. recall은 이 게이트에 영향을 받지 않는다.

**R3. 대기 시간.** 게이트가 닫히는 시간은 `max(provider가 알린 Retry-After, 백오프)`이고 최소 30초다. 백오프는 `WAM_LLM_OUTAGE_PAUSE_SECS`(기본 30)에 연속 장애 횟수마다 두 배를 곱한 값이고 최대 `WAM_LLM_OUTAGE_PAUSE_MAX_SECS`(기본 600)다. provider가 알린 시간은 `WAM_LLM_QUOTA_PAUSE_MAX_SECS`(기본 3600)까지 따른다. 연속 장애는 답이 한 번도 없이 이어진 장애의 수이고 소진의 연속 거절 수와 따로 센다. 이미 정해진 더 늦은 재개 시각을 짧게 하지 않는다. 두 이유(소진, 장애)의 pause가 겹치면 더 늦게 끝나는 쪽이 닫힌 이유다.

| 연속 장애 | 1 | 2 | 3 | 4 | 5 | 6 | 7~ |
|---|---|---|---|---|---|---|---|
| 알린 시간이 없을 때 닫히는 초 | 30 | 60 | 120 | 240 | 480 | 600 | 600 |

**R4. 회복.** provider가 답하면(결정이든 아니든) 연속 장애 횟수, 연속 소진 횟수와 그 job의 장애 대기 횟수가 0으로 돌아가고 게이트가 열린다. 게이트가 닫힐 때 `worker_budget_paused`, 열릴 때 `worker_budget_resumed`(target `provider_outage`)가 한 번씩 남는다. 대기하던 job은 R6의 순서로 처리된다.

**R5. 장애 대기 한도.** job마다 장애 때문에 시도 없이 기다릴 수 있는 횟수는 `WAM_LLM_OUTAGE_FREE_WAITS`(기본 8)다. 한도를 다 쓴 job이 또 일시 장애를 만나면 그 시도는 일반 실패처럼 쓰인다: 시도가 늘고 `retry` 백오프가 붙고 다섯 번째에서 `waiting_review`로 간다. 게이트는 이때도 닫힌다(장애는 모두에게 장애다). 0이면 장애를 처음부터 일반 실패로 센다(이전 동작). 소진 대기는 이 한도에 포함하지 않으며 한도 없이 기다린다(크레딧이 며칠 비어도 job은 남는다).

**R6. 순서와 순환.** claim 순서는 장애 대기 횟수가 적은 job이 먼저이고, 같으면 먼저 접수된 job이다(`(대기 횟수, 접수 시각, id)`). 장애가 없으면 모든 횟수가 0이라 지금과 같은 선입선출이다. 따라서 (a) provider 전체 장애에서는 탐침이 job들 사이를 돌아가며 나가서 한 job의 한도가 먼저 바닥나지 않고, (b) 한 job만 장애를 일으키면 그 job은 횟수가 늘수록 뒤로 밀려 다른 job이 먼저 처리되며, 마지막에는 R5로 수동 검토에 간다.

**R7. 철회·재시작·운영자.** `waiting_budget`인 job의 동의 철회는 `canceled`로 만들고 모델에 가지 않는다. 게이트와 job별 장애 대기 횟수는 애플리케이션 상태에 저장되므로 재시작하거나 worker가 여럿이어도 같다. `wam-worker --resume-budget`은 장애 pause도 풀고 `waiting_budget` job을 `queued`로 되돌리며 연속 횟수와 장애 대기 횟수를 0으로 한다(일일 상한은 풀지 않는다).

**R8. 노출.** `wam-worker --budget-status`의 `blocked_by`는 `provider_outage`, `provider_quota`, `daily_cap` 중 하나다. `outage_strikes`(연속 장애 수)와 `reason`(마지막 장애의 고정 문구, 80자 이하, `A-Z a-z 0-9`와 공백 `._:()/-` 밖의 문자는 제거)을 더한다. 게이트가 닫혔다는 시작 알림은 이유에 맞게 말한다: 장애는 "worker가 스스로 다시 시도한다, `--resume-budget`으로 지금 시도할 수 있다", 소진은 "크레딧을 충전한 뒤 `--resume-budget`".

**R9. 비밀과 내용.** 상태, 이벤트, `--budget-status`, 오류 어디에도 provider 응답 본문, 헤더, API key가 나타나지 않는다.

**R10. 호환성.** 스키마 변경과 migration이 없다. job 상태는 기존 `waiting_budget`이고 새 게이트 필드는 모두 읽을 때 기본값이 있어(`serde(default)`) 이전 릴리스가 쓴 상태를 열린 게이트로 읽는다. 이전 릴리스로 되돌려도 새 필드는 무시되고 `waiting_budget` job은 `not_before` 뒤에 다시 claim된다. migration `0003` 이전으로 되돌릴 때의 절차는 [`../operations/deployment.md`](../operations/deployment.md)에 있는 그대로다.

## job 상태 전이 (서버 worker)

| 현재 | 사건 | 다음 | 시도 |
|---|---|---|---|
| `queued`, `retry`, `waiting_budget` | claim(게이트가 열려 있고 `not_before` 지남) | `leased` | +1 |
| `leased` | 일시 장애, 장애 대기가 남음(R5) | `waiting_budget` | 되돌림(±0) |
| `leased` | 일시 장애, 장애 대기를 다 씀 | `retry`, 시도가 5이면 `waiting_review` | +1 유지 |
| `leased` | 소진(기존) | `waiting_budget` | 되돌림(±0) |
| `leased` | 일반 실패, 결정 못함(기존) | `retry`, 시도가 5이면 `waiting_review` | +1 유지 |
| `waiting_budget` | 동의 철회 | `canceled` | |
| `waiting_budget` | `--resume-budget` | `queued` | |

## 설정

| 이름 | 기본 | 범위 | 뜻 |
|---|---|---|---|
| `WAM_LLM_OUTAGE_PAUSE_SECS` | 30 | 30~86400 | 일시 장애가 시간을 알리지 않을 때 처음 닫는 시간. 연속 장애마다 두 배 |
| `WAM_LLM_OUTAGE_PAUSE_MAX_SECS` | 600 | 위 값~86400 | 백오프의 상한 |
| `WAM_LLM_OUTAGE_FREE_WAITS` | 8 | 0~1000 | job마다 장애로 시도 없이 기다릴 수 있는 횟수. 0이면 이전 동작 |

비어 있는 값은 기본값이다. 읽을 수 없거나 범위 밖이면 worker가 시작하지 않는다(소진·일일 상한 설정과 같은 방식이고 오류에 값을 싣지 않는다). 기본값이면 장애가 약 45분(30+60+120+240+480+600+600+600초) 이어질 때까지 한 job은 시도를 쓰지 않는다. 그 뒤 job은 일반 실패처럼 다섯 번 더 쓰인다. 같은 장애를 여러 job이 겪으면 R6의 순환 때문에 이 시간이 job 수만큼 늘어난다.

## 결정 기록

| 질문 | 선택 | 이유와 버린 안 |
|---|---|---|
| 새 job 상태(`waiting_provider`)를 만들까 | 아니오, `waiting_budget`과 같은 게이트를 쓴다 | 새 상태는 PostgreSQL CHECK 제약 migration, 큐 지표, 운영 명령을 모두 늘린다. 게이트는 이미 닫힌 이유·재개 시각·운영 명령·재시작 보존을 갖췄다. 이유는 `blocked_by`로 구분한다 |
| 장애를 job 단위로 세지 말고 worker 단위로만 셀까 (#158 제안 2) | 둘 다 한다 | 게이트(전역)가 호출을 줄이고, job별 한도(R5)가 장애를 일으키는 job이 큐를 영원히 막지 못하게 한다. 전역만 쓰면 그 job이 가장 오래된 job으로서 매 탐침을 독차지한다 |
| 한도 뒤에는 어떻게 할까 | 일반 실패처럼 센다 | 새 종착 상태가 필요 없고 기존 `waiting_review` 경로와 운영 절차를 그대로 쓴다 |
| 순서를 바꿔도 되나 | 장애 대기가 적은 job 먼저, 같으면 선입선출 | 장애가 없으면 순서가 변하지 않는다. 소진 대기는 세지 않아서 크레딧이 돌아온 뒤 순서도 변하지 않는다. 한 job이 탐침을 독차지하는 문제(위)를 새 필드 없이 푼다 |
| 장애 대기 횟수를 어디에 저장하나 | 게이트 문서 안의 job별 맵 | `Work` 구조와 PostgreSQL 행을 바꾸지 않는다(migration 없음). 끝난 job의 항목은 다음 기록 때 지운다 |
| 인증 오류도 기다릴까 | 이번에는 아니오 | 위 범위 밖 항목. 기다림이 맞는 오류(키를 고치면 낫는 것)와 job 내용 때문인 오류(400, 413)를 한 규칙으로 가를 근거가 아직 없다 |

## 요구사항과 테스트

| ID | 요구사항 | 합격 기준 | 관련 테스트 |
|---|---|---|---|
| F-OUT-01 | HTTP provider 분류 | 분류 표대로 `Unavailable`/`QuotaExhausted`/`Failed`로 나뉘고, `Retry-After`(초·HTTP 날짜)를 읽으며, 본문·키를 옮기지 않는다 | MODEL-15, MODEL-16 |
| F-OUT-02 | curator 결과 | 일시 장애는 결정·저장소 변경 없이 `Outcome::Unavailable`로 끝나고 다른 분류는 그대로다 | MODEL-17 |
| F-OUT-03 | 시도 없는 대기 | 장애를 만난 job은 `waiting_budget`, 시도 미소모, 검토행 없음, 기여 `queued` | QUE-25 |
| F-OUT-04 | 전역 게이트 | 닫힌 동안 claim·replay·모델 호출 0, 열릴 때 탐침 1건 | QUE-26 |
| F-OUT-05 | 대기 시간 | R3 표와 `Retry-After` 규칙 | QUE-27, 게이트 단위 |
| F-OUT-06 | 회복과 순서 | 답이 오면 초기화·재개, 대기 job은 순서대로 한 번씩 게시 | QUE-28 |
| F-OUT-07 | 한도와 순환 | 한 job만 장애를 일으켜도 다른 job은 먼저 처리되고 그 job은 한도 뒤 검토행 | QUE-29, 게이트 단위 |
| F-OUT-08 | 기존 계약과 되돌리기 | 일반 실패는 그대로 시도를 쓰고 `WAM_LLM_OUTAGE_FREE_WAITS=0`은 이전 동작 | QUE-30 |
| F-OUT-09 | 지속성·철회·운영자 | R7 | QUE-31, QUE-33, 게이트 단위 |
| F-OUT-10 | 실제 HTTP 경로 | provider 코드와 worker를 loopback 서버로 이어서 장애와 회복을 거친다 | QUE-32 |
| F-OUT-11 | 노출과 비밀 | R8, R9 | QUE-32, 게이트 단위 |
| F-OUT-12 | 호환성 | 이전 릴리스의 게이트 문서를 열린 게이트로 읽고 PostgreSQL 스키마를 바꾸지 않는다 | QUE-33, 게이트 단위 |
