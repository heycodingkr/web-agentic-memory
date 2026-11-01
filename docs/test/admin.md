# 관리자 인증과 기여 검수 테스트

2026-11-01 구현·배포 후 테스트 명세를 보완했다. 기능 계약은 [관리자 명세](../spec/admin.md), 배포 절차는 [4090 운영 안내](../../deploy/4090/README.md), 기존 관리자 콘솔 구현은 #172, 주소 열거 회귀는 #175, 가입자 목록·상세는 #176, durable mail delivery 후속은 #178에서 추적한다.

2026-11-01 로컬 수정 후 관리자 backend 테스트 9개, 실제 PostgreSQL의 관리자 다중 인스턴스 테스트 1개, 브라우저 2개 시나리오를 desktop/mobile로 실행한 4개가 통과했다. UI 테스트의 API와 메일 발송자는 fake이며 PostgreSQL은 격리한 실제 DB다. 기존 운영 릴리스에서는 HTTPS·비인가 요청 차단뿐 아니라 Arise의 실제 Gmail 수신→메일 링크 로그인→새로고침 세션 유지→로그아웃과 관리자 API 401까지 확인했다. 후속 working tree의 generic/background mail 변경은 미배포이므로 별도로 재검증한다. ADMIN-28과 ADMIN-32 주소 열거·provider latency 회귀는 현재 working tree에서 통과했다. ADMIN-29 가입자 목록과 ADMIN-31 실제 Muse/replay는 자동/운영 수용 대기다.

## 자동 테스트와 케이스 대응

같은 테스트 함수가 여러 케이스를 검증한다. 아래 B/D/U 표기는 실행 가능한 코드와의 대응이며, 케이스 ID 수와 테스트 함수 수는 다르다.

| 표기 | 코드와 테스트 함수 |
|---|---|
| B1 | [admin_auth.rs](../../crates/wam-server/tests/admin_auth.rs) — `unconfigured_admin_auth_fails_closed` |
| B2 | 같은 파일 — `magic_link_http_flow_is_browser_bound_one_time_and_revocable` |
| B3 | 같은 파일 — `only_allowlisted_email_sends_and_origin_csrf_and_bearer_bypasses_are_blocked` |
| B4 | 같은 파일 — `links_expire_rate_limit_persist_and_allowlist_changes_revoke_sessions` |
| B5 | 같은 파일 — `authorized_review_details_and_generation_fenced_decisions_are_audited` |
| B6 | 같은 파일 — `concurrent_redeem_and_reissued_links_cannot_create_multiple_sessions` |
| B7 | 같은 파일 — `admin_config_requires_an_isolated_safe_origin_and_valid_email` |
| B8 | 같은 파일 — `failed_delivery_cannot_leave_a_usable_link` |
| B9 | 같은 파일 — `admin_start_response_does_not_wait_for_email_delivery` |
| D1 | [email_auth_postgres.rs](../../crates/wam-server/tests/email_auth_postgres.rs) — `admin_links_and_sessions_are_shared_one_time_and_revocable_across_postgres_replicas` |
| L1 | [application_lifecycle.rs](../../crates/wam-server/tests/application_lifecycle.rs) — `manual_approval_requires_a_replay_verified_proposal_and_low_confidence_is_reviewed` |
| U1 | [admin.spec.ts](../../web/e2e/admin.spec.ts) — `email login, private layout, and link fragment removal` |
| U2 | 같은 파일 — `review list, detail, confirmation, csrf, filters and expired session` |

### 인증과 API

사전조건은 별도 HTTP router와 일회용 file repository, fake mail sender다. 정상 이메일은 `admin@example.test`, 고객과 관리자의 origin은 서로 다르다. 링크·세션 원문은 fake sender와 응답에서만 받아 검사하며 실제 고객 메일함이나 운영 DB를 사용하지 않는다.

