# QUE: 기여·큐레이션 작업 큐

상태: `wam-gatekeeper`의 인메모리 queue/worker, PostgreSQL queue adapter, v1 curator batch input envelope가 구현되었다. PostgreSQL adapter는 nonce, queue limits, fairness tick, scope lease, retry/generation, 철회·수동 상태 전이, purge와 queue stats를 저장·제공한다. 접수/백프레셔는 transaction-scoped admission lock으로 직렬화하고, worker claim은 per-scope advisory lock과 `FOR UPDATE SKIP LOCKED`를 사용한다. worker는 같은 scope의 신규 기여 batch를 한 curator interaction으로 판단하고 같은 계정의 반복 제출은 한 후보로 축약한다. queue는 서로 다른 job kind를 한 batch에 섞지 않으며, lease 뒤 모델 작업은 transaction 밖에서 실행되어 서로 다른 scope는 병렬 처리할 수 있다.

`wam-server`는 이 Gatekeeper PostgreSQL queue adapter와 별도의 durable application worker를 사용한다. 서버 작업은 `AppState.work`와 application repository에 저장되고, `durable::worker`가 Gatekeeper 판정과 설정된 replay verifier를 실행한다. 검증된 게시 결정만 fencing·동의 상태 확인 뒤 catalog/version, attribution, score ledger, outbox에 반영하며, 지원·거절·재시도·추가 검토 결과는 새 공개 버전으로 가장하지 않는다. `application_lifecycle.rs`에는 `Published`, `Supported`, needs-review, replay 실패와 영속성 경로 검사가 있다. `/review`의 `approved`는 수동 검수 상태이며 그 문자열만으로 게시나 점수를 뜻하지 않는다. 2026-11-01 공개 배포의 읽기 전용 확인에서는 API와 email auth 설정은 보였지만 trusted replay worker/provider 및 실운영 기여 수용은 아직 확인되지 않았다. 공개 운영 상태와 남은 수용 항목은 [email-auth-production.md](../test/email-auth-production.md) 및 이슈 #159를 따른다.

## 목적

기여가 몰려도 요청을 잃지 않고, 같은 URL 범위의 판정이 서로 충돌하지 않게 하며, 모델 호출 비용(해커톤 크레딧)과 속도 제한 안에서 처리한다. 접수는 빨리 끝내고(202) 판정은 큐에서 비동기로 한다.

## 작업 종류와 상태

| 종류 | 만든 곳 | 우선순위 |
|---|---|---|
| `contribution` | `POST /v1/contributions` | P1 |
| `reverify` | 게시 후 감시(G6), 오래된 메모리 | P0 |
| `withdrawal` | 기여 귀속 철회, 계정 삭제 뒤 재평가 | P1 |
| `rollback` | 관리자 kill switch 뒤 재판정 | P0 |

Gatekeeper queue job은 `queued`, `leased`, `done` 흐름과 예외 상태 `waiting_scope`(같은 scope의 다른 job이 처리 중), `waiting_budget`(모델 예산 소진), `retry`(백오프 대기), `dead`(재시도 한도 초과), `canceled`(철회)를 쓴다. 웹 owner/API의 contribution status에는 `queued`, `in_review`, `approved`, `rejected`, `withdrawn`, `expired`, `consent_required`가 있다. `approved`는 review 결정 상태일 뿐 게시 증거가 아니다. 게시 완료는 durable worker의 검증된 `Published` 결과와 memory/version·attribution 연결로 확인하며, `Supported`는 기존 memory 보강으로 구분한다.

## 요구사항

**접수.** 기여 저장과 job 생성은 한 트랜잭션이다. 202를 받은 job은 어떤 장애에서도 사라지지 않는다. 큐가 가득 차면 접수 전에 429(`Retry-After`) 또는 503으로 거절하고, 이미 접수한 job을 버려서 자리를 만들지 않는다. 깊이 상한은 전체, 사용자, 기기 단위로 둔다. 상한에 가까워지면 익명과 신규 계정 쪽을 먼저 제한한다.

