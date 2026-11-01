# 기능 테스트 설계

형식: `ID | 사전조건 | 동작 | 기대 결과`. 모든 케이스는 합성 데이터와 loopback 환경에서 실행한다.

계층을 가로지르는 장애 주입, 오류 응답과 복구는 [`failure-matrix.md`](failure-matrix.md)의 `FAIL-01~38`을 공통 기준으로 삼는다. 아래 개별 ID는 컴포넌트 동작을, FAIL ID는 장애 전후 연결 상태와 중복/유실 방지를 검증한다.

관리자 전용 이메일 인증·가입자/기여 조회·Muse 검수는 [admin.md](admin.md)의 `ADMIN-01~32`, 운영 수용은 `ADMIN-PROD-01~07`을 따른다. 자동 통과·추가 회귀·실메일 사람 확인 상태를 해당 문서에서 구분한다.

## CORE — `wam-core` (L0)

| ID | 동작 | 기대 |
|---|---|---|
| CORE-01 | 유효한 Contribution v1 파싱 | 성공, 필드 정규화됨 |
| CORE-02 | 알 수 없는 `schema_version` | 거부, 사유 코드 |
| CORE-03 | enum 밖 값 (`task_category`, `outcome` 등) | 거부 |
| CORE-04 | 필드/전체 크기 상한 초과 (2048 byte 등) | 거부, 부분 저장 없음 |
| CORE-05 | host/URL 정규화: 대문자, 포트, 경로, 쿼리, 사용자정보(`user@host`) | host는 정규화되고 query/fragment/userinfo는 제거됨 |
| CORE-06 | 이상한 호스트: IP, localhost, 내부 TLD, 퓨니코드 혼동 문자 | 정책대로 거부 또는 정규화 (IP/내부 거부) |
| CORE-07 | 호스트별 도구 이름 → 정규화 액션 매핑 (Arise, Playwright MCP 등) | 표대로 매핑, 미지 도구는 `other` |
| CORE-08 | 액션 시퀀스 정규화: 연속 중복 제거, 관측 액션 압축 | 결정적 표준형 |
| CORE-10 | 센티널 키/토큰/JWT/비공개 키 블록이 모든 문자열 필드에 존재 | 해당 항목 폐기(마스킹 아님), 플래그 기록 |
| CORE-11 | 이메일/전화/신분증/카드번호(Luhn) | 폐기 |
| CORE-12 | 로컬 경로, 내부 IP/호스트명 | 폐기 |
| CORE-13 | URL query/fragment에 token, 검색어, 개인 식별값 포함 | query/fragment는 저장·로그·응답에 남지 않음 |
| CORE-14 | 센티널 변형: base64, URL 인코딩, 16진수, 제로폭 삽입, 유니코드 유사문자, 대소문자 | 정책대로 폐기 (미탐지 0) |
| CORE-15 | 분할 센티널 (여러 필드에 쪼갬) | 결합 검사로 탐지 |
| CORE-16 | 정상 값(allow-list enum, 안전 토큰) | 통과, 오탐 없음 |
| CORE-17 | 속성 기반(proptest): 임의 문자열 + 센티널 삽입 | redaction 출력에 센티널 미포함 |
| CORE-18 | 퍼징: redaction과 파서 | 패닉/무한루프/과도한 메모리 없음 |
| CORE-19 | ReDoS 입력 (병적 패턴, 매우 긴 문자열) | 시간 상한 내 종료 (예: 입력당 ≤ 50ms) |
| CORE-20 | 지문(fingerprint): 같은 의미 시퀀스의 순서 차이/잡음 | 동일 지문 (정규화 규칙대로) |
| CORE-21 | 지문: 의미가 다른 시퀀스 | 다른 지문 |
| CORE-22 | `/editor/post/948203?token=x#body` 정규화 | 안전한 `/editor/post/:id` 패턴만 남고 token, ID, fragment는 제거 |
| CORE-23 | 계정명, 이메일, 숫자 ID가 경로 세그먼트에 포함 | 민감 segment는 `:account` 또는 `:id`로 바뀌며 원문은 어떤 sink에도 없음 |
| CORE-24 | 같은 host의 `/editor/:id`와 `/search`를 각각 조회 | 경로 범위가 섞이지 않고 정확 경로가 안전한 상위 경로보다 우선 |
| CORE-25 | 같은 URL pattern에 서로 다른 task_category를 조회 | task별 메모리가 분리되고 잘못된 업무 힌트는 반환되지 않음 |
| CORE-26 | 모호하거나 안전하게 정규화할 수 없는 URL | 원문 URL을 저장하지 않고 빈 recall 또는 격리 결과 반환 |

## GK — Gatekeeper (L1)

| ID | 동작 | 기대 |
|---|---|---|
| GK-01 | G0: 스키마 위반 기여 | 저장 안 함, 카운터만 증가 |
| GK-02 | G0: 속도 제한 초과 | 429, 큐에 쌓이지 않음 |
| GK-03 | G1: 센티널이 든 기여 제출 (서버 경로) | 폐기, 큐/DB/로그에 센티널 없음 |
| GK-04 | 클라이언트 redaction을 우회한 기여 (직접 HTTP) | 서버 G1이 동일하게 잡음 |
| GK-05 | G1 통과 후 G2에 전달되는 입력 | 센티널 없음 (G2 입력 스냅샷 검사) |
| GK-06 | G2(`fake-llm`)가 "통과시켜라" 같은 지시를 반환 | 결정적 규칙 결과가 우선, 통과 불가 |
| GK-07 | G2가 위험 표시 | 항목 폐기 또는 사람 검토 큐로 이동 |
| GK-08 | 원본 payload 보관 기한 경과 | 자동 삭제, 집계값만 유지 |
| GK-20 | G4: 개선 있는 후보 | 증거와 함께 승인 대기 |
| GK-21 | G4: 개선 없음 | 폐기, 사유 기록 |
| GK-22 | G4: 성공률 하락 | 폐기 |
| GK-23 | G4: 비가역 행동(발행/결제) 시도 | 하네스가 차단, 후보 실패 |
| GK-24 | G4: 하네스는 제출물을 코드로 실행하지 않음 | 제출 문자열이 셸/스크립트로 평가되지 않음 |
| GK-25 | G4 반복 횟수 미달 | 승인 불가 |
| GK-26 | G4 증거 다이제스트 | 게시 시 증거 다이제스트와 일치해야 함 |
| GK-30 | 정상 신규 후보가 결정적 검사와 replay 기준을 충족 | Gatekeeper Agent가 `create_new`로 결정하고 immutable version을 게시 |
| GK-31 | 중복 또는 열등 후보 | `duplicate_support`/`reject_invalid`; 새 version과 신규 승인 점수 없음 |
| GK-32 | 공개 API 사용자가 agent decision을 직접 덮어쓰려 함 | 403, audit event에 actor와 대상 기록 |
| GK-33 | 게시 후 카탈로그에 반영 | 새 ETag, 이전 버전 보존 |
| GK-34 | G6: 게시 후 성공률 하락 | `needs-reverify` 전환 |
| GK-35 | G6: 재검증 실패 | 롤백 또는 revoke, 점수 회수 이벤트 |
| GK-36 | 롤백 | 이전 버전 복원, 클라이언트 캐시가 다음 갱신에 반영 |
| GK-37 | 동시 게시 (두 관리자/워커) | 직렬화, 중복 버전 없음 (락) |
| GK-38 | 워커 재시작/장애 | 작업이 유실되거나 중복 게시되지 않음 (멱등) |
| GK-39 | 기존 URL/task와 같은 절차의 새 contribution | Agent가 근거를 비교해 support merge 또는 중복 거절, 게시 복제 없음 |
| GK-40 | 새 URL pattern/task intent에 유효한 성공 경로 | Agent가 `create_new` 판단 후 새 record 생성 |
| GK-41 | 같은 URL/task에서 검증된 더 나은 절차 | Agent가 replay 성능을 확인한 뒤 `supersede`; 기존 version은 immutable |
| GK-42 | 게시 절차가 사이트 변경으로 오래됐지만 실패 증거 없음 | 즉시 폐기하지 않고 `needs-reverify`로 표시 |
| GK-43 | 기존 절차의 실패 증가와 대체안의 replay 성공 | Agent가 evidence와 policy version을 저장하고 `supersede` 또는 `retire` 결정 |
| GK-44 | 기존/신규 안내가 상충하거나 confidence 미달 | 공개 catalog 변화 없이 `needs_human_review` 예외 큐로 이동 |
| GK-45 | 새 제출이 URL/task 범위를 넓혀 다른 route에 지침 적용 시도 | Agent decision은 거부되고 교차 범위 게시 없음 |
| GK-46 | 악성 힌트가 정상 trace에 섞임 | 제출 내용은 data로만 취급, 고정 보안 게이트가 막고 실행하지 않음 |
| GK-47 | `retire` 또는 `supersede` 뒤 과거 version 확인 | public recall은 현재 version만 반환; audit/rollback에는 과거 version 보존 |
| GK-48 | agent 재시도·모델 timeout·human appeal | 재시도는 멱등; appeal override는 권한·근거·audit trail이 있어야 함 |

