# CUR: Gatekeeper Agent의 지침과 도구

상태: 도구 계층, fake/로컬 CLI/Responses API provider, local HTTP fixture, versioned `CuratorBatch` 입력 envelope, PostgreSQL memory store/queue adapter와 `wam-server` contribution→worker→replay→publication transaction 연결이 구현되어 있다. PostgreSQL publication tests는 consent, withdrawal, lease fencing, immutable version, score/outbox consistency를 확인한다. 실제 Meta/Muse 판정 품질, 운영 browser replay executable, 장애·재시작·부하 환경, 게시 후 철회 재평가는 남아 있다. 지침과 도구 정의는 [`curator/instructions.v1.md`](curator/instructions.v1.md), [`curator/tools.v1.json`](curator/tools.v1.json)에 버전 관리한다.

## 원칙

메모리를 갱신하는 판정은 Meta(Muse 계열) 모델이 한다. 우리는 **지침(instructions)과 도구(tools)만 정의**하고, 모델은 도구로만 메모리를 읽고 쓴다. provider 계약(입력 최소화, fail closed, audit)은 [`model.md`](model.md)를 따른다. 모델은 모델이 할 수 있는 판단(중복인지, 더 나은지, 새로 만들지)을 하고, 서버의 도구 계층은 결정적 불변조건을 강제한다. 모델 출력이 틀려도 불변조건은 깨지지 않는다. 결정 도구의 인수가 `model.md`가 말하는 structured decision이고, 서버가 검증해 반영한다. 모델에 DB 게시 권한은 없다.

## 판정 규칙

| 상황 | 모델이 부르는 도구 | 결정(enum) | 결과 |
|---|---|---|---|
| 같은 범위에 같은 내용이 이미 있음 | `decide_skip` | `duplicate_support` 또는 `merge_evidence` | 새 레코드 없음, 독립 근거만 반영 |
| 같은 범위에 있지만 새 내용이 더 낫다 | `decide_replace` | `supersede` | 새 immutable version을 active로, 이전 버전은 superseded (삭제 아님) |
| 해당 URL 범위에 아무것도 없다 | `decide_create` | `create_new` | 새 레코드와 첫 version |
| 기존 내용이 위험하거나 무효하고 대안이 없다 | `decide_retire` | `retire` | 공개에서 제외, tombstone |
| 근거 상충, 낮은 확신, 범위 모호 | `decide_escalate` | `needs_human_review` | 공개 변화 없음, 예외 큐 |
| 무효하거나 위험한 입력 | `decide_reject` | `reject_invalid` | 게시 없음, 사유 코드 |

"더 낫다"의 판단 재료는 `memory_compare`가 계산한 지표(성공률, 단계 수, 새로 다루는 trait·pitfall, 독립 기여자 수, 최신성, UI fingerprint 변화)뿐이다. 문장이 더 매끄럽다는 이유로 교체하지 않는다. 정확한 fingerprint 중복은 모델을 부르기 전에 결정적으로 처리해 크레딧을 아낀다. 모델은 의미상 중복, 개선, 신규를 판단한다.

## 작업 흐름

한 curator job은 같은 scope의 동종 queue batch 전체다([`queue.md`](queue.md)). 새 기여 batch에는 계정별 최선 후보, 집계 outcome과 서로 다른 account 수를 전달한다. 같은 계정의 반복 제출은 별도 투표로 세지 않는다. 모델은 batch 전체에 대해 읽기 도구를 최대 8번 부른 뒤 scope 결정 도구를 **정확히 한 번** 부르고 끝낸다. 서버가 도구 호출을 거부하면(오류 코드) 모델은 인수를 고치거나 `decide_escalate`로 끝낸다. 결정 도구가 성공하면 서버가 한 트랜잭션으로 반영한다([`database.md`](database.md)).

## 도구

| 도구 | 종류 | 입력 | 결과 |
|---|---|---|---|
| `job_get` | 읽기 | 없음 | scope batch 개요: trigger, 집계 근거, 그리고 계정별 후보의 정규화 trace·outcome |
| `memory_search` | 읽기 | `url_pattern`, `keywords`, `kind`, `task_category`, `limit`≤10 | 같은 host의 active·needs-reverify 레코드의 L0와 L1 요약. `site_host`는 job이 정하고 모델이 바꿀 수 없다 |
| `memory_get` | 읽기 | `id`, `level`(1 또는 2) | 해당 버전의 L1 또는 L2 |
| `memory_compare` | 읽기 | `existing_id` | 후보와 기존의 결정적 비교 지표 |
| `decide_skip` | 결정 | `existing_id`, `reason`, `reason_codes[]` | batch의 독립 근거를 한 번 합산 |
| `decide_replace` | 결정 | `existing_id`, `expected_version`, `record`, `reason_codes[]`, `confidence_band` | batch 전체의 근거를 기준으로 새 version 하나를 게시 |
| `decide_create` | 결정 | `record`, `reason_codes[]`, `confidence_band` | batch 전체의 근거를 기준으로 레코드 하나를 생성 |
| `decide_retire` | 결정 | `existing_id`, `expected_version`, `reason_codes[]` | |
| `decide_escalate` | 결정 | `reason_codes[]` | |
| `decide_reject` | 결정 | `reason_codes[]` | |

