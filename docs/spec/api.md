# API: 인증·기여 큐·조회·영수증

상태: 일부 구현. 조회·프로필·순위, contribution 흐름, owner-bound receipt/outcome, PostgreSQL/local repository, GitHub OAuth/device identity, score ledger, rate limits, account usage, per-memory author analytics가 working tree에 구현되어 있다. 인증 recall은 영수증을 발급하고 `/v1/receipts/{id}/outcome`에서 선택된 성공 사용 결과를 알려진 다른 author에게 +2로 기록한다. 수동 review 승인 자체는 memory를 publish하지 않으며 +20도 지급하지 않는다. 실제 production PostgreSQL/Redis/OAuth 배포와 Gatekeeper replay 구성이 이 환경에서 검증되지는 않았다.

## 공통 규칙

API 기본 경로는 /v1이고, 원격 MCP는 /mcp에서 제공한다. 서비스는 사용자별 쓰기 요청에서 OAuth access token 또는 디바이스 토큰을 검증한다. 익명 접근은 공개 승인 메모리의 검색과 상세 조회에만 허용한다. 권한은 요청 JSON의 user_id가 아니라 인증된 세션에서 얻는다.

입력은 schema version과 크기 상한이 있는 폐쇄 스키마로 검증한다. URL, bearer token, cookie, contribution 원문을 일반 access log, tracing span, 예외 메시지에 넣지 않는다. Authenticated POST recall은 항상 `Cache-Control: no-store`로 처리하고 body URL을 구조화 로그에서 redaction한다. Anonymous recall도 `Cache-Control: no-store`로 브라우저와 공용 proxy가 저장하지 않는다. WAM CLI/local MCP client만 `X-WAM-Client-Cache` opt-in 헤더를 보고 별도 private cache에 저장할 수 있다. 자세한 내용은 [`client-cache.md`](client-cache.md)를 따른다.

## 주요 endpoint

