# API: 인증·기여 큐·조회·영수증

상태: 1차 상세 명세. API 서버는 아직 구현되지 않았다.

## 공통 규칙

API 기본 경로는 /v1이고, 원격 MCP는 /mcp에서 제공한다. 서비스는 사용자별 쓰기 요청에서 OAuth access token 또는 디바이스 토큰을 검증한다. 익명 접근은 공개 승인 메모리의 검색과 상세 조회에만 허용한다. 권한은 요청 JSON의 user_id가 아니라 인증된 세션에서 얻는다.

입력은 schema version과 크기 상한이 있는 폐쇄 스키마로 검증한다. URL, bearer token, cookie, contribution 원문을 일반 access log, tracing span, 예외 메시지에 넣지 않는다. POST recall 요청은 Cache-Control: no-store로 처리하고, body의 URL은 구조화 로그에서 redaction한다.

## 주요 endpoint

| 메서드·경로 | 권한 | 역할 |
|---|---|---|
| POST /v1/memories/recall | 익명 또는 로그인 | 안전 URL + task + 어휘 키워드로 승인된 메모리를 찾아 L1 힌트를 최대 3개 반환 ([`recall.md`](recall.md)). 인증된 caller에게만 사용 영수증을 발급 |
| GET /v1/memories/{id} | 익명 또는 로그인 | 공개 승인 버전만 상세 조회. `level`은 1(L1) 또는 2(L2) ([`memory.md`](memory.md)) |
| GET /v1/memories/vocab | 공개 읽기 | recall 키워드 어휘 (ETag 캐시). 클라이언트는 이 어휘 안의 키워드만 전송한다 ([`recall.md`](recall.md)) |
| POST /v1/contributions | 로그인 + 명시적 동의 | 정규화 trace를 작성자 계정에 귀속해 큐에 등록 |
| GET /v1/me/contributions | 본인 로그인 | 본인 기여 목록과 gatekeeper 상태 조회 |
| GET /v1/me/contributions/{id} | 본인 로그인 | 본인 기여의 결정 결과와 공개 가능한 사유 조회 |
| POST /v1/receipts/{id}/outcome | 영수증 소유 계정 | success/partial/failed 결과를 1회 보고 |
| GET /v1/me/dashboard | 본인 로그인 | 점수 합계, 본인 전체 순위, opt-in 중이면 공개 순위, 기여 요약, 최근 이벤트 원장과 기기 반환 |
| GET /v1/me/profile | 본인 로그인 | 본인 handle, bio, 서비스 링크, 공개 설정 조회 |
| PUT /v1/me/profile | 본인 로그인 | 본인 프로필 편집. handle 중복은 409 |
| GET /v1/profiles/{handle} | 공개 읽기 | 공개 opt-in 프로필만 반환. 숨김/없는 handle은 동일 404 |
| GET /v1/me/memories/analytics?from=&to=&bucket=&memory_id= | 본인 로그인 | 본인이 author인 memory version의 기간별 recall·상세 응답·다른 계정 수·outcome 집계 반환 |
| GET /v1/leaderboard | 공개 읽기 | 공개 opt-in cohort의 rank, handle, 점수 합계와 `/u/{handle}` 경로 반환 |
| GET /v1/guide/hosts/{host}/template | 공개 읽기 | 버전이 지정된 host-specific MCP config template 반환 |
| POST /v1/link/start, /v1/link/poll | device flow | stdio host를 웹 계정에 연결 |
| GET /v1/admin/gatekeeper/jobs | review role | 예외·저신뢰 gatekeeper 작업 조회 |

기존 /web-memory/v1/catalog와 reports 경로가 필요한 레거시 Arise adapter가 있으면 호환 변환 계층으로 제공한다. 새 Claude/Codex 클라이언트는 새 URL/task 계약을 쓴다.

## 기여 메모리 분석

`GET /v1/me/memories/analytics`는 인증된 계정이 `memory_version_authors`에 author로 연결된 version만 반환한다. `from`, `to`, `bucket=day|week`, 선택적 `memory_id`로 기간과 차트 범위를 정한다. 기본 기간은 최근 30일이며, `전체`는 해당 version의 최초 집계일부터 현재까지 주별 bucket으로 조회한다. 다른 계정 소유의 `memory_id`는 존재 여부를 드러내지 않는 404를 반환한다.

각 bucket은 작성자를 제외한 다른 로그인 계정에 memory가 recall 응답으로 전달된 횟수와 상세/expand 응답 횟수, 다른 로그인 계정의 distinct count, 익명 요청 집계, 유효한 다른 계정의 success/partial/failed outcome 수를 분리해 반환한다. bucket별 distinct count는 account 기준이며 작성자 본인과 기기는 제외한다. 선택 기간 전체의 고유 계정 합계는 bucket 값을 더하지 않고 기간 전체에서 중복 제거한다. 익명 값은 고유 방문자 수가 아니며 작성자 본인 여부를 판별할 수 없다고 표시한다. 분석 응답에는 reader identity, receipt ID, device ID, 이메일, raw URL을 넣지 않는다. 집계 값은 점수 원장이 아니며, 점수는 별도 원장 endpoint/data에서 계산한다.