**scope 직렬화와 병합.** job의 `scope_key`는 [`dedup.md`](dedup.md)의 비교 키다. 같은 `scope_key`는 한 번에 한 worker만 처리한다(advisory lock). 같은 scope의 대기 job은 한 번의 큐레이터 호출로 묶어 처리한다(기본 최대 20건, 대기 창 60초, 설정 가능). 묶음은 중복 판정을 쉽게 하고 모델 호출을 줄인다. 다른 scope는 worker 수만큼 병렬로 처리한다. 정확한 fingerprint 중복은 모델을 부르기 전에 결정적으로 처리한다([`curator.md`](curator.md)).

**신뢰성.** PostgreSQL adapter는 scope별 transaction advisory lock과 `FOR UPDATE SKIP LOCKED`로 job을 claim하고 `lease_owner`, 만료 시각, 단조 증가 `lease_generation`을 기록한다(기본 5분, 하트비트로 연장). 동일 scope를 두 worker가 동시에 claim하는 테스트는 한 worker만 batch를 받는지 확인한다. 만료 뒤 다른 worker가 새 generation을 발급받으면 오래된 worker가 늦게 돌아와도 이전 generation으로 결과를 commit할 수 없다(fencing): worker는 lease token을 curator job에 전달하고 `PostgresStore`가 memory mutation과 decision journal transaction 안에서 모든 job generation과 owner를 다시 잠그고 검증한다. queue adapter가 초기화된 DB에서는 fence가 빠진 결정을 거부한다. 재시도는 지수 백오프에 지터를 더하고 최대 5회 뒤 `dead`로 옮긴다. 모델 호출 중에는 DB 트랜잭션을 잡지 않는다.