| ID | 동작 | 합격 기준 | 대응과 결과 |
|---|---|---|---|
| ADMIN-01 | 관리자 미설정 router에서 발송·세션·목록 요청 | 발송/목록 503, 세션 401; 모두 no-store | B1 통과 |
| ADMIN-02 | 허용 이메일로 발송 요청 | 202 generic accepted, 동일한 공개 response shape, 600초 링크와 `__Host-` Secure pending 쿠키; fake sender에 비동기 링크 전달 | B2 통과 |
| ADMIN-03 | 다른 주소·plus alias·비슷한 도메인·CRLF 주소로 발송 요청 | 허용 주소와 같은 202/body/header/cookie shape; fake sender 발송 기록 없음; 발급한 유효 관리자 링크와 조합해도 verify 실패 | B3 통과 (#175 수정) |
| ADMIN-04 | 유효 링크를 pending 쿠키 없이 검증한 뒤 올바른 쿠키로 검증하고 재사용 | 쿠키 없이는 401, 올바른 조합은 200, 같은 링크 재사용은 401 | B2 통과 |
| ADMIN-05 | 결정적 시계로 링크 600초·세션 2시간의 만료 경계 검사 | 링크는 600초에 거부, 세션은 발급 시각+7200초에 거부 | B4 통과 |
| ADMIN-06 | 공백·대문자를 포함한 허용 주소로 로그인하고 60초 전/후 재발송 | 정규화 주소 허용; 60초 전 RateLimited; 새 링크 발급 후 이전 링크 거부 | B4/B6 통과 |
| ADMIN-07 | 동일 링크를 두 스레드에서 동시에 검증 | 정확히 한 요청만 세션 발급 | B6 통과 |
| ADMIN-08 | 저장 snapshot을 다시 열고 로그인·허용 이메일 변경·원문 검색 | 복원한 링크/세션 사용 가능; 허용 주소 변경 뒤 기존 세션 거부; 저장 JSON에 이메일·binding·링크·세션 원문 없음 | B4 통과 |
| ADMIN-09 | 고객 demo bearer·기존 reviewer bearer·고객 형식 쿠키·잘못된/중복 host·다른 Origin·same-site fetch로 관리자/기존 review API 접근 | 성공 응답 없음; bearer 거부 응답 no-store; claim/decision/logout의 누락·오답 CSRF 또는 누락 Origin은 403 | B3 통과. 실제 발급 고객 세션/기기 토큰 조합은 ADMIN-26 |
| ADMIN-10 | 정상 세션으로 세션/목록 조회 후 CSRF를 포함해 로그아웃하고 다시 조회 | 조회 200, 로그아웃 204, 같은 세션의 목록 조회 401 | B2 통과 |
| ADMIN-11 | 고객과 같은 origin, 원격 HTTP, userinfo, path/query/fragment, 다른 scheme 설정 | 관리자 설정 거부 | B7 통과 |
| ADMIN-12 | fake mail sender가 실패하고 잘못된 binding으로 검증 | 발송은 Provider 오류, 검증 실패 | B8 통과. 올바른 binding의 취소 검증·Resend wire/error 전체 범위는 ADMIN-24 |
| ADMIN-13 | 합성 고객 기여를 조회하고 검수 시작→반려 | 상세 계정/대기 상태 확인, 제안 없음 표시, claim 성공, 반려 상태·집계·`email-admin` 감사 이력 기록 | B5 통과 |
| ADMIN-14 | 현재와 다른 review generation으로 결정 | 409; 이어 현재 generation의 결정은 성공 | B5 통과 |
| ADMIN-15 | replay 검증된 제안이 없는 수동 승인과 검증된 제안의 승인 | 제안이 없으면 검증을 요청하고 검증 절차를 생략해 게시하지 않음; 검증된 제안은 기존 게시 계약 유지 | L1 기존 lifecycle 회귀 통과 |

ADMIN-09에서 고객 형식 쿠키는 합성 문자열이다. 정상 고객 로그인으로 발급한 세션의 거부를 검증한 것으로 확대하지 않는다. B4의 파일 복원은 snapshot 복사본을 새 repository로 여는 방식이며, 여러 애플리케이션 인스턴스의 공유는 다음 D1이 별도로 검증한다.

### 실제 PostgreSQL

| ID | 사전조건과 동작 | 합격 기준 | 대응과 결과 |
|---|---|---|---|
| ADMIN-16 | 소유한 disposable PostgreSQL에 별도 schema를 만들고 두 repository에서 발송·제한·동시 검증·재접속·로그아웃 | 60초 제한 공유; 동시 검증 성공 1개; 새 repository에서도 세션 인증; 한 인스턴스의 로그아웃이 나머지 인스턴스에도 적용 | D1 통과 |

D1은 기본 실행에서 ignored다. `WAM_TEST_DATABASE_URL`을 격리 DB로 설정하고 `--ignored`를 명시한 실행만 실제 DB 통과로 집계한다. 같은 파일의 고객 이메일 DB 테스트도 함께 실행하여 총 2개가 통과했으며 관리자 케이스는 그중 1개다.

### 브라우저와 데이터 제거

브라우저 설정은 [playwright.admin.config.ts](../../web/playwright.admin.config.ts)이며 Chrome desktop과 Pixel 7 mobile을 사용한다. 이 계층은 API를 route fixture로 대체하므로 실제 백엔드·메일 수신·게시 성공을 판정하지 않는다.

| ID | 동작 | 합격 기준 | 대응과 결과 |
|---|---|---|---|
| ADMIN-17 | 미인증 관리자 페이지에서 로그인 링크 요청 | 일반 고객 navigation 없음; 요청 접수 안내가 주소 허용 여부를 단정하지 않음; 재전송 대기 버튼 disabled | U1, 두 화면 크기 통과 |
| ADMIN-18 | fragment에 합성 토큰이 든 verify 페이지 진입·인증 실패 | fragment 즉시 제거; localStorage 비어 있음; 사용자가 완료 버튼을 눌러야 검증 요청; 실패 안내 표시 | U1, 두 화면 크기 통과 |
| ADMIN-19 | HTML 이벤트 handler가 포함된 합성 메모리 상세 열기 | 내용을 텍스트로 표시; 삽입한 handler 실행 없음 | U2, 두 화면 크기 통과 |
| ADMIN-20 | 검수 시작·반려 버튼·확인 버튼 순서로 처리 | claim/decision에 CSRF 전달; 반려 버튼만 눌렀을 때 결정 요청 0회, 확인 후 1회; 현재 generation 전달 | U2, 두 화면 크기 통과 |
| ADMIN-21 | 검색 결과 없음/검색 해제, 이어 API가 401을 반환하도록 하고 새로고침 | 검색 상태 정상; 가로 overflow 없음; 재로그인 화면으로 전환하고 이전 계정 ID·요청 경로 제거 | U2, 두 화면 크기 통과 |

U2의 desktop/mobile 대표 PNG는 이미지 검사 도구로 열어 레이아웃을 확인했다. 상태 필터와 최신순 정렬, 승인 버튼 분기는 U2가 직접 조작하지 않으므로 다음 추가 회귀 목록에 둔다.

## 추가 회귀 기준

아래는 미완료 회귀 계약이다. 이미 자동화된 케이스는 결과 열에 통과 근거를 구분해 적는다.

| ID | 동작 | 합격 기준 | 상태 |
|---|---|---|---|
| ADMIN-22 | 여러 상태의 목록에서 필터·최신순 정렬·빈 목록·승인/취소 분기를 조작 | 선택 상태/정렬/집계가 일치; 승인 확인 전 요청 없음; 취소 시 변경 없음 | 자동화 추가 예정 |
| ADMIN-23 | 결정적 시계로 시간당 발송 5회 상한·3600초 경계·실패 발송·동시 발송 검사 | 6회째 차단, 기간 경계에서 회복, 실패/동시 요청으로 상한 우회 불가 | 60초 제한만 자동 통과; 나머지 추가 예정 |
| ADMIN-24 | 관리자 메일 payload·HTML 버튼·idempotency 및 timeout/redirect/4xx/5xx/잘못된 receipt·올바른 binding으로 실패 링크 검증 | 올바른 주소/링크만 전송; 공급자 실패는 인증 불가; 취소된 링크 소비 불가; 비밀 응답 반사 없음 | 관리자 전용 wire/error 회귀 추가 예정 |
| ADMIN-25 | 쿠키 모든 속성·중복 쿠키·본문 초과·잘못된 JSON/unknown field·에러 응답 검사 | Secure/HttpOnly/SameSite/Path/Domain 정책, 모호한 쿠키·본문 거부, 토큰 반사 없음, no-store 유지 | 일부 prefix/Secure/no-store 통과; 전체 행 추가 예정 |
| ADMIN-26 | 실제 발급 고객 이메일/GitHub 세션·CLI/MCP 토큰, 관리자 env 부분 설정, 허용 origin 변경 | 고객 권한 승격 불가; 부분 설정 시작 실패; origin 변경 뒤 이전 관리자 세션 거부 | 합성 bearer·고객 형식 쿠키·주소 변경만 자동 통과; 나머지 추가 예정 |
| ADMIN-27 | 동일 IP의 관리자 요청을 연속 실행하고 Redis 장애·quota 소진 주입 | 관리자 account bucket 적용, 제한 시 429, limiter 장애 시 안전하게 거부하며 상태/인증 변경 없음 | 관리자 전용 부하/장애 회귀 추가 예정 |
| ADMIN-28 | 허용/비허용 이메일의 `POST /v1/admin/auth/start` 관찰 응답 비교 | status/body/header/cookie shape가 같고 비허용 주소로 메일 발송 0건; 재요청 제한은 유지하되 제한 응답도 주소를 노출하지 않음 | B3 통과. 운영 배포 후 ADMIN-PROD-02 재확인 필요 (#175) |
| ADMIN-29 | 가입자 목록/상세의 cursor, 검색, 권한과 반환 필드 검사 | 관리자 전용 paging; 목록 이메일 마스킹; token/device secret/raw IP 없음 | `/v1/admin/accounts` 미구현, #176; 자동화 추가 예정 |
| ADMIN-30 | 합성 기여의 요청 URL/task/action과 제안 memory/replay/detail 표시 | canonical URL pattern과 요청 작업을 연결; query/fragment/raw page/secret 없음 | U2가 일부 검증; URL/task/result 조합 fixture 확대 예정 |
| ADMIN-31 | Muse Gateway contract/model ID, worker replay, 명시 게시, 실패/중복 경로 | 실제 provider/model ID가 audit에 남고 replay 검증된 현재 generation만 게시; 장애/중복/support/reject는 공개 version/score 변경 없음 | fake/live provider full acceptance와 #159 worker/replay production gate 대기 |
| ADMIN-32 | 허용 주소의 fake email provider를 지연시킨 상태에서 로그인 시작 응답과 provider 완료를 비교 | provider 지연과 무관하게 generic 202 response를 반환; 비허용 주소에는 provider 호출 0회 | B9 통과, 350ms fake delay에서 요청 150ms 안에 반환. 허용/비허용 간 DB 경로 timing 통계와 프로세스 재시작 내구성(#178)은 추가 점검 |

## 운영 수용 기준과 실행 상태

공개 서비스에는 아래 읽기 전용·비인가 요청과 명시된 로그인 메일 발송만 수행했다. 검수/게시 수용은 격리 staging 또는 승인된 테스트 계정의 합성 기여를 사용하며 고객 기여를 테스트 데이터로 변경하지 않는다.

| ID | 절차 | 합격 기준 | 2026-11-01 결과 |
|---|---|---|---|
| ADMIN-PROD-01 | admin DNS·HTTPS·루트 페이지 접속 | 인증서 정상, `/admin`으로 이동 후 200; no-store·frame DENY | 통과 |
| ADMIN-PROD-02 | 다른 이메일 발송, 다른 Origin 발송, 임의 링크 verify | 각각 202 generic·403·401, 모두 no-store; fake/live resender에 비허용 주소 전달 0 | 이전 배포는 비허용 주소 400으로 관찰되어 새 계약에 미달; generic response 수정은 아직 배포 전, 재검증 대기 |
| ADMIN-PROD-03 | 미로그인 관리자 session/list/claim, 기존 review bearer, admin host 고객 auth API | 401·401·401, 기존 bearer 403, 고객 auth API 404 | 통과 |
| ADMIN-PROD-04 | Git 제외·env 권한·웹 artifact·서비스·이전 릴리스/DB 백업 확인 | 실제 주소/secret이 공개 source·번들에 없음; env 600; 서비스 정상; 복구 자료 보존 | 통과 |
| ADMIN-PROD-05 | 허용된 수신 주소로 관리자 UI에서 로그인 링크 요청 | UI는 generic 접수 안내만 표시; 서버 진단에서 provider acceptance 확인 | 기존 배포의 Resend 접수 기록은 있음; 새 비동기/generic-response 구현은 미배포, 재검증 대기 |
| ADMIN-PROD-06 | 받은 메일 버튼을 요청한 브라우저에서 열고 완료→목록→새로고침→로그아웃 | 실제 수신, 관리자 세션·목록 유지, 로그아웃 뒤 401; 재사용/다른 브라우저 거부 | 기존 배포 Arise에서 실제 수신·로그인·새로고침·정상 로그아웃 통과. session/list/기존 review 200→로그아웃 뒤 401; CSRF 없는 로그아웃 403 후 세션 200 유지. 사용한 링크 재요청 401. 별도 브라우저 프로필 격리와 유효기간 내 재사용 원인 분리는 live 미실행 |
| ADMIN-PROD-07 | 전용 합성 기여를 조회·claim·승인/반려하고 게시/감사 결과 확인 | 화면·API·DB 이력 일치; 동의/검증/generation 조건을 만족하는 경우만 게시 | 미실행. 실제 worker/replay 수용은 #159와 함께 확인 |

운영 메일 발송 접수와 자동 테스트의 fake sender를 실제 메일 수신의 증거로 대체하지 않는다.

## 재실행 명령

코어 관리자 검사는 외부 메일을 보내지 않는다.

```sh
cargo test --locked -p wam-server --test admin_auth
cargo test --locked -p wam-server --lib --test api
cargo test --locked -p wam-server --test email_auth --test oauth_flow --test application_lifecycle --test shutdown
```

PostgreSQL은 소유한 일회용 DB의 `WAM_TEST_DATABASE_URL`을 비공개 환경에 설정한 뒤 실행한다. 운영 DB URL을 사용하지 않는다.

```sh
cargo test --locked -p wam-server --test email_auth_postgres -- --ignored
```

브라우저는 기존 QA 서버 `http://127.0.0.1:5276`을 재사용한다. 서버가 없다면 host detached shell의 `detached:true`로 `pnpm --dir web exec vite dev --host 127.0.0.1 --port 5276 --strictPort`를 시작하고 PID·로그를 기록한다.

```sh
WAM_ADMIN_WEB_ORIGIN=http://127.0.0.1:5276 pnpm --dir web exec playwright test --config playwright.admin.config.ts
pnpm --dir web test
pnpm --dir web check
VITE_SITE_URL=https://webagenticmemory.com VITE_WAM_MCP_ENDPOINT=https://webagenticmemory.com/mcp pnpm --dir web build
```

## 운영 Arise 실메일 재검증과 진단

사용자가 확인한 주소로 로컬·운영의 비공개 관리자 env를 정정한 뒤 실제 로그인된 Gmail에서 수신한 링크로 검증했다. 런타임 코드를 바꾸거나 고객 기여를 생성/승인/반려하지 않았다. 목록은 실제 운영 데이터 0건을 반환했다. 원자료는 Git 제외 `build/deploy-admin/live-mail-auth-check.json`이다.

검사 중 Arise의 화면상 활성 탭과 자동화 대상이 달라지고, 같은 세션 이름의 소유 탭을 `tab_busy`로 거부하는 #27 증상이 재현됐다. 소유자 표시가 세션 이름만 비교하는 것과 별도 제어 토큰이 필요한 것, 제어 핸들이 MCP 프로세스 메모리에만 있는 것을 코드에서 확인했다. 상위 연결 교체의 정확한 계기는 미확정이다. Gmail 본문 전환의 renderer timeout도 별도로 관찰했다.

임시 관찰 표식이 성공 리디렉션 뒤 사라지는데도 이를 계속 기다린 QA 스크립트가 실제 로그인 성공을 실패로 분류했다. 이후 서버의 challenge 소비/세션 생성, 실제 화면과 API 200, 새로고침 유지 및 로그아웃 후 401로 판정했다. 앞선 한 번의 verify 401은 당시 사용 토큰의 비교값을 남기지 않아 정확한 사유를 소급 확정하지 않는다. 관리자 요청이 진단에서 `unknown`으로 분류되는 관측 공백은 #179로 별도 등록했다.

## 실행 증거

대상은 `20261101T075251Z-admin` working-tree snapshot이다. 커밋 HEAD만으로 이 artifact를 식별하지 않고 `source-hashes.json`과 배포 메타데이터를 함께 확인한다. Git push는 수행하지 않았다.

| 검사 | 결과 | Git 제외 로컬 증거 |
|---|---|---|
| 관리자 경로 선행 실패 | 미구현 start가 404로 응답하여 기대한 503과 달라 실패; 구현 후 B1 통과 | #172 실행 기록 |
| Linux 주요 서버 검사 | 배포 당시 관리자 8 + lifecycle 13 + 이메일 7 + OAuth 17 + shutdown 1 통과 | `build/deploy-admin/linux-build-and-tests.log` |
| 2026-11-01 local #175 regression | admin_auth 9/9 통과, 느린 fake mail provider response-independence 포함 | 이번 working tree. production에는 미배포 |
| 격리 PostgreSQL 16 | 관리자 D1 1개와 고객 이메일 1개, 총 2개 통과 | 같은 Linux 로그; disposable DB 컨테이너 정리 |
| macOS library/API | 68 + 19 통과 | `build/deploy-admin/server-api-tests.log` |
| 웹 단위 | 46 통과 | `build/deploy-admin/web-tests.log` |
| Chrome desktop/mobile | 4 통과 | `build/deploy-admin/admin-ui-tests.log`, `web/test-results/admin/` |
| 타입/빌드 | svelte-check 0 errors/0 warnings, production build 성공 | `build/deploy-admin/web-check.log`, `web-build.log` |
| 운영 비인가 요청 | 기대한 400/403/401/404와 no-store 확인 | `build/deploy-admin/public-security-checks.json`; #172의 session/list 읽기 기록 |
| 배포 대상·정적 파일 | source 해시, Caddy 적용, root 전용 env와 DB 백업 확인 | `build/deploy-admin/source-hashes.json`, `caddy-reload.log`; 서버 릴리스 `deployment.json` |
| Arise 실제 운영 이메일 인증 | 실제 Gmail 수신·메일 링크 로그인, session/list/기존 review 200, 새로고침 200, CSRF 누락 로그아웃 403·세션 유지, 정상 로그아웃 뒤 401, 사용 링크 재요청 401 | `build/deploy-admin/live-mail-auth-check.json`; live MeshCode 화면을 이미지 검사하여 로그인 상태 확인 |
| 전체 서버 `--lib --tests` | 120초 제한으로 중단; 전체 통과로 집계하지 않음 | `build/deploy-admin/full-server-tests.log` |

이번 문서 보완은 기존 로그와 테스트 코드의 대응을 확인한 작업이다. 문서 변경을 이유로 실메일 발송·운영 데이터 변경을 반복하지 않았다. 기능 명세와 테스트 ID는 구현 뒤에 보완했으며 선행 명세를 작성했다고 기록하지 않는다.