## Profile, rank, rich-text contract

`GET /v1/me/dashboard`는 `Cache-Control: no-store`를 적용한다. `overall_rank`는 인증 계정의 양수 점수 순위를 모든 자격 계정 기준으로 계산해 본인에게만 반환한다. 점수가 없거나 0 이하이면 null이다. opt-in 상태일 때만 공개 cohort의 `public_rank`도 반환한다. 순위는 서버가 score event 합계로 계산하며 client-provided rank/points를 받지 않는다.

`GET /v1/profiles/{handle}`는 opt-in 공개 profile의 handle, bio blocks, service links만 반환한다. `GET /v1/profiles/{handle}`와 `GET /v1/leaderboard`는 `Cache-Control: no-store`를 적용해 opt-out 뒤 공개 데이터가 남지 않게 한다. `PUT /v1/me/profile`은 authenticated session에서 owner를 정한다. body는 canonical handle, schema-versioned bio blocks, service links, 공개 설정만 담는다. handle 충돌은 409이고 profile 내용은 부분 저장하지 않는다. 외부 URL은 validation만 하며 preview를 위해 fetch하지 않는다. 공개 profile/leaderboard 응답은 email, internal ID, 비공개 계정 점수와 bio를 제외한다. 전체 입력·출력 schema는 [`profile.md`](profile.md), 수용 테스트는 PROFILE-01~10을 따른다.

## Recall 응답과 사용 영수증

응답은 결과 메모리의 id, version, canonical URL pattern, task category, 간결한 untrusted hint, freshness, verification summary를 포함한다. 사용자와 무관한 공개 응답에 작성자의 이메일이나 원본 기여 내용을 넣지 않는다. 전체 결과는 최대 3개로 제한한다.

익명 recall은 공개 힌트만 반환하고 user-specific receipt를 만들지 않는다. 로그인 recall은 서버가 consumer account, memory version, issue time, nonce에 연결된 1회성 receipt를 생성한다. 클라이언트가 outcome report를 보낼 때 server는 receipt ownership과 만료, 재사용 여부를 확인한다. 동일 receipt 결과의 재요청은 멱등 처리하며 추가 점수를 만들지 않는다.

## Contribution 및 Gatekeeper 큐

POST contributions 요청은 canonical URL pattern, task category, schema version, 정규화 action sequence, outcome, observed traits, consent version, idempotency nonce를 받는다. contributor identity는 인증 middleware가 채우고 body에서 덮어쓸 수 없다. 성공 응답은 202와 contribution ID를 반환하며 즉시 게시를 뜻하지 않는다.

GET /v1/me/contributions는 작성자 본인 것만 반환한다. 다른 계정이 UUID를 추측해 접근해도 404 또는 동일한 권한 오류가 발생한다. 공개 관리 화면에는 agent decision, confidence band, reason codes, evidence digest만 제공하고 모델 체인, 비공개 payload, 사용자 프롬프트는 제공하지 않는다.

## 인증 정책

웹 로그인은 GitHub OAuth를 초기 provider로 쓴다. Remote MCP는 OAuth 2.1 authorization flow를 우선한다. OAuth 미지원 local host는 device linking으로 발급한 최소 권한 device token을 OS secure storage에 둔다. read:memories와 write:contributions, report:outcome 권한을 분리하고 토큰 해지 즉시 쓰기 요청을 401로 거부한다.

## 오류·멱등성·속도 제한

400은 스키마/URL 오류, 401은 인증 실패, 403은 권한 부족, 404는 권한으로 숨긴 리소스 또는 없는 항목, 409는 충돌 상태, 413은 크기 초과, 429는 quota 초과, 5xx는 일시 서버 오류다. 오류에 요청 원문을 echo하지 않는다.

모든 제출과 결과 보고는 idempotency key를 갖는다. gatekeeper job을 재시도해도 같은 결과와 score event가 두 번 만들어지지 않는다. 익명과 로그인 검색은 별도 rate limit을 가지며, 기여 제출은 사용자·device 단위 제한을 함께 적용한다.

## 연결 테스트

API-30~40은 recall, contribution ownership, OAuth claims, receipt, dashboard analytics, leaderboard와 guide template endpoint를 계약 수준에서 확인한다. API 스키마와 MCP tool schema는 동일한 core contract에서 생성되어 호스트별 필드 이름 차이를 줄인다.