**모델 예산.** 큐레이터 호출은 분당 요청·토큰 토큰버킷과 일일 크레딧 상한을 지킨다. 한도에 닿으면 job은 `waiting_budget`으로 남고 사라지지 않는다. 모델 장애, 429, 시간 초과는 카탈로그를 바꾸지 않고 재시도한다([`model.md`](model.md)). 서버 worker는 이 일시 장애를 시도를 쓰지 않는 대기로 다루고([`provider-outage.md`](provider-outage.md), #158), 키·모델 설정 거부(401, 403, 404)도 같은 방식으로 더 길게 기다린다(#168). 예산 소진은 recall 응답에 영향을 주지 않는다.

**크레딧 소진과 대기(서버 worker).** 애플리케이션 worker도 같은 약속을 지킨다. provider가 쿼터·크레딧 소진이나 결제 미설정을 알리면(HTTP 402, `insufficient_quota`, 사용 한도 문구) 그 오류를 만난 job은 시도를 쓰지 않고 `waiting_budget`으로 대기하며, 동시에 내구성 있는 모델 예산 게이트가 닫힌다. 게이트가 닫힌 동안에는 어떤 worker도 job을 claim하지 않고 browser replay도 돌리지 않으며 모델을 부르지 않는다. 기여 접수는 계속되고 기여는 `queued`로 남는다. 닫히는 시간은 provider가 알려 준 시간(최소 30초, 최대 1시간)이고, 알리지 않으면 15분에서 시작해 연속 거절마다 두 배로 늘어 최대 1시간에 머문다(`WAM_LLM_QUOTA_PAUSE_SECS`, `WAM_LLM_QUOTA_PAUSE_MAX_SECS`). 시간이 지나면 job 하나가 탐침으로 나가 성공하면 게이트가 열리고, 다시 거절되면 더 오래 닫힌다. 게이트는 애플리케이션 상태(`wam_app_meta`)에 저장되므로 worker가 여럿이거나 재시작해도 같다. 크레딧을 충전한 직후에는 `wam-worker --resume-budget`으로 기다리지 않고 재개하고(대기 job은 다시 `queued`가 된다), `wam-worker --budget-status`로 닫힌 이유, 재개 시각, 오늘 지출, 대기 job 수를 본다. 대기 job은 큐 건강 지표(`queue_health`)에 포함되어 가장 오래된 job의 나이가 알림 기준을 넘으면 알린다. 동의 철회는 `waiting_budget` job도 `canceled`로 바꾼다.

**모델 일시 장애와 대기(서버 worker, #158).** provider의 일시 장애(HTTP 429로 소진 신호가 없는 것, 408, 5xx, 연결 실패, 시간 초과, 본문 수신 중단)는 job의 잘못이 아니므로 job을 일반 실패로 세지 않는다. 장애를 만난 job은 시도를 쓰지 않고 `waiting_budget`으로 대기하고, 같은 모델 예산 게이트가 `provider_outage`로 닫혀 모든 worker의 claim·replay·모델 호출이 멈춘다. 닫히는 시간은 provider가 알린 `Retry-After`와 백오프(30초에서 시작해 연속 장애마다 2배, 최대 10분) 중 큰 값이고 최소 30초다. provider가 답하면 게이트가 열리고 대기 job이 먼저 온 순서대로 처리된다. 한 job이 장애를 일으켜 큐를 막지 못하도록 job마다 시도 없이 기다릴 수 있는 장애는 8번이고(`WAM_LLM_OUTAGE_FREE_WAITS`, 0이면 이전 동작) 장애 대기가 적은 job이 먼저 탐침으로 나간다. 8번을 쓴 job은 이후 장애를 일반 실패처럼 세어 다섯 번째에 `waiting_review`로 간다. 요청 오류(400, 413, 422)와 독립 `wam-gatekeeper` engine은 이번 범위 밖이다(401, 403, 404는 아래 문단). 상세와 요구사항은 [`provider-outage.md`](provider-outage.md), 검증은 [`../test/provider-outage.md`](../test/provider-outage.md)에 있다.

**키·모델 설정 거부와 대기(서버 worker, #168).** provider가 키나 모델 설정을 거부하면(HTTP 401, 403, 404, 소진 신호가 없을 때) 이것도 job의 잘못이 아니므로 일반 실패로 세지 않는다. job은 시도를 쓰지 않고 `waiting_budget`으로 대기하고 같은 게이트가 `provider_rejected`로 닫힌다. 사람이 키나 설정을 고쳐야 낫기 때문에 장애보다 길게 닫힌다: 1시간에서 시작해 연속 거부마다 2배, 최대 6시간(`WAM_LLM_REJECTED_PAUSE_SECS`, `WAM_LLM_REJECTED_PAUSE_MAX_SECS`). job의 대기 한도(`WAM_LLM_OUTAGE_FREE_WAITS`)와 탐침 순환은 장애와 합쳐서 센다. 고치면 다음 탐침이 저절로 알아차리고 `wam-worker --resume-budget`은 지금 알아차린다. 운영자에게는 `--budget-status`(`blocked_by`, `rejected_strikes`, `reason`), 닫힘 로그(키·모델·주소를 확인하라는 안내), 거부 pause 중 켜지는 `queue_health.alert`로 알린다. 400, 413, 422는 계속 일반 실패다. 상세는 [`provider-outage.md`](provider-outage.md) R11~R14, 검증은 [`../test/provider-outage.md`](../test/provider-outage.md)에 있다.

**운영자 일일 상한.** 콘솔의 월 한도는 이메일 알림일 뿐 차단하지 않으므로 `WAM_LLM_DAILY_BUDGET_USD`로 직접 건다. provider가 보고한 `cost_usd`의 UTC 일일 합이 상한에 닿으면 다음 UTC 날짜까지 새 job을 시작하지 않고 기여는 `queued`로 대기한다. 상한은 job 시작 전에 검사하므로 마지막 job은 상한을 넘을 수 있고, worker가 여럿이면 그 수만큼 더 넘을 수 있다. HTTP provider는 `WAM_LLM_PRICE_INPUT_PER_M`, `WAM_LLM_PRICE_CACHED_INPUT_PER_M`, `WAM_LLM_PRICE_OUTPUT_PER_M`(1M 토큰당 달러)으로 `usage`에서 비용을 계산한다. 비용을 보고하지 않는 provider에서는 상한이 발동하지 않으므로 worker는 시작을 거부한다. 정확한 fingerprint 중복은 모델 호출 전에 규칙으로 처리하므로 비용이 들지 않는다.

**공정성과 우선순위.** P0 다음에 P1을 처리한다. 같은 우선순위에서는 기여자·기기별 라운드 로빈이라 한 사용자의 폭주가 다른 사용자의 job을 굶기지 않는다. 한 scope 안에서는 먼저 온 것이 먼저다.

**철회와 보관.** 동의 철회는 `queued`, `waiting_*`, `retry` job을 `canceled`로 바꾸고 payload를 삭제한다. 이미 `leased`인 job은 분석을 끝낼 수 있지만 최종 commit 직전에 최신 동의 상태를 다시 확인한다. 철회 transaction이 먼저 commit됐으면 job은 공개 상태나 작성자 점수를 바꾸지 못하고 취소/대기로 끝난다. publish transaction이 먼저 끝난 경우는 별도 withdrawal job이 attribution과 남은 evidence를 재평가한다. 끝난 job의 payload는 보관 기간 뒤 purge한다.

**관측.** 큐 깊이, 가장 오래된 job의 나이, 처리량, 재시도·dead 수, 모델 지연·토큰·크레딧 사용량을 지표로 낸다. 가장 오래된 job이 15분을 넘으면 알린다. 수치 기준(처리 지연 SLO, dead 비율)은 [`../test/metrics.md`](../test/metrics.md)에 둔다.

**구현과 세부 정책.** 인메모리 큐와 PostgreSQL queue adapter(`wam-gatekeeper`의 `queue.rs`, `postgres_queue.rs`, `budget.rs`, `worker.rs`)가 worker 정책을 공유한다. `CuratorBatch` v1은 ordered curator job list, scope key, content digest를 담고 최대 후보 수, 빈/중복 job id, 혼합 scope, 지원하지 않는 형식을 검증한다. worker는 이 envelope를 검사한 뒤 후보별 idempotent curator loop를 실행한다. 따라서 한 scope를 한 번 lease하는 것과 후보마다 모델을 다시 호출하는 것은 구분한다. 정해 둔 큐 세부는 다음과 같다. 병합 창은 scope의 첫 job이 도착한 때 시작하고 늦게 온 같은 scope job이 같은 묶음에 합류한다. 20건이 차거나 P0이거나 재시도면 창을 기다리지 않는다. 깊이 상한은 전체·사용자·기기 단위이고, 전체의 80%부터 신규·익명 계정을 429로 제한하며 100%에서는 모두 503으로 거절한다. P0(안전 작업)는 120%까지 받아 막히지 않게 한다. idempotency key는 (기여자, nonce)이고 철회 뒤에도 유지해 늦게 도착한 옛 요청이 취소된 job을 되살리지 못하며, payload purge 때 함께 지운다. lease가 만료되면 job은 시도 1회를 쓰고 바로 대기열로 돌아가므로 worker를 계속 죽이는 job도 5회 뒤 `dead`가 된다. fencing 토큰은 batch의 job별 lease generation이다. 인메모리 worker는 curator job마다 lease를 다시 확인하고, PostgreSQL store는 queue가 활성화되면 decision transaction에서 모든 generation과 owner를 검증한다. 재시도된 묶음은 이미 결정한 job(같은 id, 같은 내용 digest)을 다시 적용하지 않는다. 모델 예산은 요청·토큰 버킷과 UTC 일일 크레딧 상한이며 호출 전에 확인하므로 마지막 호출이 상한을 조금 넘길 수 있다. provider가 사용 한도나 크레딧 소진을 알리면(`ProviderError::QuotaExhausted`) 시도를 쓰지 않고 `waiting_budget`으로 대기시키고, 예산 게이트가 provider가 알려 준 시간(없으면 기본 15분) 동안 모든 worker의 모델 호출을 막는다. 소진을 뜻하는 문구가 아닌 오류는 일반 실패로 보고 백오프로 재시도한다(독립 engine의 동작이다. 서버 worker는 일시 장애를 위 문단처럼 기다림으로 다룬다). batch-wide curator interaction, wam-server 연결, contribution과 job의 원자 접수는 미구현이다.

## 요구사항과 테스트

| ID | 요구사항 | 합격 기준 | 관련 테스트 |
|---|---|---|---|
| F-QUE-01 | 접수 원자성 | 202 이후 job 유실 0, 기여 저장과 job 생성은 같은 트랜잭션 | QUE-01, DB-10 |
| F-QUE-02 | scope 직렬화 | 같은 scope 동시 처리 1개, active version 중복 0 | QUE-02, DEDUP-12, GK-37 |
| F-QUE-03 | 병합 | 같은 scope 대기 기여 N건이 한 번의 모델 호출로 처리된다 | QUE-03 |
| F-QUE-04 | 백프레셔 | 상한 초과 시 429/503과 `Retry-After`, 접수된 job 보존 | QUE-04, API-24 |
| F-QUE-05 | 공정성과 우선순위 | 폭주 사용자가 다른 사용자를 굶기지 않고 P0가 먼저 처리된다 | QUE-05, QUE-11 |
| F-QUE-06 | 재시도와 dead-letter | 백오프, 한도 뒤 `dead`, 재시도 멱등 | QUE-06 |
| F-QUE-07 | worker 장애와 stale lease | lease 만료 뒤 재처리, 이전 generation의 늦은 commit 거부, 중복 게시 0 | QUE-07, GK-38, DB-10~11 |
| F-QUE-08 | 모델 예산과 장애 | `waiting_budget`/`retry`, 카탈로그 불변, 유실 0 | QUE-08~09, MODEL-05~06 |
| F-QUE-09 | 철회 | 대기 job 취소와 payload 삭제, in-flight publish와의 commit 순서를 확인 | QUE-10, PRIV-03~04, DB-18 |
| F-QUE-10 | 관측과 상태 노출 | 지표와 기여자 상태가 job 상태와 일치 | QUE-12~13, WEB-20~22 |
| F-QUE-11 | 보관 | 끝난 job payload가 기한 뒤 삭제된다 | QUE-14, PRIV-05 |
| F-QUE-12 | versioned curator batch input | 최대 크기와 scope/job 식별자가 검증되고 잘못된 envelope는 모델 호출 전에 거부된다 | QUE-15 |
| F-QUE-13 | PostgreSQL queue persistence | 접수 nonce·queue state가 재시작 후 유지되고 같은 scope의 active lease는 하나다 | QUE-16, DB-22 |
| F-QUE-14 | transactional lease 및 attribution fencing | 오래된 owner/generation과 철회된 contribution은 memory decision transaction을 commit할 수 없고 현재 fence는 한 번만 반영된다 | QUE-17, DB-23, DB-25 |
| F-QUE-15 | withdraw, resolve, retention, monitoring | 철회는 대기 job을 취소하고 실행 중 attribution을 제거하며, review 상태 전이는 stale worker를 fence하고 purge/stats는 저장 상태와 일치한다 | QUE-18, DB-24 |
| F-QUE-16 | joint contribution batch | 같은 scope와 trigger의 batch는 한 curator interaction에서 하나의 scope 결정을 내리고, 같은 계정의 재제출은 독립 근거를 늘리지 않는다 | QUE-19, CUR-14 |
| F-QUE-17 | 모델 일시 장애 대기 (서버 worker) | 429·408·5xx·연결 실패·시간 초과가 시도를 쓰지 않고 `waiting_budget`으로 대기시키고 게이트가 `provider_outage`로 닫힌다. 장애를 일으키는 job은 한도 뒤 검토행이고 다른 job을 막지 않는다 | QUE-25~33, MODEL-15~17 (`provider-outage.md` F-OUT-01~12) |
| F-QUE-18 | 키·모델 설정 거부 대기 (서버 worker) | 401·403·404(소진 신호 없음)가 시도를 쓰지 않고 `waiting_budget`으로 대기시키고 게이트가 `provider_rejected`로 1시간부터 닫힌다. 한도·순환은 장애와 합산한다. 운영자는 `--budget-status`, 닫힘 로그, `queue_health.alert`로 알게 된다 | QUE-34~39, MODEL-18~19 (`provider-outage.md` F-OUT-13~15) |