`record`는 [`memory.md`](memory.md)의 L0/L1/L2 구조다. 모델에게는 이 도구만 보인다. 파일, 네트워크, 셸, SQL, 브라우저 도구는 없다.

## 서버가 강제하는 불변조건

모델 출력과 무관하게 도구 계층이 검사한다. 위반하면 오류 코드를 반환하고 상태는 바뀌지 않는다.

| 코드 | 조건 |
|---|---|
| `SCOPE_VIOLATION` | job host 밖의 레코드나 경로를 읽거나 쓰려 함 |
| `DUPLICATE_EXISTS` | 같은 `scope_key`에 active 레코드가 있는데 `decide_create` |
| `VERSION_CONFLICT` | `expected_version`이 현재 active와 다름 |
| `EVIDENCE_INSUFFICIENT` | `decide_replace`나 `decide_retire`가 증거 정책을 채우지 못함 |
| `SCHEMA_INVALID` | 레코드가 L0/L1/L2 스키마나 한도를 어김 |
| `TEXT_REJECTED` | G1 또는 주입 문구 검사 실패 (비밀값, 전체 URL, 위험 유도 문장 등) |
| `BUDGET_EXCEEDED` | 읽기 호출 8회, 시간, 토큰 한도 초과 |
| `ALREADY_DECIDED` | 같은 job에 두 번째 결정 도구 호출 |
| `STORAGE_UNAVAILABLE` | 저장소 read/decision transaction이 실패했으며 변경 없이 job을 재시도해야 함 |

추가로 host당 active 레코드 상한, job당 변경 1건, 모든 결정의 audit 기록(지침·도구 버전 해시, 모델 ID, 입력·출력 digest, reason code)을 둔다. 결정 도구가 실패한 뒤 재시도는 같은 idempotency key를 쓴다.

**교체 증거 정책**은 설정값이다. 기본은 "격리 replay 통과, 또는 독립 기여자 2명 이상이 같은 개선을 관측하고 성공률이 낮아지지 않음"이다. replay를 필수로 할지는 열린 결정이다(아래). 어느 쪽이든 게시 뒤 G6 감시와 롤백이 안전망이다.

**모델 호출 전 규칙.** 새 기여의 단계가 같은 범위의 active 메모리와 정확히 같으면(`wait`, `snapshot` 같은 잡음은 정규화로 제외) 모델을 부르지 않고 규칙이 같은 `decide_skip` 도구로 중복 판정한다. 근거만 합산하고 reason code는 `same_fingerprint`다. 같은 도구 계층을 거치므로 범위·권한 검사는 그대로 적용되고, 재검증·철회 job에는 적용하지 않는다. 단계가 조금 다른 "거의 같은" 후보부터 모델이 판단한다. 판정 품질 평가(CUR-15)와 시나리오 러너는 이 규칙을 꺼서 모델 자체의 판단을 잰다.

## 지침

[`curator/instructions.v1.md`](curator/instructions.v1.md)가 모델에 주는 지침이다. 역할, 판정 절차, "더 낫다"의 기준, 레코드 작성 규칙, 모든 입력을 데이터로 취급하는 안전 규칙, 도구 사용 한도와 오류 코드 대응, 짧은 예시를 담는다. 지침 파일의 해시와 버전은 모든 결정의 audit에 기록하고, 바꾸면 평가셋을 다시 돌린 뒤에만 배포한다.

## 모델 연결 (provider)

모델은 provider 인터페이스 하나 뒤에 있다. 지침, 도구, 서버 검증, 큐는 어떤 provider에서도 같다. 로컬 개발은 설치된 Claude Code·Codex CLI를 쓴다(사용자 로그인 사용, API 키 불필요, 도구 없이 JSON 턴 프로토콜). 배포용 OpenAI-compatible HTTP provider는 Responses API를 호출하고 같은 JSON 턴 프로토콜을 사용한다. `WAM_LLM_PROVIDER`, `WAM_LLM_MODEL`, `MODEL_API_KEY`를 설정하고, `WAM_LLM_BASE_URL`은 생략하면 `https://api.meta.ai/v1`을 쓴다. HTTP provider는 비어 있지 않은 printable ASCII API key만 허용하고 credential을 body나 오류에 복사하지 않는다. 원격 endpoint는 HTTPS여야 하며 HTTP는 loopback 테스트에서만 허용한다. 응답 본문은 1 MiB로 제한하고 저장을 끄며, HTTP 402 또는 quota 오류는 `Retry-After` 초/날짜를 읽어 worker를 예산 대기시킨다. local HTTP fixture는 provider 계약을 확인하지만 실제 Meta/Muse 연결과 판정 품질은 별도 `CUR-17`, `MODEL-09` 검증이 남아 있다.