| 메서드·경로 | 권한 | 역할 |
|---|---|---|
| GET /v1/mcp/status | 익명 또는 optional bearer | MCP endpoint와 인증 상태, 동의 상태, 숫자 catalog contract version을 반환한다. 만료/해지 bearer는 401. token·계정 ID·이메일·device 정보는 반환하지 않으며 응답은 `no-store` |
| POST /v1/memories/recall | 익명 또는 로그인 | 안전 URL + task + 어휘 키워드로 승인된 메모리를 찾아 L1 힌트를 최대 3개 반환 ([`recall.md`](recall.md)). 인증된 caller에게만 사용 영수증을 발급 |
| GET /v1/memories/{id}?level=&version_id= | 익명 또는 로그인 | 공개 승인 버전만 상세 조회. `level`은 1(L1) 또는 2(L2); 선택한 `version_id`가 있으면 그 immutable version을 반환한다. 없는/폐기된 버전은 404 ([`memory.md`](memory.md)) |
| GET /v1/memories/vocab | 공개 읽기 | recall 키워드 어휘 (ETag 캐시). 클라이언트는 이 어휘 안의 키워드만 전송한다 ([`recall.md`](recall.md)) |
| GET /v1/metrics | 공개 읽기 | 계정 ID, 영수증, 메모리 ID, URL 없이 aggregate recall/detail/outcome 합계 반환 |
| POST /v1/me/contributions | demo bearer token + 명시적 동의 | URL/action 요약을 canonicalize하고 local file review queue에 `queued` 상태로 접수 |
| GET /v1/me/contributions | 본인 demo token | 본인 로컬 파일 접수 목록 |
| GET /v1/me/contributions/{id} | 본인 demo token | 본인 로컬 파일 접수 상세 |
| DELETE /v1/me/contributions/{id} | 본인 인증 | payload·기여 귀속 철회 및 독립 근거 재판정. 단독 근거 version은 즉시 retire |
| GET /v1/review/contributions | 별도 review token | contributor ID 없이 검수 상태 집계와 로컬 접수 목록 반환 |
| POST /v1/review/contributions/{id}/claim | 별도 review token | queued 항목을 generation을 올려 `in_review`로 전환 |
| POST /v1/review/contributions/{id}/decision | 별도 review token | 현재 generation에 한해 `approve`/`reject` 결과를 저장 |
| POST /v1/receipts/{id}/outcome | 영수증 소유 계정 | success/partial/failed 결과를 1회 보고. 같은 결과 재전송은 멱등 응답, 다른 결과는 409 |
| GET /v1/me/dashboard | 본인 인증 | 점수 합계, 본인 전체 순위, opt-in 중이면 공개 순위를 반환한다. 점수 내역은 별도 score-events endpoint에서 조회한다 |
| GET /v1/me/score-events | 본인 인증 (`read:dashboard`) | 본인 수령 원장만 최근 기록 순으로 반환. `limit` 기본 20, 1~100. `before`는 본인 응답의 event ID. `{schema_version, events, next_cursor}`이며 event는 id/type/delta/memory/version/created_at만 포함한다. 타인·없는 cursor는 동일 404, 미인증 401, scope 부족 403. 응답은 `no-store` |
| GET /v1/me/consent | 본인 인증 (`write:contributions`) | 현재 기여 수신 허용과 public-sharing-v1, 7일 retention policy 반환 |
| PUT /v1/me/consent | 본인 인증 + web CSRF | `{enabled,consent_version}`. false는 새 기여 차단·기존 payload/attribution 철회. true가 예전 귀속을 복구하지 않음 |
| DELETE /v1/me/account | web session (`manage:account`) + CSRF | `confirmation=DELETE` 후 프로필·자격증명·식별 연결 제거, 독립 근거 없는 version retire. Device에는 이 scope를 발급하지 않음 |
| GET /v1/me/profile | 본인 로그인 | 본인 handle, bio, 서비스 링크, 공개 설정 조회 |
| PUT /v1/me/profile | 본인 web session (`manage:account`) + CSRF | 본인 프로필 편집. handle 중복은 409 |
| GET /v1/profiles/{handle} | 공개 읽기 | 공개 opt-in 프로필만 반환. 숨김/없는 handle은 동일 404 |
| GET /v1/me/memories/analytics?from=&to=&bucket=day\|week | 본인 로그인 (`read:dashboard`) | 본인이 author인 version의 90일 이내 날짜별 recall hit·상세 조회·명시 outcome aggregate 반환. 다른 계정 수는 distinct reader가 아니라 hit 횟수다. `no-store` |
| GET /v1/me/usage | 본인 로그인 (`read:dashboard`) | account bucket quota, burst/remaining/refill wait, 최근 30일 admitted call·unit과 account-attributed denied request 일별 aggregate 반환. 익명·invalid credential 거절은 계정 기록에 포함하지 않는다. `no-store` |
| GET /v1/leaderboard | 공개 읽기 | 공개 opt-in cohort의 rank, handle, 점수 합계와 `/u/{handle}` 경로 반환 |
| GET /v1/guide/hosts/{host}/template | 공개 읽기 | 버전이 지정된 host-specific MCP config template 반환 |
| POST /v1/link/start, /v1/link/poll | device flow | stdio host를 웹 계정에 연결 |
| GET /v1/link/status | 유효 device bearer | 현재 token의 기기 ID/이름/scope/만료만 반환. token·계정 정보 없음, no-store |
| POST /v1/link/revoke | 유효 device bearer | 현재 token의 기기만 해지하고 204 반환. body의 타인 device ID로 대상을 바꿀 수 없음 |
| POST /v1/admin/auth/start, /verify | 허용 이메일 + 전용 admin origin / browser-bound pending cookie | 관리자 이메일 버튼 로그인 시작·교환 (세부: [admin.md](admin.md)) |
| GET /v1/admin/auth/session; POST /v1/admin/auth/logout | 관리자 session | 세션 복구와 서버 세션 폐기 |
| GET /v1/admin/accounts, /v1/admin/accounts/{id} | 관리자 session | 가입자 목록/상세; 현재 endpoint와 화면은 추가 구현 필요 |
| GET /v1/admin/contributions, /{id} | 관리자 session | 기여 목록·상세와 Muse/replay 근거 |
| POST /v1/admin/contributions/{id}/claim, /decision | 관리자 session + CSRF | 검수 시작, Muse 검증 요청/게시 결정/반려. 세대 충돌은 409 |

