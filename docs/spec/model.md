# MODEL: Meta 모델을 사용하는 Gatekeeper Agent

상태: 일부 구현. OpenAI-compatible Responses API provider와 local HTTP fixture 검증이 있다. 실제 Meta Model API 연결·모델 동작 평가는 아직 실행되지 않았다.

## 목적과 범위

Gatekeeper Agent는 로그인 사용자가 기여한 안전 정규화 trace와 기존 URL/task 메모리를 비교할 때 Meta 모델을 사용해 의미상 중복, 최신성 신호, 새로운 절차 여부를 구조화된 decision candidate로 제안한다. 이 프로젝트의 해커톤 산출물은 Meta 모델을 실제 제품 핵심 흐름에 통합해야 한다. 단순히 README에 이름을 언급하거나 데모와 무관한 채팅 endpoint를 호출하는 것으로 대체하지 않는다.

Meta 모델 출력만으로 public memory를 쓰지 않는다. 인증, 동의, URL scope, secret/PII 검사, replay evidence, version transaction은 서버가 결정적으로 검증한다. Meta 모델은 정책이 허용한 structured JSON decision을 제안하고, Gatekeeper policy engine이 publish·merge·supersede·retire·human review 중 가능한 상태를 확정한다. 모델은 [`curator.md`](curator.md)의 읽기 도구로 기존 메모리를 조회하고, 결정 도구의 인수로 이 structured decision을 낸다. 도구는 서버가 검증하며 모델에 게시 권한은 없다.

## 입력 계약

모델 입력은 G0/G1 정규화 이후에만 생성한다. 허용되는 필드는 `canonical_url_pattern`, `task_category`, `intent_key`, normalized action/result category, observed trait enum, 기존 활성 memory의 sanitized summary, verification timestamp bucket, aggregate replay outcome이다. Gatekeeper는 전체 URL, query, account ID, prompt, page text/HTML, user-entered text, cookie, screenshot, selector, trace 원문을 Meta API에 보내면 안 된다.

모델 호출 직전 payload를 schema validate하고 redaction sentinel scan한다. model adapter는 해커톤 환경에서 Meta Model API 또는 Meta open-weight model endpoint를 통해 요청을 수행한다. 구체 모델 ID는 사용 가능한 공식 모델과 참가 계정 권한을 확인한 뒤 배포 설정으로 선택하고 소스에 credentials나 endpoint secret을 하드코딩하지 않는다.

## 출력 계약

모델은 고정 schema의 candidate를 반환한다. 예상 필드는 `candidate_decision` enum, `confidence_band`, `reason_codes[]`, `compared_memory_version`, `evidence_needed[]`다. 허용 decision enum은 `duplicate_support`, `merge_evidence`, `create_new`, `supersede`, `retire`, `needs_human_review` 중 하나다. 설명이 꼭 필요한 경우 제한 길이의 sanitized summary를 별도 필드로 받고, 이를 사용자 지시나 실행 가능한 명령으로 취급하지 않는다.

JSON parse 실패, enum 밖 값, candidate scope 이탈, 낮은 confidence, schema version mismatch는 fail closed 처리한다. 서버는 모델의 임의 SQL/API 호출, browser action, shell command, publish 권한을 주지 않는다. 고정 replay와 deterministic policy가 불합격이면 모델이 `publish`를 주장해도 승인할 수 없다.

## 운영·관측 계약

각 요청은 provider family, 실제 model ID/version, prompt schema version, policy version, 요청·응답 digest, latency, retry count, 최종 structured decision을 기록한다. 원문 payload, secret, private prompt, chain-of-thought는 저장하지 않는다. 동작 재현에 필요한 digest와 model version은 audit용으로 보관한다.

timeout, quota exhaustion, 429/5xx, invalid model response는 memory state를 변경하지 않는다. retry는 idempotency key를 유지한다. 반복 실패는 작업을 재시도하거나 `needs_human_review`/provider unavailable 상태로 분리할 수 있지만 검증되지 않은 후보를 게시하지 않는다. 비용과 rate quota는 운영 제한으로 측정하며 조용히 비용 상한을 넘기지 않는다.

HTTP provider 설정은 `MODEL_API_KEY`를 비어 있지 않은 printable ASCII bearer 값으로만 받는다. 공백, 제어 문자, 비 ASCII 값은 요청을 보내기 전에 거부하고 오류에는 키 값이나 일부를 포함하지 않는다. credential은 Authorization 헤더로만 전달하고 JSON body, audit, 일반 로그에는 넣지 않는다.

## 요구사항과 테스트

| ID | 요구사항 | 합격 기준 | 관련 테스트 |
|---|---|---|---|
| F-MODEL-01 | 핵심 판정에 Meta 모델 사용 | 데모 실행에서 실제 provider/model ID가 model trace에 기록되고 Gatekeeper decision에 연결됨 | MODEL-01, HOST-01~04, ACC-SERVER-01 |
| F-MODEL-02 | 입력 최소화 | Meta endpoint로 가는 payload가 허용 구조만 포함하고 sentinel/raw URL/private content가 0건 | MODEL-02, MODEL-07, DB-15, SEC-A |
| F-MODEL-03 | 구조화된 출력 | schema-valid enum candidate만 policy engine에 전달 | MODEL-03, GK-39~48 |
| F-MODEL-04 | 모델 불가/출력 오류 fail closed | timeout, invalid JSON, quota/HTTP error는 active memory/catalog를 바꾸지 않음 | MODEL-05~06, GK-38, DB-11 |
| F-MODEL-05 | 결정 추적성 | model ID, policy/schema version, input/output digest가 audit event와 연결되고 raw reasoning은 없음 | MODEL-08, GK-48, DB-16 |
| F-MODEL-06 | 모델 권한 최소화 | Meta 모델은 decision candidate만 생성하며 직접 DB publish, 브라우저 조작, 외부 쓰기를 할 수 없음 | MODEL-04, SEC-B, SEC-E |
| F-MODEL-07 | provider credential 격리 | 잘못된 API key 형식은 송신 전 거부되고 credential 값이 body·오류·로그에 나타나지 않음 | MODEL-07, MODEL-10, SEC-I07 |

해커톤 데모는 Gatekeeper의 실제 URL/task 중복 판정 또는 stale memory 갱신 흐름에 모델 응답이 사용되는 장면을 포함한다. 단순 mock 응답 테스트는 provider contract 검증용이며, 실제 Meta 모델 연결의 대체물이 아니다. 제품 정확도와 개인정보 안전성은 [`gatekeeper.md`](gatekeeper.md), [`privacy.md`](privacy.md), [`../test/feature-acceptance.md`](../test/feature-acceptance.md) 기준도 함께 통과해야 한다.