## MODEL — Meta 모델 provider와 구조화 판정 (L1/L2)

| ID | 동작 | 기대 |
|---|---|---|
| MODEL-01 | Gatekeeper worker에서 Meta model provider 선택 | 허용된 Meta Model API 또는 Meta open-weight endpoint가 설정되고 provider/model ID가 traceable |
| MODEL-02 | model adapter로 입력 payload 전달 | sanitized URL pattern/task/action summary만 포함; secret, raw URL, page text, user prompt는 없음 |
| MODEL-03 | 유효한 structured JSON decision 응답 | 알려진 enum과 schema version만 policy engine으로 전달 |
| MODEL-04 | model 출력에 prompt injection, SQL, shell, 외부 write 지시 포함 | 일반 텍스트 data로 취급; 도구/DB 권한이 없고 deterministic gate가 차단 |
| MODEL-05 | timeout, 429/quota, 5xx, invalid JSON, enum 밖 decision | fail closed; active version/catalog/score event 변화 없음 |
| MODEL-06 | 동일 request retry와 worker 재시작 | idempotency key 기준 단일 decision/publish; 추가 비용·중복 event를 추적 |
| MODEL-07 | model request/response log와 audit sink 검사 | sentinel, 원문 URL, private trace, chain-of-thought, credential 0건 |
| MODEL-08 | 결정 결과의 audit record 확인 | provider/model ID, prompt schema/policy version, input/output digest, reason code가 남고 raw reasoning은 없음 |
| MODEL-09 | 해커톤 demo에서 실제 Meta model로 URL/task dedup 또는 stale decision | live provider/model ID가 trace에 기록되고 structured decision이 안전 gate 이후 최종 상태에 연결; fake provider만 사용하면 실패 |
| MODEL-10 | `MODEL_API_KEY`가 빈 값·공백·제어 문자·비 ASCII 포함 | provider 생성 시 안전하게 거부; 오류에 credential 값/부분값 0건이며 HTTP 요청 미발생 |
| MODEL-11 | `WAM_LLM_REASONING_EFFORT` 미설정·허용값·허용 밖 값 | 설정하면 `reasoning.effort`만 추가로 전송하고 미설정이면 필드가 없음; 허용 목록 밖 값은 요청 전 거부하며 오류에 입력값 0건 |
| MODEL-12 | 추론 모델이 출력 한도를 추론에 소진해 `status: incomplete`로 응답 | 일반 malformed와 구분된 사유(`incomplete (max_output_tokens)`)로 보고; provider가 보낸 reason 문자열은 에코하지 않고 memory state 변화 없음 |
| MODEL-13 | HTTP provider 비용 보고 | `WAM_LLM_PRICE_*` 셋이 모두 설정되면 `usage`(입력·캐시 입력·출력 토큰)로 계산한 `cost_usd`를 turn 메타데이터에 넣고, 설정이 없거나 `usage`가 없으면 보고하지 않는다. 일부만 설정·숫자 아님·범위 밖 값은 요청 전 거부하며 오류에 값을 싣지 않는다 |
| MODEL-14 | Meta가 문서화한 결제 오류 | 402 `billing_error`(`billing_not_configured`)는 소진 문구가 없어도 쿼터·크레딧 소진으로 분류되어 job을 실패가 아니라 대기로 보낸다 |
| MODEL-15 | HTTP provider의 상태·본문 분류 | 429(소진 신호 없음)·408·모든 5xx(본문이 JSON이 아니어도)는 `Unavailable`이고 `Retry-After`(초·HTTP 날짜)를 읽는다. 소진 신호(402, `insufficient_quota`)는 소진이 우선, 그 밖의 4xx는 일반 실패이며 오류 문자열에 provider 본문·키가 없다 (`wam-gatekeeper/tests/provider.rs`, [`provider-outage.md`](provider-outage.md)) |
| MODEL-16 | 전송 실패 분류 | 연결 거부, 응답 없이 멈춤(timeout), 본문 중 끊김은 `Unavailable`이고 200인데 JSON이 아니거나 한도 초과인 응답은 일반 실패다 (`tests/provider.rs`, [`provider-outage.md`](provider-outage.md)) |
| MODEL-17 | curator가 일시 장애를 만남 | `Outcome::Unavailable`로 끝나고 결정·저장소 변경이 없으며 감사 outcome이 `provider_unavailable: …`다. 앞선 턴의 비용은 남고 `Failed`·`Timeout`·`QuotaExhausted`의 결과는 그대로다 (`tests/curator_loop.rs`, [`provider-outage.md`](provider-outage.md)) |
| MODEL-18 | HTTP provider의 거부 분류 (#168) | 401·403·404는 소진 신호가 없으면 `Rejected`다(Meta 형식의 401·404 본문, 본문 없음·HTML·JSON 403 모두). 소진 신호는 상태 코드보다 우선하고 400·413·422는 일반 실패이며 오류 문자열에 provider 본문·키가 없다 (`tests/provider.rs`, [`provider-outage.md`](provider-outage.md)) |
| MODEL-19 | curator·독립 worker가 거부를 만남 (#168) | `Outcome::ProviderRejected`로 끝나고 결정·저장소 변경이 없으며 감사 outcome이 `provider_rejected: …`다. 독립 engine은 일반 실패처럼 재시도한다 (`tests/curator_loop.rs`, `tests/worker.rs`) |

## DEDUP — 신규성/중복 (L0/L1)

| ID | 동작 | 기대 |
|---|---|---|
| DEDUP-01 | 동일 지문 기여 100건 | 레코드 1개, `support_count` 증가 |
| DEDUP-02 | 유사 시퀀스 (임계 이상, 열등) | 폐기, 카운트만 증가 |
| DEDUP-03 | 유사 시퀀스 (더 짧은 성공 경로) | 개정안 Candidate (`supersedes`) |
| DEDUP-04 | 새 `observed_trait`/pitfall 해결 경로 | 새 Candidate |
| DEDUP-05 | 기존과 반대 지시 (충돌) | 격리 후 G4로 우열 판정 |
| DEDUP-06 | 같은 사이트, 다른 `task_category` | 별개 레코드 |
| DEDUP-07 | 다른 사이트, 같은 시퀀스 | 별개 레코드 (도메인 간 병합 금지) |
| DEDUP-08 | 실패 기여(outcome=failed)만 존재 | 게시 후보 아님, pitfall 통계로만 사용 |
| DEDUP-09 | 합성 라벨 세트 (동일/유사/신규/충돌) | 정확도 기준 이상 |
| DEDUP-10 | 임계값 경계 케이스 | 결정적이고 문서화된 결과 |
| DEDUP-11 | 같은 기여자의 반복 제출 | support 중복 집계 안 됨 |
| DEDUP-12 | 대량 기여(10만 건) | 합리적 시간 내 처리, 메모리 상한 |
| DEDUP-13 | 같은 host/task의 서로 다른 canonical route | exact route와 안전한 상위 route만 scope 규칙에 따라 비교; sibling route 자동 merge 없음 |
| DEDUP-14 | 같은 URL pattern에서 다른 task_category 제출 | 별도 active scope, memory와 score attribution 교차 없음 |
| DEDUP-15 | last_verified_at만 오래됐고 실패 evidence는 없음 | Agent가 곧바로 삭제하지 않고 `needs-reverify`로 둠 |
| DEDUP-16 | 동일 route에 새 UI fingerprint와 독립 failure/success evidence | 격리 replay 비교 뒤에만 supersede 또는 retire, evidence 없는 교체는 거부 |
| DEDUP-17 | v1 → v2 supersede 후 v2 replay failure | immutable v1을 재활성화/rollback하고 audit/reversal event 기록 |
| DEDUP-18 | scope 모호성, confidence 미달, 상충하는 지침 | 공개 catalog를 변경하지 않고 `needs_human_review` 예외 큐로 이동 |

## API — 서버 (L1/L2)

| ID | 동작 | 기대 |
|---|---|---|
| API-01 | `GET /v1/catalog` 익명 | 승인된 레코드만, ETag, 크기 상한 |
| API-02 | `If-None-Match` 재요청 | 304 |
| API-03 | candidate/revoked 레코드 | 카탈로그에 없음 |
| API-04 | `POST /v1/memories/recall`에 URL과 task 전송 | 최대 3개, 각 ≤ 400 토큰, URL/task 범위 일치 |
| API-05 | 관련 없는 도메인 | 빈 결과 (교차 주입 없음) |
| API-06 | `GET /v1/memories/{id}` | 상세/증거, 존재하지 않으면 404 |
| API-07 | 잘못된 host 파라미터 (IP, 경로, 인젝션 문자열) | 400 |
| API-08 | OpenAPI 스키마와 실제 응답 일치 | 계약 테스트 통과 |
| API-09 | 서버 일시 장애 | 클라이언트는 마지막 유효 캐시 유지 |
| API-10 | 승인된 빈 릴리스 | 번들 시드를 덮어씀 (기존 규칙) |
| API-11 | `/web-memory/v1/*` 호환 어댑터 | 기존 클라이언트 계약 통과 |
| API-12 | CORS/보안 헤더 | 정책대로 (카탈로그만 허용) |
| API-20 | `POST /v1/contributions` 유효, 디바이스 키 | 202, 큐 적재 |
| API-21 | 키 없음/잘못된 키/해지된 키 | 401 |
| API-22 | 동의(consent) 없는 디바이스 | 거부 |
| API-23 | 중복 제출(같은 `client_nonce`) | 멱등 처리 |
| API-24 | 페이로드 크기 초과 | 413 |
| API-25 | 잘못된 JSON, 중복 키, 깊은 중첩 | 400, 서버 안정 |
| API-26 | `POST /v1/receipts/{id}/outcome` 유효 | owner-bound receipt 기록, `recorded:true` |
| API-27 | 같은/다른 결과로 영수증 재보고 | 같은 결과는 `recorded:false`; 다른 결과는 409; score event 중복 없음 |
| API-28 | 남의 영수증으로 보고 | receipt 존재 노출 없이 404, score event 없음 |
| API-29 | 만료된 영수증 | 404, score event 없음 |
| API-30 | 익명 recall 응답 | public hint만 반환, `receipt_id`와 score event는 없음 |
| API-31 | 로그인 recall | session/device token에서 consumer를 정하고 해당 version에 receipt 바인딩 |
| API-32 | contribution body에 타인 `contributor_user_id` 포함 | body 값 거부 또는 무시, session 소유자만 기여자로 저장 |
| API-33 | 사용자 A가 사용자 B의 contribution/dashboard 조회 | 개인정보 노출 없이 404/403, B의 상태는 변경되지 않음 |
| API-34 | 로그인 및 consent 없는 contribution POST | 401/403, contribution row와 queue job 모두 생성되지 않음 |
| API-35 | contribution nonce 재전송 | 같은 사용자의 동일 결과를 반환, 두 번째 job/점수 없음 |
| API-36 | 인증된 다른 사용자의 successful receipt outcome | 유효 version/계정 기준으로 한 번 처리, 작성자에게만 정책 점수 후보 생성 |
| API-37 | guide host template 조회 | 버전·endpoint·copy text 반환, token, 개인 ID, secret은 없음 |
| API-38 | token revoke 후 propose/outcome | 즉시 401, anonymous recall만 별도 정책에 따라 가능 |
| API-39 | URL이 access log/error trace로 전달되는지 검사 | 민감 query와 path ID를 redaction; request body를 error로 echo하지 않음 |
| API-40 | `GET /v1/me/memories/analytics` | 본인 authored version만 90일 이내 aggregate 반환; anonymous/other-account hit 수는 distinct people이 아닌 request count; self-read 제외, identity/receipt/raw URL 없음 |
| API-41 | local review API의 별도 인증과 목록 응답 | contributor token은 401; review token만 허용; 목록은 contributor ID와 원문 없이 상태 집계를 반환 |
| API-42 | local review claim과 approve/reject 전이 | generation 단조 증가; 중복 claim과 오래된 decision은 409; 재시작 뒤 상태와 active queue가 복구됨 |
| API-43 | recall receipt/outcome 및 공개 지표 | 로그인 hit만 30일 receipt 발급; owner-only outcome은 멱등; aggregate에 account/receipt/memory ID/URL 없음 |
| API-44 | shared IP/account rate limit | 익명은 trusted peer IP, IPv6 /64, 로그인은 account+shared-IP bucket 모두 적용; 429, `Retry-After`, `RateLimit-*` CORS metadata |
| API-45 | `GET /v1/me/usage` | owner-only account quota와 최근 30일 admitted request/unit aggregates; `no-store`; URL/IP/body/token/receipt 없음 |
| API-46 | anonymous/private cache headers | anonymous recall과 authenticated/error responses 모두 HTTP `no-store`; WAM CLI/MCP만 `X-WAM-Client-Cache` opt-in 시 별도 local cache 허용 |

## LINK — 디바이스 연결 (L1/L3)

| ID | 동작 | 기대 |
|---|---|---|
| LINK-01 | `link/start` → 승인 URL과 `user_code` | 코드 형식/만료 시간 올바름 |
| LINK-02 | 사용자가 승인 → `poll` | 디바이스 키 발급 (한 번만 반환) |
| LINK-03 | 승인 전 `poll` | pending |
| LINK-04 | 만료된 코드 | 거부 |
| LINK-05 | 거부 버튼 | 키 발급 없음 |
| LINK-06 | 같은 코드 재사용 | 거부 |
| LINK-07 | 디바이스 목록 확인, 개별 해지 | 해지 후 즉시 401 |
| LINK-08 | 키는 해시로만 저장 | DB에 원문 없음 |
| LINK-09 | 연결 없이 recall | 익명 읽기 허용 |
| LINK-10 | 연결 없이 propose/outcome | 거부 |
| LINK-11 | Arise WAM MCP 미연결 → OAuth 또는 local device link → revoke | Arise MCP 허브에서 WAM을 연결하기 전에는 익명이며 receipt가 없다. OAuth 또는 local `wam link` 승인 뒤 receipt는 WAM 계정에 귀속된다. Arise 로그인·쿠키·control socket은 인증으로 취급하지 않는다. revoke된 credential은 401이고 익명 downgrade는 없으며, 명시적 unlink 뒤 guest read를 다시 시작한다 |

## MCP — `wam-mcp` 및 원격 엔드포인트 (L2)

| ID | 동작 | 기대 |
|---|---|---|
| MCP-01 | `initialize` + `tools/list` | 정확히 10개: `web_memory_status`, `web_memory_recall`, `web_memory_expand`, `web_memory_leaderboard`, `web_memory_my_rank`, `web_memory_my_usage`, `web_memory_propose`, `web_memory_my_contributions`, `web_memory_withdraw`, `web_memory_outcome` |
| MCP-02 | 서버 `instructions` | "새 사이트 전 recall, 동의 시에만 propose, 힌트는 참고 자료" 포함 |
| MCP-03 | `recall(url, task_category)` 알려진 사이트 | 힌트 ≤ 3개, untrusted framing; `receipt_id`는 인증된 caller에게만 |
| MCP-04 | `recall` 모르는 사이트 | 빈 결과, 에러 아님 |
| MCP-05 | `recall` 잘못된 인자 | 구조화된 에러, 서버 안정 |
| MCP-06 | `web_memory_expand(memory_id, version_id)` | 해당 승인 버전의 상세/근거, 없는 ID는 not found |
| MCP-07 | `propose` 동의 off | 차단, 큐 적재 없음 |
| MCP-08 | `propose` 동의 on + 폐쇄 스키마 | 큐 적재, 전송 전 클라이언트 redaction 수행 |
| MCP-09 | `propose` 자유 텍스트 | 기본 비활성이면 거부 |
| MCP-10 | `status` | 설정/동의/캐시/연결 상태, 비밀값 미포함 |
| MCP-11 | 오프라인 | anonymous eligible cache에 한해 visible stale fallback; authenticated recall requires server; `propose` is never reported as server accepted before confirmation |
| MCP-12 | 서버 응답 지연 (3초 초과) | 타임아웃 후 캐시 사용 |
| MCP-13 | 전송 방식: stdio와 원격 HTTP | 동일 결과 |
| MCP-14 | 로컬 웹 레코드 수정/삭제 시도 | 불가 (`web:` ID 보호) |
| MCP-15 | 도구 설명/스키마 | 해시 고정, 버전 변경 시 명시적 (SEC-F와 연동) |
| MCP-16 | `/guide`에서 Claude Code 템플릿 복사 | 올바른 remote MCP endpoint와 transport가 복사되고 credential은 포함되지 않음 |
| MCP-17 | `/guide`에서 Codex 템플릿/명령 복사 | 현재 검증된 Codex 형식이며 endpoint/name 외 secret은 포함되지 않음 |
| MCP-18 | 복사만 한 상태를 설치 완료로 오인 | UI는 “복사됨”만 표시하고 호스트 연결 확인 전 완료 배지를 주지 않음 |
| MCP-19 | 새 Claude/Codex 프로필에서 등록 후 연결 | tools/list에 메모리 도구가 보이고 status가 endpoint/auth 상태를 정확히 표시 |
| MCP-20 | memory MCP와 Chrome browser MCP를 함께 등록 | 각 서버의 tool set이 구분되어 둘 다 호출 가능; 한 서버가 다른 것을 설치했다고 표시하지 않음 |
| MCP-21 | task-aware URL recall | exact route/task memory 우선, host가 다르거나 task가 다르면 반환하지 않음 |
| MCP-22 | 익명 연결에서 propose/outcome 호출 | 권한 오류; queue 및 score ledger 변경 없음 |
| MCP-23 | OAuth 로그인 후 propose/outcome | 서버가 인증 계정·scope·동의·영수증 소유권을 검사한 뒤 한 번 처리 |
| MCP-24 | Arise MCP Hub remote/local WAM 경로와 일반 MCP가 같은 catalog 사용 | 같은 URL/task/version/keyword 순위와 익명·연결 계정 receipt/권한 정책을 반환 (`CUST-01~06`, `HOST-05`, `HOST-10~11`). Arise browser의 내부 control socket 직접 호출은 포함하지 않음 |

## SCORE — 영수증, 점수, 리더보드 (L1/L3)

| ID | 동작 | 기대 |
|---|---|---|
| SCORE-01 | 신규 메모리 승인 | 저자 +20 이벤트 |
| SCORE-02 | 다른 로그인 사용자가 recall 후 success 보고 | 저자 +2 (소비자-레코드-일 1회) |
| SCORE-03 | 같은 소비자가 같은 날 반복 | 추가 점수 없음 |
| SCORE-04 | 저자 본인 호출 | 점수 없음 |
| SCORE-05 | recall만 하고 결과 보고 없음 | 점수 없음 |
| SCORE-06 | outcome=failed | 점수 없음 |
| SCORE-07 | revoke/롤백 | 해당 레코드 사용 점수 회수, 저자 감점 |
| SCORE-08 | 리더보드 opt-out 사용자 | 순위에 노출 안 됨, 닉네임 비공개 |
| SCORE-09 | 기간별 순위(주간/전체) | 이벤트 합계와 일치 |
| SCORE-10 | 동점 처리 | 문서화된 결정적 규칙 |
| SCORE-11 | 점수 이벤트 멱등 | 재처리해도 중복 지급 없음 |
| SCORE-12 | 신규 계정 가중치 | 규칙대로 낮은 가중 |
| SCORE-13 | 독립 소비자 수 체감 곡선 | 상한/체감 적용 |
| SCORE-14 | 점수 재계산 | 이벤트 로그에서 재현 가능 |
| SCORE-15 | 익명 사용자가 같은 memory를 반복 recall | recall은 정책상 허용돼도 점수는 0, receipt와 user-level event 모두 없음 |
| SCORE-16 | 소비자가 로그인하지 않고 success를 주장 | server rejects outcome; author points unchanged |
| SCORE-17 | 로그인 소비자가 recall만 하고 outcome 미보고 | 기여자 점수 이벤트 없음 |
| SCORE-18 | 작성자 본인이 다른 device에서 recall + success | 같은 account로 식별해 self-use 점수 없음 |
| SCORE-19 | client가 consumer/author ID 또는 points를 임의 전송 | identity/score body fields 신뢰하지 않고 서버 값만 사용 |
| SCORE-20 | receipt outcome 재생, 만료, 타인 receipt 사용 | replay/expired는 추가 event 없음; 타인 영수증은 거부 |
| SCORE-21 | 다른 로그인 계정이 실제 성공 보고 | 승인된 version, distinct consumer, quota 통과 시 author에게 policy event 1건 |
| SCORE-22 | 한 계정이 여러 device 또는 호출 자동화를 사용 | 계정 단위 quota와 rate limit이 적용되어 반복 점수 증가를 제한 |
| SCORE-23 | version revoke 또는 확정된 abuse rollback | 기존 지급을 삭제하지 않고 연결된 reversal event 추가 |
| SCORE-24 | 공개 leaderboard와 dashboard 조회 | opt-in handle/합계만 공개; anonymous, opt-out, 이메일, receipt ID 미노출 |
| SCORE-25 | 본인 score-events 내역, pagination, 재시작, 인증/scope | 본인 지급 event만 최근 append 순으로 조회된다. 제한은 1~100이고 타인/없는 cursor는 동일 404다. 새 event가 추가되어도 다음 page가 중복되지 않으며 consumer identity/receipt/idempotency key는 미노출이다. 파일 재시작 뒤 내역과 순서가 유지된다. |

## WEB — 웹 UI (L3, Playwright)

| ID | 동작 | 기대 |
|---|---|---|
| WEB-01 | 홈 | 문제 설명, 수치 표, 시작하기 링크 렌더링 |
| WEB-02 | GitHub 로그인 (테스트용 OAuth 대역) | 세션 생성, 로그아웃 동작 |
| WEB-03 | `/link?code=` 승인 흐름 | 로그인 후 승인 → 디바이스 연결 표시 |
| WEB-04 | 잘못된/만료된 코드 | 명확한 에러 |
| WEB-05 | `/dashboard` (기존 `/me` 호환 가능) | 내 점수, 기여 상태(대기/승인/반려 사유), 디바이스 |
| WEB-06 | 디바이스 해지 | 목록에서 사라지고 해당 키 즉시 무효 |
| WEB-07 | `/leaderboard` | 주간/전체 전환, opt-in 사용자만 |
| WEB-08 | `/memories`, `/memories/{host}` | 승인된 힌트만 표시, 비밀값 없음 |
| WEB-09 | `/guide` | 비회원이 호스트별 메모리 MCP 스니펫과 CLI 명령을 복사하고 URL+키워드 테스트 호출을 볼 수 있다. Chrome/browser MCP 설치와 선택적 tab-aware adapter 안내를 별도 단계로 표시한다. 자동 회수는 connector가 지원하는 local interface와 전송 전 정규화가 검증된 경우에만 지원하며, 복사만으로 설치 완료 처리하지 않는다 (`CUST-01~04`, HOST-03/04/09) |
| WEB-10 | `/admin` 익명 접근 | 로그인 요청 UI만 표시, 비공개 계정/기여 데이터 없음; 관리자 API는 401/403 (`admin.md` ADMIN-01~03, ADMIN-09) |
| WEB-11 | 관리자 review navigation | 관리자 접근·candidate 검수·게시 권한은 `admin.md` ADMIN-01~32에서 검증; 고객 세션은 관리자 API를 호출할 수 없음 |
| WEB-12 | 반응형, 접근성(키보드, 라벨) | 기본 점검 통과 |
| WEB-13 | 로딩/에러/빈 상태 | 각각 적절한 UI |
| WEB-14 | 닉네임 정책 (금칙어) | 거부 |
| WEB-15 | 동의 설정 토글 | 즉시 반영, 철회 시 큐 비움 |
| WEB-16 | 로그인/로그아웃과 세션 만료 | Secure session 생성, 만료 후 protected page에서 재로그인 요구 |
| WEB-17 | `/contribute` URL/task preview와 동의 | 제출 필드 미리보기와 명시 동의 후에만 POST; contributor는 세션 계정 |
| WEB-18 | 기여자 A의 dashboard | 본인 기여 상태·사유·memory version·score ledger만 표시 |
| WEB-19 | 기여자 A가 B의 contribution ID를 직접 방문 | B의 원문/상태/개인 정보 미노출 |
| WEB-20 | gatekeeper 정상 판정이 published로 전환 | agent decision과 audit가 끝난 뒤 catalog와 dashboard를 갱신 |
| WEB-21 | 중복·열등 기여 결과 표시 | 중복으로 설명, 신규 승인 점수 없음, 원본 공개 없음 |
| WEB-22 | 개선/폐기/사람 검토 상태 표시 | superseded, retired, needs-human-review를 구분하고 reason code 안내 |
| WEB-23 | 대시보드의 사용 이벤트 확인 | 타 로그인 계정의 성공 사용만 집계; caller 이메일이나 전체 URL은 숨김 |
| WEB-24 | opt-in/opt-out leaderboard | opt-in 후 handle/합계만 공개; opt-out 즉시 목록에서 제거 |
| WEB-25 | 기여 memory version별 분석 차트 | recall hit·detail view·explicit outcome·익명/other-account aggregate를 구분하고 API 집계와 일치; 일/주간 토글 및 표 대안 제공; 개인 식별정보 없음 |
| WEB-26 | account quota meter | account available units/burst/refill wait는 owner API와 일치하고 shared IP bucket count는 노출하지 않으며 accessible progress, unit, window를 표시 |
| WEB-27 | limit and service failure message | 429 wait comes from `Retry-After`; 503/unavailable is distinct from quota and zero-data; no automatic retry loop |
| WEB-28 | account usage trend | 30-day chart and table match aggregate API and keep API activity separate from authored-memory analytics and score; author impact chart supports daily/weekly grouping |
| WEB-29 | leaderboard rank trend integrity | show movement only with stored history; with no history, show current rank without invented delta; opt-out user is absent |
| WEB-30 | dashboard analytics/history 재조회에서 session 401 | summary/usage/contribution/score/analytics 전체 owner state와 CSRF를 비우고 재로그인 UI로 전환; 이전 계정 정보 잔류 0 |
| WEB-31 | accepted contribution POST 후 목록 refresh 실패 | 서버 receipt를 성공 근거로 표시하고 receipt row를 유지; refresh 실패는 별도 안내; GET 재시도 성공 후 목록 reconciles; 자동 또는 수동 중복 POST 0 |
| WEB-36 | analytics 요청 중 전체 dashboard 재조회 및 오래된 응답 도착 | 새 generation만 적용하고 busy/error를 초기화; 이전 응답/401이 새 상태를 변경하지 않으며 Update가 다시 활성화 |
| WEB-32 | dashboard 429/503 후 사용자 재조회 | 429는 Retry-After 대기, 503은 일시 서비스 장애로 표시; 오류를 0점/빈 기록이나 OAuth 설정 오류로 단정하지 않고 자동 재요청 0 |
| WEB-33 | Chrome/Firefox/WebKit 및 모바일 emulation의 guide/contribute/dashboard/login | viewport overflow 0, main/H1 유일, 표시 UI의 자동 WCAG A/AA 위반 0. 실제 Safari/VoiceOver·수동 WCAG 심사는 별도 |
| WEB-34 | pointer 없이 skip-link → main → host 선택 → 연결 명령 | hydration 완료 뒤 Tab/Enter/Space로 이동·선택 가능, focus와 aria-pressed 및 명령이 일치 |
| WEB-35 | 390px에서 키보드로 모바일 메뉴 열기·랭킹 이동 | aria-expanded가 실제 표시와 일치하며 첫 링크로 이동·라우팅 후 메뉴 닫힘, 키보드 trap 0 |

## PRIV — 동의, 철회, 보관 (L1)

| ID | 동작 | 기대 |
|---|---|---|
| PRIV-01 | 기여 기본값 | off |
| PRIV-02 | 동의 on → 기여 큐 적재 | 폐쇄 스키마만 |
| PRIV-03 | 철회 | 대기 큐 즉시 비움, 이후 업로드 없음 |
| PRIV-04 | 이미 시작된 request와 동의 철회 race | 분석은 끝날 수 있으나 commit 전 최신 consent 재확인; 철회가 먼저면 게시·점수 0, publish가 먼저면 별도 withdrawal job |
| PRIV-05 | 원본 payload 보관 기한 | 기한 후 삭제 확인 |
| PRIV-06 | 텔레메트리 동의와 독립 | 한쪽 변경이 다른 쪽에 영향 없음 |
| PRIV-07 | 전송 예시 JSON 표시 | 실제 전송 형식과 동일 |
| PRIV-08 | 희귀 조합 k-익명성 | 임계 미만은 공개 집계에서 제외 |
| PRIV-09 | 계정 삭제와 기여 귀속 철회 | 세션·기기가 해지되고 비공개 자료가 제거/비식별화된다. 독립 기여 근거가 남은 메모리만 유지하며 단독 근거 메모리는 재검증 또는 retire 전까지 recall에서 숨긴다. |

## MEM — 구조화 메모리 레이어 (테스트 L0/L1)

| ID | 동작 | 기대 |
|---|---|---|
| MEM-01 | 레코드 스키마: l0·l1·l2 누락, 알 수 없는 필드, 잘못된 `schema_version` | 거부, 부분 저장 없음 |
| MEM-02 | L0 한도: 키워드 9개 이상, 키워드 33자 이상, URL 패턴 5개 이상, 직렬화 1 KiB 초과 | 거부 |
| MEM-03 | 키워드 정규화: NFKC, 대소문자, 공백, 한국어·영어 혼합. URL, 이메일, 비밀값 형태, 문장 | 정규화된 토큰만 저장, 나머지 거부 |
| MEM-04 | L1 주입 한도: 힌트당 400 estimated tokens 초과, recall당 4개 이상 | 초과 거부 또는 잘림, 응답은 ≤3개 |
| MEM-05 | `expand`의 level 1, 2, 그 밖의 값 | L1/L2 반환, 나머지 거부, L2 ≤8,000자 |
| MEM-06 | kind별 필수 필드: workflow는 steps, site_note는 structure, gotcha는 pitfalls | 누락 시 거부 |
| MEM-07 | 같은 URL·task에서 kind가 다른 레코드 | 공존. 같은 kind는 active 1개 (DB 제약) |
| MEM-08 | 텍스트 안전: 전체 URL, 쿼리, 계정명, 비밀값, 호스트 밖 경로, 로그인·자격증명·외부 전송·도메인 이동 유도 문장, 제어 문자 | 저장 거부와 사유 코드 |
| MEM-09 | 불변 버전: 레이어 일부만 수정 시도 | 거부. 변경은 새 version으로만 |
| MEM-10 | 색인 일관성: version 저장·교체·롤백 뒤 `memory_index` | 색인이 version 내용과 항상 일치 (불일치 0) |
| MEM-11 | 기존 카탈로그 항목(`tags`, `summary`, `detail`, `evidence`)과 L0/L1/L2 왕복 변환 | 손실 없음 (`API-11`) |

## RCL — URL + 키워드 recall (테스트 L1/L2)

| ID | 동작 | 기대 |
|---|---|---|
| RCL-01 | 정확 route + task + 키워드 질의 | 해당 레코드가 1순위 |
| RCL-02 | 정확 route 메모리가 없을 때 | 안전한 상위 route, 그다음 host 전체 순으로 폴백. 상위 메모리는 L0에 하위 경로 포함이 적힌 경우만 |
| RCL-03 | `task_category` 없이 키워드만 | 키워드 일치 수 순으로 순위, 응답에 각 항목의 task 표시 |
| RCL-04 | `draft`와 `초안`, `medium`과 `미디엄` | 같은 레코드를 찾는다 |
| RCL-05 | 어휘에 없는 키워드를 넣은 클라이언트 호출 | 서버 요청 payload에는 어휘 안 키워드만 (요청 캡처 검사) |
| RCL-06 | `task_hint`에 센티널 문장 | 서버 요청, 로그, 캐시 어디에도 없다 |
| RCL-07 | 결과 수와 크기, 다른 host의 레코드 | ≤3개, 힌트당 ≤400 tokens, 교차 host 0 |
| RCL-08 | 같은 입력 반복, 동점 후보 | 같은 순서, 신뢰도 → `verified_at` → `id` |
| RCL-09 | `needs-reverify`, superseded, retired 레코드 | `needs-reverify`는 최신성 표시와 함께 같은 점수대에서 뒤로, 나머지는 결과에 없다 |
| RCL-10 | 매칭 없음 | 오류 없이 빈 결과와 `reason` |
| RCL-11 | URL 정규화 실패나 모호 | 빈 결과, 원문 URL을 저장·로그하지 않는다 |
| RCL-12 | server outage, conditional revalidation, cached anonymous recall | eligible cache respects 5-minute fresh/15-minute stale boundary; stale status visible; no authenticated response or receipt cached; no seed assumption |
| RCL-13 | 라벨 질의 세트: PR 고정 subset ≥100, 전체 ≥500 (URL 변형 × 키워드 한/영 × 카테고리) | precision@3 ≥ 0.9, recall@3 ≥ 0.85 (`metrics.md`; full corpus는 RCL-22) |
| RCL-14 | 같은 질의를 `wam recall`(CLI)과 MCP recall로 | 결과와 순서가 같다 |
| RCL-15 | 호출 시점: 같은 (세션, route 패턴, task) 5분 안 반복, 다른 route 패턴으로 이동, 작업 변경 | 반복은 재호출 없음, 이동·작업 변경은 재호출 (호스트 어댑터·하네스 trace 검사) |
| RCL-16 | 사용자가 '메모리 쓰지 마'라고 지시 | recall 호출 없음 (`BEH-06`) |

## QUE — 기여·큐레이션 큐 (테스트 L1)

| ID | 동작 | 기대 |
|---|---|---|
| QUE-01 | 기여 접수 중 DB 장애·프로세스 종료 주입 | 기여와 job이 함께 저장되거나 둘 다 없다. 202 이후 유실 0 |
| QUE-02 | 같은 scope의 job 여러 개를 worker 여러 개가 동시에 | 한 번에 하나만 처리, active version 중복 0. 다른 scope는 병렬 |
| QUE-03 | 같은 scope의 대기 기여 N건 | 한 번의 joint curator pass로 처리 (최대 20건, 60초 창, 설정 가능). 창은 scope의 첫 job이 도착한 때 시작하고 늦게 온 같은 scope job이 같은 묶음에 합류한다. 20건이 차거나 P0·재시도면 기다리지 않는다. 현재 구현은 v1 envelope 안 후보를 개별 curator loop로 처리하므로 joint pass acceptance는 미통과 |
| QUE-04 | 깊이 상한 초과 접수 | 429 또는 503과 `Retry-After`. 이미 접수한 job은 보존. 익명·신규 계정이 먼저 제한. 사용자·기기 상한(한 기기의 여러 계정 포함). 신규 계정 제한은 429, 전체 포화는 503. P0는 상한의 120%까지 접수. 같은 idempotency key의 재전송은 같은 job |
| QUE-05 | 한 기기·사용자가 폭주 | 다른 사용자의 job이 라운드 로빈으로 계속 처리된다 |
| QUE-06 | 모델 호출 실패 반복 | 지수 백오프와 지터, 최대 5회 뒤 `dead`, 재시도는 멱등 |
| QUE-07 | worker 강제 종료 (lease 중) | lease 만료 뒤 다른 worker가 재처리, 중복 게시·점수 0. 같은 이름으로 재시작한 worker도 이전 lease generation이면 거부(fencing). 만료 뒤 아무도 가져가지 않았다면 느린 worker가 마칠 수 있다. 계속 worker를 죽이는 job은 5회 뒤 `dead`. 재시도된 묶음은 이미 결정한 job을 다시 적용하지 않는다 |
| QUE-08 | 모델 토큰버킷·일일 크레딧 상한 도달 | job은 `waiting_budget`에 남고 유실 0, 해제 뒤 재개. recall에는 영향 없음. provider가 사용 한도·크레딧 소진을 알리면 시도를 쓰지 않고 대기하며, 알려 준 시간(없으면 기본 15분) 동안 아무 worker도 모델을 부르지 않는다. 이미 반영한 결정은 재개 때 다시 적용하지 않는다 |
| QUE-09 | 모델 429, 5xx, 시간 초과, 잘못된 응답 | 카탈로그 불변 (`MODEL-05`). 소진을 뜻하는 문구가 아닌 오류는 독립 engine에서 일반 실패로 백오프 재시도한다. 서버 worker는 429(소진 신호 없음)·408·5xx·연결 실패·시간 초과를 일시 장애로 보고 시도를 쓰지 않는 대기로 다루며(`QUE-25~33`), 잘못된 응답과 그 밖의 4xx는 일반 실패로 백오프 재시도한다 |
| QUE-10 | 동의 철회 | `queued`, `waiting_*`, `retry` job은 `canceled`되고 payload가 삭제된다. `leased` job은 commit 직전 동의를 확인한다. 철회가 먼저면 publish/author score 없이 취소되고, publish가 먼저면 별도 withdrawal job으로 재평가한다 (`PRIV-04`, `DB-18`). 철회 뒤 늦게 도착한 같은 idempotency key의 재전송은 취소된 job을 되살리지 않는다. 철회 당시 `leased`였던 job은 그 기여자의 증거를 빼고 판정한다 |
| QUE-11 | P0(reverify, rollback)와 P1 job이 함께 대기 | P0가 먼저 처리된다 |
| QUE-12 | 정해진 도착률에서 부하 시험 | 접수→결정 p95가 SLO 이내, 큐 깊이·나이·처리량 지표 노출 (`metrics.md`) |
| QUE-13 | 기여자 대시보드의 상태 | job 상태와 판정 단계에서 파생한 값이 일치 (`WEB-20~22`) |
| QUE-14 | 끝난 job payload 보관 기한 | 기한 뒤 purge, 감사 메타데이터만 남음 |
| QUE-15 | curator batch envelope의 버전·scope·job IDs·후보 상한 검증 | 지원하지 않는 버전, 빈/중복 job ID, 혼합 scope, 초과 크기는 모델 호출 전 거부되고 queue retry로 남음; 유효 envelope의 digest는 직렬화 왕복 후 동일 |
| QUE-16 | PostgreSQL queue 재시작·nonce·scope lease·expired lease 복구 | 재접수는 같은 ID를 반환하고 held scope 중복 lease는 없으며, 재시작 후 만료 lease는 시도를 소진하고 새 generation으로 회수됨 (`tests/postgres_queue.rs`, ignored disposable DB) |
| QUE-17 | decision transaction의 queue lease/attribution fence | fence 누락·stale generation·선행 철회는 memory/decision journal을 바꾸지 않으며, current owner와 동의된 contribution만 원자 commit됨 (`tests/postgres_queue.rs`, ignored disposable DB; DB-23, DB-25) |
| QUE-18 | PostgreSQL queue withdrawal, resolve, purge, stats | contribution 철회는 대기 job을 취소하고 leased attribution을 제거한다. resolve는 이전 worker를 fence하며 terminal purge는 nonce를 해제하고 metrics가 남은 저장 상태와 맞는다 (`tests/postgres_queue.rs`, ignored disposable DB) |
| QUE-19 | scope-level joint contribution batch | 여러 계정의 같은 scope 후보는 하나의 curator report/결정으로 처리되고, 같은 계정 반복 제출은 후보·support evidence를 늘리지 않는다. 서로 다른 queue kind는 한 batch로 섞이지 않는다 (`tests/worker.rs`, `tests/queue.rs`) |
| QUE-20 | 서버 worker: provider가 크레딧·쿼터 소진을 알림 | 소진을 만난 job은 `waiting_budget`에서 시도를 쓰지 않고 기다린다. 게이트가 닫힌 동안 어떤 job도 claim·replay·모델 호출이 없고(`Idle`), 그 사이 접수된 기여도 `queued`로 남으며 `queue_health`가 대기 job을 센다 (`tests/model_budget.rs`) |
| QUE-21 | 소진 지속·시간 경과·재시작 | provider가 알린 시간(30초~1시간)을 쓰고 알리지 않으면 15분부터 연속 거절마다 2배(최대 1시간). 크레딧이 돌아오면 대기 job이 게시되고 게이트와 연속 거절 횟수가 초기화되며, 게이트는 재시작과 다른 worker·connection에서도 유지된다 (`tests/model_budget.rs`, PostgreSQL은 `tests/postgres_application.rs` ignored) |
| QUE-22 | 운영자 일일 지출 상한 | 상한에 닿으면 다음 UTC 날짜까지 새 job의 claim·replay·모델 호출이 없고 다음 날 합계 0에서 재개한다. 이벤트는 닫힐 때와 열릴 때 한 번씩이며, 정확 중복은 모델 호출 없이 비용 0으로 처리된다. 비용을 보고하지 않는 provider와 상한을 함께 설정하면 worker가 시작을 거부한다 (`tests/model_budget.rs`, `wam-worker` 설정 검사) |
| QUE-23 | 운영자 재개와 철회 | `wam-worker --resume-budget`이 pause를 풀고 대기 job을 `queued`로 되돌려 즉시 시작한다(일일 상한은 풀지 않는다). 철회한 `waiting_budget` job은 취소되어 모델에 가지 않는다 (`tests/model_budget.rs`) |
| QUE-24 | 이전 릴리스 DB 업그레이드 | migration `0003`이 연결 시 적용되어 work 상태 `waiting_budget`을 허용하고 그 밖의 상태는 계속 거부한다. 게이트는 `wam_app_meta`에 저장된다 (`tests/postgres_application.rs`, ignored disposable DB) |
| QUE-25 | 서버 worker: provider 일시 장애가 이어짐 | 장애를 만난 job은 `waiting_budget`에서 시도를 쓰지 않고 기다린다. 장애가 일곱 번 이어져도(이전 동작이면 다섯 번째에 검토행) `waiting_review`로 가지 않고 기여는 `queued`이며, 회복하면 한 번 게시되고 점수 20이 한 번 생긴다 (`tests/provider_outage.rs`) |
| QUE-26 | 장애 중 다른 job과 호출 수 | 게이트가 `provider_outage`로 닫히는 동안 어떤 job도 claim·replay·모델 호출이 없고(`Idle`), 열리면 탐침 1건만 나간다. 접수된 기여는 `queued`로 남고 `queue_health`가 대기 job을 센다 (`tests/provider_outage.rs`) |
| QUE-27 | 장애 대기 시간 | `max(Retry-After, 백오프)`, 최소 30초. 백오프는 30초에서 연속 장애마다 2배(최대 600초: 30, 60, 120, 240, 480, 600, 600). `Retry-After`는 1시간까지 따르고 이미 정한 재개 시각을 앞당기지 않는다 (`tests/provider_outage.rs`, `budget.rs` 단위) |
| QUE-28 | 장애 회복과 처리 순서 | provider가 답하면 연속 장애·소진 횟수와 job의 장애 대기 횟수가 0이 되고 게이트가 열린다. 대기하던 job은 장애를 덜 만난 job부터(없으면 먼저 온 순서) 한 번씩 게시된다 (`tests/provider_outage.rs`) |
| QUE-29 | 장애를 일으키는 job | 한 job만 장애를 일으키면 다른 job이 먼저 처리되고(장애 대기가 적은 job 우선), 그 job은 시도 없는 대기 8번 뒤 일반 실패처럼 5번 쓰여 `waiting_review`로 간다(총 13번 호출, `review_required` 1건) (`tests/provider_outage.rs`, `budget.rs` 단위) |
| QUE-30 | 일반 실패와 이전 동작 되돌리기 | `Failed`와 결정 못함은 계속 5번째에 검토행이고 게이트를 닫지 않는다. `WAM_LLM_OUTAGE_FREE_WAITS=0`이면 일시 장애도 일반 실패처럼 쓰인다. 설정은 엄격히 읽고 값을 오류에 싣지 않는다 (`tests/provider_outage.rs`, `budget.rs` 단위) |
| QUE-31 | 장애 대기의 재시작·철회·운영자 재개와 노출 | 재시작 뒤에도 같은 재개 시각이 유지되고 철회한 `waiting_budget` job은 모델에 가지 않는다. `resume_model_budget`이 장애 pause를 풀고 대기 job을 `queued`로 돌린다. 상태는 `blocked_by`, `outage_strikes`, 정제된 `reason`을 보인다 (`tests/provider_outage.rs`, `budget.rs` 단위) |
| QUE-32 | 실제 HTTP provider로 장애와 회복 | loopback 서버가 503(`Retry-After: 45`), 429, 정상 응답을 차례로 줄 때 worker가 45초·60초 대기를 지키고 세 번째에 게시한다. 닫힌 동안 서버에 요청이 가지 않고 상태·이벤트에 키와 서버 본문이 없다 (`tests/provider_outage.rs`) |
| QUE-33 | PostgreSQL에서 장애 게이트와 이전 형식 문서 | job 행이 `waiting_budget`(migration 없음)이고 게이트가 다른 연결·재연결에서도 같다. 새 필드가 없는 이전 형식의 게이트 문서는 열린 게이트로 읽힌다 (`tests/postgres_application.rs`, ignored disposable DB) |
| QUE-34 | 키·모델 설정 거부는 시도 없는 대기 (#168) | 401·403·404를 만난 job은 `waiting_budget`에서 시도를 쓰지 않고 게이트가 `provider_rejected`로 1시간 닫힌다. 닫힌 동안 claim·replay·모델 호출이 없다 (`tests/provider_rejected.rs`) |
| QUE-35 | 거부 pause는 1시간부터 두 배, 최대 6시간 (#168) | 3600, 7200, 14400, 21600초로 닫히고 `WAM_LLM_REJECTED_PAUSE_SECS`·`…_MAX_SECS`가 이를 바꾼다. job은 검토로 가지 않는다 (`tests/provider_rejected.rs`, `budget.rs` 단위) |
| QUE-36 | 키를 고치면 저절로 회복, `--resume-budget`은 즉시 (#168) | 다음 탐침이 답하면 게시되고 게이트가 열린다. `resume_model_budget`은 거부 pause를 풀고 대기 job을 되돌려 바로 처리된다 (`tests/provider_rejected.rs`, `budget.rs` 단위) |
| QUE-37 | 거부도 job 한도 뒤에는 검토행 (#168) | 장애와 합산한 한도(`WAM_LLM_OUTAGE_FREE_WAITS`)를 다 쓴 job은 일반 실패처럼 시도가 쓰여 다섯 번째에 검토행이 된다. 한도 0이면 첫 거부부터 시도가 쓰인다 (`tests/provider_rejected.rs`, `budget.rs` 단위) |
| QUE-38 | 거부의 운영자 신호와 재시작 보존 (#168) | `blocked_by = provider_rejected`, `rejected_strikes`, `reason`, 닫힘 로그의 "키·모델·주소를 확인" 안내, 거부 pause 중 `queue_health.alert`. 재시작 뒤에도 같다. 이전 릴리스는 거부 pause를 장애 pause로 읽는다 (`tests/provider_rejected.rs`, `budget.rs` 단위) |
| QUE-39 | 실제 HTTP provider로 거부와 회복 (#168) | loopback 서버가 Meta 형식의 401, 정상 응답을 차례로 줄 때 worker가 1시간 대기를 지키고 재개 시각에 게시한다. 닫힌 동안 서버에 요청이 가지 않고 상태·이벤트에 키와 서버 본문이 없다 (`tests/provider_rejected.rs`) |

## CUR — 큐레이터 지침과 도구 (테스트 L1/L2)

| ID | 동작 | 기대 |
|---|---|---|
| CUR-01 | 모델에 노출되는 도구 목록과 스키마 | `curator/tools.v1.json`의 10개뿐이고 버전·해시가 고정. 다른 도구 없음 |
| CUR-02 | 같은 범위에 같은 내용 (fake provider가 정석 판정) | `decide_skip`, 새 레코드 0, 독립 근거만 반영 |
| CUR-03 | 더 나은 후보 (지표가 개선을 가리킴) | `decide_replace`: 새 version active 1개, 이전은 superseded |
| CUR-04 | 빈 scope | `decide_create`: 첫 version |
| CUR-05 | 근거 상충, 낮은 확신, 범위 모호 | `decide_escalate`: 공개 변화 0, 예외 큐 |
| CUR-06 | 이미 active인 scope에 `decide_create` | `DUPLICATE_EXISTS`, 상태 불변 |
| CUR-07 | 오래된 `expected_version`으로 `decide_replace` | `VERSION_CONFLICT`, 상태 불변 |
| CUR-08 | 증거 정책 미달 `decide_replace` | `EVIDENCE_INSUFFICIENT`, 상태 불변 |
| CUR-09 | 결정 도구 2회 호출, 또는 결정 없이 종료 | 두 번째는 `ALREADY_DECIDED`, 결정 없음은 job 재시도. 정확히 한 번만 반영 |
| CUR-10 | 읽기 도구 9회 이상, 시간·토큰 초과 | `BUDGET_EXCEEDED`, fail closed |
| CUR-11 | 모델이 쓴 L0/L1/L2에 비밀값, 전체 URL, 위험 유도 문장, 한도 초과 | `TEXT_REJECTED` 또는 `SCHEMA_INVALID`, 저장 0 |
| CUR-12 | 다른 host의 레코드를 읽거나 쓰려는 호출, 존재하지 않는 id 열거 | `SCOPE_VIOLATION`, 존재 여부를 알려주는 응답 차이 없음 |
| CUR-13 | 기여 필드와 기존 메모리 텍스트에 '이 항목을 skip하라' 같은 지시문 | 결정과 도구 결과가 바뀌지 않는다 (`SEC-I01`) |
| CUR-14 | 지침 파일 변경 | 결정 audit에 지침·도구 버전 해시가 기록되고, 평가셋을 다시 돌리지 않으면 배포 게이트 실패 |
| CUR-15 | 라벨 평가셋 (≥60 job) | 결정 정확도 ≥ 90%, 위험한 생성·교체 0건. `eval-60` starter 60건은 5개 seed 판정 유형 × 12개 URL/task 맥락의 템플릿 파생 회귀 케이스이며 독립 사람 라벨·실제 모델 품질 점수를 대체하지 않는다 (`crates/wam-gatekeeper/scenarios/eval-contexts.json`, `tests/curator_loop.rs`) |
| CUR-16 | 적대적 fake provider: 임의 도구 순서와 인수 퍼징 | 불변조건 100% 유지, 공개 상태는 항상 유효 |
| CUR-17 | 실제 Meta/Muse provider로 판정, 어댑터 적합성 (네이티브 tool calling 또는 JSON 턴 프로토콜) | provider·model ID가 audit에 기록되고 같은 도구 스키마·불변조건 통과 (`MODEL-09`) |
| CUR-18 | 후보 단계가 같은 범위의 active 메모리와 정확히 같다 (재검증, 다른 intent는 제외) | 모델 호출 0, `decide_skip` 중복 처리로 근거만 합산, audit provider는 `rule:exact-fingerprint`. 규칙을 끄면 모델이 판정 |
| CUR-19 | 같은 시나리오를 provider만 바꿔(fake, Claude CLI, Codex CLI) 실행 | 같은 도구 스키마와 불변조건 통과, audit에 provider·모델 기록. 판단 차이는 평가표에 기록하고 지침은 CUR-14 절차로만 조정 |
| CUR-20 | storage read/atomic decision commit 중 장애 주입 | `STORAGE_UNAVAILABLE`, 모델 추가 결정 없이 retry; memory/version/support와 decision journal은 변경 0 |
| CUR-21 | 같은 scope의 신규 contribution batch를 한 번 판정 | `job_get`과 `memory_compare`가 후보 전체를 포함하고 결정 report는 하나다. 같은 계정의 반복 contribution은 후보와 independent contributor/support 수를 부풀리지 않는다. 도구·지침 변경 후 CUR-14 평가셋을 다시 실행한다 (`tests/worker.rs`, `QUE-19`) |

## PERF — 성능 이득 (L6)

| ID | 동작 | 기대 |
|---|---|---|
| PERF-01 | Medium/Naver 유사 fixture, tools-only vs 주입, 각 3~5회 | 평균과 편차 기록, 성공률 비하락 |
| PERF-02 | 리치 에디터 fixture 반복 | 위와 동일 |
| PERF-03 | iframe 에디터 fixture 반복 (수동 recall vs 자동 주입 분리) | 조건별 결과 보고 |
| PERF-04 | 호스트별 분리 (Arise 자체 메모리 자동 주입, Claude Code/Codex + WAM 수동 recall) | 호스트별 표 |
| PERF-05 | 같은 모델/effort/태스크/fresh state, 동일 툴 버전끼리만 비교 | 비교 조건 기록 |
| PERF-06 | 잘못된/오래된 힌트 주입 (UI 변경 시뮬레이션) | 성능 저하 크기와 에이전트의 회복 여부 기록 (G6 근거) |

## 검색·구조화 데이터 회귀와 성능 하네스

검색 fixture, 데이터 분포, 실행 환경과 sink 규약은 [`retrieval-benchmark.md`](retrieval-benchmark.md)를 기준으로 한다. 아래 PERF ID는 앞 절의 사용자 작업 생산성 벤치 ID와 구분되는 검색 시스템 성능 테스트다.

### MEM — 구조화 메모리 추가 테스트

| ID | 동작 | 기대 |
|---|---|---|
| MEM-12 | JSON schema 왕복, 필드 순서·공백 변형, unknown field/schema version 입력 | 같은 typed record와 canonical fingerprint/index projection; 지원하지 않는 형식은 저장 전 거부 |
| MEM-13 | immutable version에서 L0 index projection을 재생성하고 같은 job을 재시도 | 같은 tuple 집합, 순서 무관, 중복 row 0; 비활성 version은 기본 recall 결과에서 제외 |
| MEM-14 | L0 keyword/task/url pattern과 L1/L2 kind별 필수 필드 누락·불일치 | 저장 전 전체 record 거부, partial version/index 0 |
| MEM-15 | L0/L1/L2 및 필드 간 분할 sentinel·악성 지시문 입력 | candidate 전체 거부, 고정 reason code만 반환, DB/queue/index/log/public memory에 원문·부분 마스킹 레코드 0 |

### RCL — URL + keyword 모듈 경계 추가 테스트

| ID | 동작 | 기대 |
|---|---|---|
| RCL-17 | 같은 query를 core exhaustive scan과 in-memory/PostgreSQL candidate repository로 각각 평가 | 후보 집합, 전체 필터, 최종 ID/version/rank가 동일; repository가 후보를 누락하지 않음 |
| RCL-18 | 저장 순서, 입력 순서, 중복 index fixture, pagination 경계를 바꿔 반복 | 같은 memory는 한 번만 반환되고 결과 ID/version/rank가 항상 결정적 |
| RCL-19 | 합성 host/path/task/kind 조합을 property test로 생성 | 다른 host/task/sibling route leakage 0; 명시적으로 포함한 상위 route만 후보다 |
| RCL-20 | vocabulary/schema 버전, 한국어·영어 alias, 미지원 버전을 교차 | 지원된 버전에서 alias 결과가 같고, 지원되지 않는 버전은 빈 결과 또는 명시 오류; 자유 prompt 전송 0 |
| RCL-21 | publish/supersede/retire/rollback/reindex 직후 cache와 DB 조회 | active version만 반환; stale cache, inactive index hit, duplicate result 0 |
| RCL-22 | 고정 500-query label corpus와 tuning에서 제외한 20% held-out set | `metrics.md` precision@3/recall@3 gate 통과, cross-host/task leakage와 inactive hit 0 |

### PERF — 검색 시스템 성능·재현성 추가 테스트

| ID | 동작 | 기대 |
|---|---|---|
| PERF-13 | 동일 toolchain·seed로 fixture generator를 두 번 실행 | manifest, record/index row 수, query/label/expected hash가 동일 |
| PERF-14 | 1k/10k/100k memory version 후보로 core-only filter/rank 측정 | 정확도 변동 0; p50/p95/p99, throughput, RSS 기록 |
| PERF-15 | 100k memory version/약 1.25M index row에서 exact/prefix/keyword/no-match query와 `EXPLAIN` 측정 | scope 및 keyword 검색 인덱스 사용 여부, rows examined, DB p95를 기록; 목표·기준은 `metrics.md` 적용 |
| PERF-16 | loopback recall API 100 RPS × 5분, concurrency 1/10/50/100 | 결과 ID/order/schema 동일, p95와 error rate가 `metrics.md` gate 충족 |
| PERF-17 | app cache cold/warm, ETag 304, invalidate 후 같은 query | 결과 동일, hit/miss 지연 분리, stale version 반환 0 |
| PERF-18 | exact/prefix/no-match/hot-host/한영 혼합 workload 실행 | query class별 p50/p95/p99, throughput, error와 host/task leakage를 분리 기록 |
| PERF-19 | 검색 부하 중 publish/reindex/rollback 및 DB 장애를 주입 | partial index, 중복, inactive hit 0; 복구 후 active version contract 유지 |
| PERF-20 | 고정 build·seed·runner에서 각 profile 5회 반복 | 다섯 histogram과 변동계수 보고, 회귀 guard 초과 run은 원인 분석 대상 |
