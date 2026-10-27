# QUE: 기여·큐레이션 작업 큐

상태: 1차 상세 명세. 큐 워커와 부하 제어는 아직 구현되지 않았다. 저장은 [`database.md`](database.md)의 `gatekeeper_jobs`를 확장한다.

## 목적

기여가 몰려도 요청을 잃지 않고, 같은 URL 범위의 판정이 서로 충돌하지 않게 하며, 모델 호출 비용(해커톤 크레딧)과 속도 제한 안에서 처리한다. 접수는 빨리 끝내고(202) 판정은 큐에서 비동기로 한다.

## 작업 종류와 상태

| 종류 | 만든 곳 | 우선순위 |
|---|---|---|
| `contribution` | `POST /v1/contributions` | P1 |
| `reverify` | 게시 후 감시(G6), 오래된 메모리 | P0 |
| `withdrawal` | 기여 귀속 철회, 계정 삭제 뒤 재평가 | P1 |
| `rollback` | 관리자 kill switch 뒤 재판정 | P0 |

상태는 `queued`, `leased`, `done` 흐름과 예외 상태 `waiting_scope`(같은 scope의 다른 job이 처리 중), `waiting_budget`(모델 예산 소진), `retry`(백오프 대기), `dead`(재시도 한도 초과), `canceled`(철회)다. 웹의 기여 상태(`received`, `validating`, `comparing` 등)는 이 상태와 판정 단계에서 파생한다.

## 요구사항

**접수.** 기여 저장과 job 생성은 한 트랜잭션이다. 202를 받은 job은 어떤 장애에서도 사라지지 않는다. 큐가 가득 차면 접수 전에 429(`Retry-After`) 또는 503으로 거절하고, 이미 접수한 job을 버려서 자리를 만들지 않는다. 깊이 상한은 전체, 사용자, 기기 단위로 둔다. 상한에 가까워지면 익명과 신규 계정 쪽을 먼저 제한한다.

**scope 직렬화와 병합.** job의 `scope_key`는 [`dedup.md`](dedup.md)의 비교 키다. 같은 `scope_key`는 한 번에 한 worker만 처리한다(advisory lock). 같은 scope의 대기 job은 한 번의 큐레이터 호출로 묶어 처리한다(기본 최대 20건, 대기 창 60초, 설정 가능). 묶음은 중복 판정을 쉽게 하고 모델 호출을 줄인다. 다른 scope는 worker 수만큼 병렬로 처리한다. 정확한 fingerprint 중복은 모델을 부르기 전에 결정적으로 처리한다([`curator.md`](curator.md)).

**신뢰성.** worker는 `FOR UPDATE SKIP LOCKED`로 job을 가져가 `lease_owner`, 만료 시각, 단조 증가 `lease_generation`을 기록한다(기본 5분, 하트비트로 연장). 만료 뒤 다른 worker가 새 generation을 발급받으면 오래된 worker가 늦게 돌아와도 이전 generation으로 결과를 commit할 수 없다(fencing). 재시도는 지수 백오프에 지터를 더하고 최대 5회 뒤 `dead`로 옮긴다. 결정 반영은 한 트랜잭션이며 job 내용 digest, 유효 lease generation, idempotency key에 묶여 재시도가 중복 게시나 점수를 만들지 않는다. 모델 호출 중에는 DB 트랜잭션을 잡지 않는다.

**모델 예산.** 큐레이터 호출은 분당 요청·토큰 토큰버킷과 일일 크레딧 상한을 지킨다. 한도에 닿으면 job은 `waiting_budget`으로 남고 사라지지 않는다. 모델 장애, 429, 시간 초과는 카탈로그를 바꾸지 않고 재시도한다([`model.md`](model.md)). 예산 소진은 recall 응답에 영향을 주지 않는다.

**공정성과 우선순위.** P0 다음에 P1을 처리한다. 같은 우선순위에서는 기여자·기기별 라운드 로빈이라 한 사용자의 폭주가 다른 사용자의 job을 굶기지 않는다. 한 scope 안에서는 먼저 온 것이 먼저다.

**철회와 보관.** 동의 철회는 `queued`, `waiting_*`, `retry` job을 `canceled`로 바꾸고 payload를 삭제한다. 이미 `leased`인 job은 분석을 끝낼 수 있지만 최종 commit 직전에 최신 동의 상태를 다시 확인한다. 철회 transaction이 먼저 commit됐으면 job은 공개 상태나 작성자 점수를 바꾸지 못하고 취소/대기로 끝난다. publish transaction이 먼저 끝난 경우는 별도 withdrawal job이 attribution과 남은 evidence를 재평가한다. 끝난 job의 payload는 보관 기간 뒤 purge한다.

**관측.** 큐 깊이, 가장 오래된 job의 나이, 처리량, 재시도·dead 수, 모델 지연·토큰·크레딧 사용량을 지표로 낸다. 가장 오래된 job이 15분을 넘으면 알린다. 수치 기준(처리 지연 SLO, dead 비율)은 [`../test/metrics.md`](../test/metrics.md)에 둔다.

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