기존 /web-memory/v1/catalog와 reports 경로가 필요한 레거시 Arise adapter가 있으면 호환 변환 계층으로 제공한다. 새 Claude/Codex 클라이언트는 새 URL/task 계약을 쓴다.

## 기여 메모리 분석

`GET /v1/me/memories/analytics`는 인증된 계정이 author로 연결된 memory version만 반환한다. `from`과 `to`는 UTC `YYYY-MM-DD` 날짜이고, 둘 다 생략하면 최근 30일을 반환한다. 최대 조회 기간은 90일이다. `bucket=day|week`는 daily 또는 월요일 시작 UTC 주간 집계를 선택한다. 다른 계정 소유 version의 식별자는 응답에 포함하지 않는다.

각 bucket은 anonymous recall hit, 다른 로그인 계정 recall hit, anonymous detail view, 다른 계정 detail view, 명시적으로 사용 선택된 receipt outcome(success/partial/failed)을 반환한다. 조회 수는 고유 사용자 수가 아니라 요청 수다. 작성자 본인의 read/outcome은 impact에 합산하지 않는다. 익명 값은 고유 방문자 수가 아니다. 분석 응답에는 reader identity, receipt ID, device ID, 이메일, raw URL을 넣지 않는다. 집계 값은 점수 원장이 아니며, 점수는 별도 원장 endpoint/data에서 계산한다.

## Profile, rank, rich-text contract

`GET /v1/me/dashboard`는 `Cache-Control: no-store`를 적용한다. `overall_rank`는 인증 계정의 양수 점수 순위를 모든 자격 계정 기준으로 계산해 본인에게만 반환한다. 점수가 없거나 0 이하이면 null이다. opt-in 상태일 때만 공개 cohort의 `public_rank`도 반환한다. 순위는 서버가 score event 합계로 계산하며 client-provided rank/points를 받지 않는다.

`GET /v1/profiles/{handle}`는 opt-in 공개 profile의 handle, bio blocks, service links만 반환한다. `GET /v1/profiles/{handle}`와 `GET /v1/leaderboard`는 `Cache-Control: no-store`를 적용해 opt-out 뒤 공개 데이터가 남지 않게 한다. `PUT /v1/me/profile`은 authenticated session에서 owner를 정한다. body는 canonical handle, schema-versioned bio blocks, service links, 공개 설정만 담는다. handle 충돌은 409이고 profile 내용은 부분 저장하지 않는다. 외부 URL은 validation만 하며 preview를 위해 fetch하지 않는다. 공개 profile/leaderboard 응답은 email, internal ID, 비공개 계정 점수와 bio를 제외한다. 전체 입력·출력 schema는 [`profile.md`](profile.md), 수용 테스트는 PROFILE-01~10을 따른다.

## Recall 응답과 사용 영수증

응답은 결과 메모리의 id, version, canonical URL pattern, task category, 간결한 untrusted hint, freshness, verification summary를 포함한다. 사용자와 무관한 공개 응답에 작성자의 이메일이나 원본 기여 내용을 넣지 않는다. 전체 결과는 최대 3개로 제한한다.

익명 recall은 공개 힌트만 반환하고 user-specific receipt를 만들지 않는다. local authenticated recall에 결과가 있으면 서버가 consumer account, memory/version/author 귀속과 발급 시각을 보존하는 30일짜리 owner-bound receipt를 생성한다. 서버는 URL이나 page content를 metrics store에 저장하지 않는다. outcome report body는 `outcome`과 선택적인 `used_version_ids`를 받는다. 실제 사용한 receipt 소속 버전만 최대 3개 지정하며, 생략/빈 목록은 결과만 기록하고 점수를 지급하지 않는다. 중복 ID나 receipt 밖의 버전은 400이고 영수증을 소비하지 않는다. receipt owner와 만료를 검사하며 같은 결과·사용 버전 집합(순서 무관)의 재요청은 `recorded:false`로 멱등 처리한다. 결과나 집합을 바꾸면 409다. success는 지정한 버전의 알려진 다른 계정 author에게만 +2 score event를 기록하며 consumer/version 및 consumer/author 하루 상한을 최초 보고일(UTC)에 고정한다. 다음 날 재전송해도 당시 상한을 유지한다. partial/failed/익명/자기 사용은 점수를 만들지 않는다. PostgreSQL 또는 단일 local application snapshot에서 outcome/score/outbox를 함께 commit한다. 폐기된 version은 새 사용 점수가 없고 retirement/rollback은 별도 reversal을 만든다.