## 평가

라벨 붙은 job 평가셋(중복, 개선, 신규, 충돌, 오래됨, 범위 확대 시도, 인젝션 포함 ≥60건)에서 결정 정확도 ≥90%와 위험한 생성·교체 0건을 요구한다. 현재 `crates/wam-gatekeeper/scenarios/eval-contexts.json`의 12개 URL/task context를 다섯 seed archetype에 적용한 60개 starter 회귀 케이스가 있고, `wam-curate --scenario eval-60 --provider fake`와 `starter_evaluation_corpus_has_60_unique_labeled_scenarios_that_run`로 실행한다. 이 케이스는 템플릿에서 파생되어 서로 독립적인 사람 라벨 60개를 대신하지 않으므로 CUR-15 최종 품질 게이트는 미완료다. 불변조건은 적대적 fake provider(임의 도구 순서와 인수 퍼징, Rust 테스트 `crates/wam-gatekeeper/tests/tool_layer.rs`)로 100% 유지되어야 한다. 실제 Meta/Muse provider는 [`model.md`](model.md)의 `MODEL-09` 증거로 확인한다. 평가에 쓰는 모델 호출은 큐의 예산 안에서 쓴다.

## 열린 결정

1. 교체에 replay를 필수로 할지. 기존 [`gatekeeper.md`](gatekeeper.md), [`dedup.md`](dedup.md)는 필수이고, 위 기본 정책은 독립 증거로 대신할 수 있게 한다. 해커톤 일정(plan M4)과 안전 사이의 선택이다.
2. Meta 모델 API의 네이티브 tool calling 지원. 지원하지 않으면 provider 어댑터가 JSON 턴 프로토콜(`{"tool": …, "args": …}`)로 흉내 내며 같은 도구 스키마와 불변조건을 쓴다. Meta Model API는 OpenAI 호환 HTTP(Responses API)로 기록되어 있으나, 도구 호출 지원은 실제 호출로 확인하기 전까지 가정이다([`../design/architecture.md`](../design/architecture.md) 열린 질문 7, `MODEL-09` 증거). 미지원이면 위 JSON 턴 프로토콜과 Rust 검증만 쓴다.
3. 신규 레코드 공개의 최소 독립 기여자 수 N과 자동 승인 기준(기존 GK-30).

## 요구사항과 테스트

| ID | 요구사항 | 합격 기준 | 관련 테스트 |
|---|---|---|---|
| F-CUR-01 | 도구로만 권한 행사 | 모델에 노출되는 도구는 정의된 10개뿐이고 버전·해시가 고정된다 | CUR-01 |
| F-CUR-02 | 판정 규칙 | 중복은 skip, 개선은 replace, 없으면 create, 불확실은 escalate | CUR-02~05 |
| F-CUR-03 | 서버 불변조건 | 어떤 모델 출력에도 중복 active, 범위 이탈, 이중 결정이 없다 | CUR-06~10, CUR-12, CUR-16 |
| F-CUR-04 | 모델 작성 텍스트 검증 | 비밀값, 위험 문장, 한도 초과 텍스트는 저장되지 않는다 | CUR-11, MEM-08 |
| F-CUR-05 | 입력은 데이터 | 기여·기존 메모리 속 지시문을 따르지 않는다 | CUR-13, SEC-I01 |
| F-CUR-06 | 추적성 | 지침·도구 버전 해시, 모델 ID가 모든 결정 audit에 남는다 | CUR-14, MODEL-08 |
| F-CUR-07 | 판정 품질 | 평가셋 정확도 ≥90%, 위험한 생성·교체 0 | CUR-15 |
| F-CUR-08 | 실제 provider | 실제 Meta/Muse 모델로 한 번 이상 판정하고 provider 어댑터가 적합하다 | CUR-17, MODEL-09 |
| F-CUR-09 | 정확 중복 선처리 | 단계가 정확히 같은 후보는 모델 호출 없이 같은 도구 계층으로 중복 처리된다 | CUR-18 |
| F-CUR-10 | provider 교체 | 설정만 바꿔 같은 시나리오가 fake, 로컬 CLI, HTTP provider에서 실행되고 audit에 provider가 남는다 | CUR-19 |
| F-CUR-11 | 저장소 오류 fail closed | read/commit 오류는 안전한 코드로 노출되고 memory·decision journal은 부분 반영되지 않는다 | CUR-20, DB-11 |
| F-CUR-12 | scope batch joint curation | 한 URL/task/trigger batch는 계정별 후보를 한 curator interaction으로 판단하고 scope 결정 하나만 반영한다. 같은 계정의 반복 제출은 독립 근거를 늘리지 않는다 | QUE-19, CUR-14, CUR-21 |