상세 조회는 `version_id`를 생략하면 현재 `active` 또는 `needs-reverify` 버전만 반환한다. `version_id`를 지정하면 해당 immutable 버전을 반환하며, `superseded` 버전도 메모리 자체가 공개 상태인 동안 조회할 수 있다. retired memory/version, 잘못된 ID/version 조합은 L1/L2 모두 없는 ID와 같은 404다. 거부된 version 조회는 detail usage를 증가시키지 않는다.

## Contribution 및 Gatekeeper 큐

Contribution POST는 canonical URL, task/action enum, `consent=true`, `consent_version=public-sharing-v1`, 선택적 `nonce`를 받는다. nonce는 계정 범위에서 같은 payload 재전송을 같은 접수 ID로 묶고, payload 변경이나 철회된 접수 재시도는 409다. nonce 원문은 저장하지 않는다. 접수·consent·durable work·outbox는 함께 commit한다. Worker는 Gatekeeper와 신뢰할 수 있는 replay를 transaction 밖에서 실행한 뒤 schema/secret/정확한 route scope/consent/lease/current version을 다시 확인한다. 성공 시 immutable version·author +20·기여/작업 상태·decision/audit/outbox가 함께 바뀐다. Duplicate support는 작성자/+20을 만들지 않는다. 낮은 confidence는 human review에 남고, 검증 proposal 없는 수동 승인은 재검증만 요청한다. 실제 모델/브라우저 재현 실행기 설정은 서버 운영 문서를 따른다.

GET /v1/me/contributions는 작성자 본인 것만 반환한다. 다른 계정이 UUID를 추측해 접근해도 404 또는 동일한 권한 오류가 발생한다. local review 화면은 URL pattern, task, 선택 action, 상태와 집계만 제공하며 contributor ID, 원문 페이지, 사용자 프롬프트는 제공하지 않는다.

## 인증 정책

웹 로그인은 GitHub OAuth를 초기 provider로 쓴다. Remote MCP는 OAuth 2.1 authorization flow를 우선한다. OAuth 미지원 local host는 device linking으로 발급한 최소 권한 device token을 OS secure storage에 둔다. read:memories와 write:contributions, report:outcome 권한을 분리하고 토큰 해지 즉시 쓰기 요청을 401로 거부한다.

## 오류·멱등성·속도 제한

400은 스키마/URL 오류, 401은 인증 실패, 403은 권한 부족, 404는 권한으로 숨긴 리소스 또는 없는 항목, 409는 충돌 상태, 413은 크기 초과, 429는 quota 초과, 5xx는 일시 서버 오류다. 오류에 요청 원문을 echo하지 않는다.

모든 제출과 결과 보고는 idempotency key를 갖는다. gatekeeper job을 재시도해도 같은 결과와 score event가 두 번 만들어지지 않는다. 익명 요청은 신뢰된 peer IP 기준 bucket을 적용한다. 인증 요청은 account bucket과 IP bucket을 모두 통과해야 하며 account allowance가 더 높더라도 IP ceiling은 유지한다. 각 endpoint class의 pilot quota와 `Retry-After`, optional rate headers, store outage behavior는 [`rate-limits.md`](rate-limits.md)에 정의한다. 기여 제출은 account/IP와 기존 idempotency 제한을 함께 적용한다.

## 연결 테스트

API-30~46은 recall, contribution ownership, OAuth claims, receipt, dashboard analytics, rate-limit headers, owner usage endpoint, leaderboard와 guide template endpoint를 계약 수준에서 확인한다. API 스키마와 MCP tool schema는 동일한 core contract에서 생성되어 호스트별 필드 이름 차이를 줄인다.
