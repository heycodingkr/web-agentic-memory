# 관리자 이메일 인증과 기여 검수

2026-11-01 구현 후 정리한 기능 계약이다. 관리자 화면과 API는 운영 릴리스 `20261101T075251Z-admin`에 처음 배포했고, generic login response(#175)와 재시작을 견디는 링크 전달 outbox(#178)는 릴리스 `20261101T140344Z-main-301c5a1`(커밋 `301c5a1`)에 배포했다. 첫 릴리스에서 DNS/HTTPS·비인가 경로·Arise 실제 메일 수신/링크 로그인·새로고침 세션 유지·로그아웃을 확인했고, 새 릴리스에서는 비허용 주소의 generic 202와 전달 항목 0, 허용 주소의 링크가 outbox를 거쳐 접수되는 것을 확인했다. 새 릴리스의 받은 메일 링크로 로그인하는 실제 수신 검증과 Muse Gateway worker/replay 게시 수용은 대기다. 가입자 목록 API는 아직 없다. generic 응답·비동기 전달 수정은 #175, 가입자 목록·상세는 #176, 프로세스 재시작 뒤 전달 내구성은 #178로 추적한다. 테스트와 결과는 [관리자 테스트 명세](../test/admin.md)에 있고, 기존 콘솔 구현은 #172다.

## 요구사항과 테스트 연결

| 요구사항 | 계약 | 관련 테스트 |
|---|---|---|
| F-ADMIN-01 설정과 허용 이메일 | 서버 전용 `WAM_ADMIN_EMAIL` 한 주소만 허용한다. 앞뒤 공백과 ASCII 대소문자는 정규화하고 Gmail 점·plus alias를 합치지 않는다. `WAM_ADMIN_ORIGIN`은 고객 origin과 분리한다. 실제 주소는 Git 제외 env에만 보관한다. 시작 응답은 허용 주소 존재 여부를 노출하지 않는다 | ADMIN-01~03, ADMIN-11, ADMIN-26, ADMIN-28, ADMIN-32; ADMIN-PROD-02/04 |
| F-ADMIN-02 이메일 링크 | 허용된 주소에만 256비트 링크를 비동기로, 프로세스 재시작을 넘어 남는 outbox에서 전달하고(실패는 5번까지 재시도) 요청 브라우저의 별도 난수 쿠키와 함께 검증한다. 링크는 600초 동안 한 번만 사용한다. 재발송은 60초 간격, 시간당 최대 5회이며 이전 링크를 무효화한다. 발송 시작은 주소·메일 제공자 결과를 응답으로 구분하지 않는다 | ADMIN-02, ADMIN-04~07, ADMIN-12, ADMIN-17~18, ADMIN-23~24, ADMIN-32~35 |
| F-ADMIN-03 독립 세션 | 관리자 전용 쿠키와 2시간 절대 만료 세션을 사용한다. 로그아웃과 허용 이메일 변경은 기존 권한을 무효화한다. 고객 로그인·CLI/MCP·GitHub 권한이 관리자 권한으로 승격되지 않는다 | ADMIN-08~10, ADMIN-16, ADMIN-21, ADMIN-25~26 |
| F-ADMIN-04 검수 조회 | 관리자만 전체 기여 요청과 상세를 조회한다. 목록/상세에서 요청 계정, 정규화 웹 URL pattern, 요청 task·action 순서, 제안 메모리와 replay 결과, active memory/version, 처리 사유와 이력을 연결한다. 검색·상태 필터·최신순 cursor 목록을 제공한다. 원문 페이지, URL query/fragment, secret은 노출하지 않는다 | ADMIN-13, ADMIN-19, ADMIN-21~22, ADMIN-30 |
| F-ADMIN-05 검수 결정 | 검수 시작은 durable worker에 검증을 요청한다. 검증된 제안이 없는 결정은 worker를 다시 요청하고 게시하지 않는다. 검증 제안 게시와 반려를 분리해 명시적으로 확인하며 현재 review generation에만 적용하고 감사 이력을 남긴다. 동의·scope·schema·replay 검증을 생략하지 않는다 | ADMIN-13~15, ADMIN-20, ADMIN-31; ADMIN-PROD-07 |
| F-ADMIN-06 API 경계 | 관리자 host, 허용 Origin, 관리자 세션을 검사한다. 변경 요청은 CSRF 토큰도 검사한다. Authorization bearer를 관리자 자격으로 받지 않는다. 기존 `/v1/review/*`에도 같은 경계를 적용하고 production에서 개발용 review token을 받지 않는다 | ADMIN-09~10, ADMIN-14, ADMIN-16, ADMIN-20, ADMIN-26~27; ADMIN-PROD-02~03 |
| F-ADMIN-07 비밀값과 브라우저 | 이메일·링크·세션 원문을 영속 identity 상태에 저장하지 않는다(전달 outbox도 링크 토큰이 아니라 nonce만 저장한다). 인증 토큰을 URL fragment에서 읽고 주소 표시줄에서 제거하며 localStorage에 보관하지 않는다. 요청 데이터는 HTML로 실행하지 않는다. 비공개 API의 오류를 포함한 응답은 no-store다 | ADMIN-01, ADMIN-08~09, ADMIN-18~19, ADMIN-21, ADMIN-24~25, ADMIN-36; ADMIN-PROD-04 |
| F-ADMIN-08 가입자 조회 | 관리자만 가입 계정 목록/상세를 조회한다. cursor/page size를 제한하고 목록 이메일은 마스킹하며 토큰·IP·인증 정보는 제외한다 | ADMIN-29, SEC-D18~19; #176 |
| F-ADMIN-09 Muse Gateway 검수 | 실제 지정 provider/model ID를 감사하고 모델 결과는 structured candidate로만 받는다. 중복·모호·오류 결과는 게시하지 않고 replay 통과 candidate만 현재 generation에서 명시적으로 게시한다 | ADMIN-31, CUR-17, MODEL-09, ACC-ADMIN-01 |
| F-ADMIN-10 주소 열거·요청 프라이버시 | 시작 응답은 허용/비허용 주소에 구별되는 공개 신호를 주지 않는다. 요청 URL/task/제안은 admin 전용 no-store 상세에서만 노출한다 | ADMIN-28, ADMIN-30, ADMIN-32, SEC-D15~21 |

테스트 ID가 연결되어 있어도 모든 세부 조건의 실행 완료를 뜻하지 않는다. 추가 회귀가 필요한 조건과 미실행 운영 수용은 [테스트 명세](../test/admin.md)의 상태를 따른다.

## API 계약

운영 origin은 `https://admin.webagenticmemory.com`이다. 본문은 JSON이며 변경 요청의 Origin은 이 origin과 정확히 일치해야 한다.

| 요청 | 입력 또는 자격 | 성공 | 주요 거부 |
|---|---|---|---|
| `POST /v1/admin/auth/start` | `{email}`, 관리자 Origin/host | 모든 주소에 동일한 202·body·header shape·응답 크기; 허용 주소만 비동기 메일 발송, 발송 결과는 본문에 반영하지 않음 | 이메일 일치 여부를 노출하는 202/400 차이는 허용하지 않음 (#175); origin/host 403, 미설정 503 |
| `POST /v1/admin/auth/verify` | `{token}`, pending 쿠키, 관리자 Origin/host | 200, CSRF·만료 시각·관리자 세션 쿠키; pending 삭제 | 누락/만료/사용한 링크·다른 브라우저 401, origin/host 403 |
| `GET /v1/admin/auth/session` | 관리자 세션·host | 200, CSRF·만료 시각 | 미로그인/만료/미설정 401, host 또는 제공된 Origin 불일치 403 |
| `POST /v1/admin/auth/logout` | 관리자 세션·Origin/host·CSRF | 204, 세션 철회·쿠키 삭제 | 세션 401, Origin/CSRF 403 |
| `GET /v1/admin/contributions` | 관리자 세션·host | 목록·상태 집계·작업 큐 상태·결정 이력 | 세션 401, 관리자 미설정 503 |
| `GET /v1/admin/contributions/{id}` | 관리자 세션·host | 요청·계정·제안·재실행 결과 상세 | 세션 401, 없는 ID 404 |
| `POST /v1/admin/contributions/{id}/claim` | 관리자 세션·Origin/host·CSRF | 검수 상태와 generation | 미인증 401, CSRF 403, 상태 충돌 409 |
| `POST /v1/admin/contributions/{id}/decision` | 위 자격과 `{review_generation, decision}` | 현재 generation 검증 요청/검증된 제안 게시/반려 결과 | 미인증 401, CSRF 403, 오래된 generation 409 |
| `GET /v1/admin/accounts?cursor=&limit=&q=` (추가 구현 필요) | 관리자 session·host | 가입자 목록, masked email, 가입시각, 상태별 기여 수; 안정적 cursor | 현재 route/화면 없음; #176. 토큰·device secret·raw IP 반환 금지 |
| `GET /v1/admin/accounts/{id}` (추가 구현 필요) | 관리자 session·host | 마스킹된 계정 식별 정보와 해당 계정의 기여 상태 요약 | 현재 route/화면 없음; #176. 없는 계정은 404; 인증·session/device secret·raw IP 반환 금지 |

`start`·`verify`·관리자 `decision` 본문은 4 KiB로 제한한다. 관리자 설정이 활성화되면 기존 `/v1/review/contributions`와 하위 claim/decision도 관리자 세션으로만 접근한다. 관리자 vhost는 고객용 `/v1/auth/*`를 404로 처리한다.

## 가입자 목록, URL/task와 Muse Gateway

가입자 목록은 현재 관리자 화면/API에 없으며 #176으로 구현을 추적한다. 목표 목록은 내부 account ID, 인증 여부, 기본 마스킹 이메일, 가입 시각, 계정 상태, 기여 상태별 건수를 제공하고 page size를 최대 100으로 제한한다. cursor는 `(created_at,id)`처럼 안정적인 keyset이어야 한다. 전체 인증 이메일이 필요한 경우 별도 관리자 상세에서만 제공한다. 고객 access token, device token, session cookie, 인증 코드, raw IP는 반환하지 않는다.

기여 목록 행은 요청 ID·접수 시각·상태/진행 단계·canonical `url_pattern`·task category·action 순서를 보여준다. 상세는 “어떤 웹 URL에서 어떤 agentic memory 작업을 요청했는지”를 확인할 수 있도록 해당 요청의 URL pattern, 요청된 task/actions, 제안 memory, replay 근거와 기존 memory/version을 연결한다. URL의 query/fragment/userinfo/tracking/dynamic identifiers를 제거하고 서버가 사용자가 제출한 URL을 fetch하지 않는다. 제안·사용자 텍스트는 untrusted text로 렌더링하고 account identifier 이외의 이메일은 기본 마스킹한다.

`Muse 검증 요청`은 현재 generation의 durable worker만 enqueue/retry하고 그 자체로 게시를 표시하지 않는다. 검증 결과가 생기면 provider와 실제 model ID, reason code, versioned instruction/tool schema와 digest를 보여준다. 모델 장애·malformed output·scope/schema/version 오류·replay 실패는 fail closed로 대기/사람 검토 상태에 남긴다. 검증된 제안을 공개하는 행위는 별도 관리자 결정으로 하고, server transaction만 active immutable version, contribution reflection, attribution/score와 audit/outbox를 갱신한다. 중복/support/reject는 새 공개 version이나 점수를 만들지 않는다.

관리자 응답은 no-store이며 낙관 UI로 claim, publish, 점수, 반영 결과를 먼저 표시하지 않는다. 목록은 cursor pagination과 좁은 projection을 사용하고 상세 JSON은 목록에서 제외한다. 인덱스는 실제 query plan으로 결정하며 후보는 `(submitted_at DESC,id)`, `(status,submitted_at DESC,id)`, `(account_id,submitted_at DESC,id)`, `(created_at DESC,id)`다. 중복 인덱스를 만들지 말고 EXPLAIN/latency/write cost를 함께 측정한다.

## 저장과 배포

허용 주소의 link 전달은 HTTP 응답을 기다리게 하지 않고, 프로세스가 재시작해도 남는 outbox에서 수행한다(#178, 아래 "링크 전달 outbox"). 전달의 성공·실패는 응답에 나타나지 않고 비밀값 없는 서버 진단으로만 남는다.

**링크 전달 outbox.** 시작 요청은 challenge와 rate limit 기록, 전달 항목 하나를 같은 저장 transaction에 쓴 뒤 응답한다. 항목에는 링크도 토큰도 이메일도 저장하지 않는다. 항목이 가진 것은 무작위 nonce와 시도 횟수, 다음 시각, 임대 시각, 만료 시각이고, 링크 토큰은 필요할 때마다 `HMAC-SHA256(이메일 identity secret, "wam-admin-link-v1:" + nonce)`로 다시 계산한다(256비트, 43자). identity secret은 저장하지 않으므로 저장 상태만으로는 링크를 만들 수 없고, 링크가 있어도 요청 브라우저의 pending 쿠키(binding)가 없으면 소비할 수 없다. 수신자는 설정된 `WAM_ADMIN_EMAIL`이라 항목에 적지 않는다.

전달 worker는 서버가 시작할 때, 시작 요청 직후, 그리고 항목이 남아 있는 동안 2초마다(항목이 없으면 다른 인스턴스가 넣었을 수 있어 30초마다) 만기 항목을 가져가 보낸다. 저장소 I/O는 비동기 worker 스레드 밖에서 한다. 새 설정은 없고 관리자 로그인이 설정된 서버는 시작할 때 worker를 함께 시작한다. 가져갈 때 60초 임대를 걸어서 인스턴스가 여럿이어도 한 곳만 보낸다. 임대가 끝난 항목(보내다 프로세스가 죽은 경우)은 다른 프로세스가 같은 토큰과 같은 `Idempotency-Key`(`wam-admin/<토큰 SHA-256>`)로 다시 보내므로, 제공자가 같은 key를 한 번으로 처리하는 한 같은 메일이 두 번 가지 않는다(제공자 동작은 실제로 확인하지 않았다. Resend 문서의 idempotency를 가정한다). 보내기가 실패하면 5, 15, 30, 60초 뒤에 다시 보내고 5번째 실패나 링크 만료(600초)에서 pending challenge를 취소하고 항목을 지운다. 이전과 달리 한 번의 실패로 취소하지 않는다. 성공하면 항목을 지운다. 이전 릴리스가 저장한 상태에는 항목 필드가 없고 읽을 때 기본값(`serde(default)`)이 빈 목록이며, 이전 릴리스로 되돌려도 항목은 무시된다(그 릴리스는 이 링크를 전달하지 않으므로 10분 안에 다시 요청해야 한다).

링크와 pending binding, 세션 원문은 난수로 생성하고 저장 시 SHA-256으로 처리한다. 허용 이메일과 origin에 대한 식별자는 기존 이메일 identity secret을 사용하는 HMAC으로 묶는다. 검증과 링크 소비·세션 생성은 하나의 저장 transaction에서 실행하며 PostgreSQL 인스턴스 간에도 공유한다.

운영 쿠키는 `__Host-wam_admin_pending`과 `__Host-wam_admin_session`이며 `Secure; HttpOnly; SameSite=Lax; Path=/`를 사용하고 Domain을 지정하지 않는다. 관리자 계정에도 공유 IP/account 요청 제한을 적용한다. 실제 주소·전달 키·인증 토큰을 브라우저 번들이나 공개 문서에 넣지 않는다.

로컬 `credentials/admin.env`와 서버 `/etc/web-agentic-memory/admin.env`는 Git에서 제외하고 권한 600을 유지한다. 서버는 systemd 추가 EnvironmentFile로 읽는다. DNS·HTTPS·기존 릴리스/DB 백업과 복구 절차는 [4090 배포 안내](../../deploy/4090/README.md)를 따른다.

## 2026-11-10 보안 보완 계약 (#415–417)

이 절과 아래 테스트 입력/기대값은 구현 변경 전에 작성한다.

- 공개 문서·예제·fixture는 예약된 example 도메인 주소만 진단 수신 예제로 사용한다. 실제 관리자 주소는 private env에만 둔다. 배포 스크립트는 진단 수신자가 없으면 실제 주소를 채우지 않고, 명시된 private 설정은 그대로 보존한다. 이미 공개된 Git 이력은 일반 수정 커밋으로 지워지지 않으며 관리자 주소 변경은 별도의 실제 수신 주소가 필요하다.
- 관리자 인증의 네 고정 경로 start/verify/session/logout에는 서버가 생성한 `admin_auth` 메타데이터를 기록한다. `action`은 이 네 값만 허용하고 결과는 기존 HTTP method/status로 구분한다. start의 202는 접수 응답이며 메일 발송 성공이나 허용 주소 일치를 의미하지 않는다.
- `peer_tag`는 기존 rate-limit secret을 사용한 HMAC-SHA256으로 만든 64자리 소문자 hex다. 입력은 별도 관리자 감사 domain, UTC 일자와 기존 trusted-proxy 규칙을 거친 정규화 IP다. 같은 날짜·접속원은 연관 지을 수 있고 날짜·키·접속원이 바뀌면 다른 값이다. 원본 IP·헤더·이메일·쿠키·토큰·URL/query·본문은 저장하지 않는다.
- 관리자 인증 trace ID와 source는 서버가 생성·분류한다. 요청자가 보낸 trace ID, client source, 감사 메타데이터로 이를 위조할 수 없다. 신뢰하지 않는 peer의 X-Forwarded-For는 무시한다. 감사 정보는 응답 body/header에 추가하지 않는다.
- 기존 StoredDiagnostic JSON에 optional 메타데이터를 추가해 이전 행을 계속 읽는다. 기존 SQLite/PostgreSQL 저장, 7일 보관·행 수·큐 상한과 dropped-events 관측을 사용하고 공개 로그 조회 API는 만들지 않는다. 감사 큐/저장 장애가 인증 결과를 바꾸지 않는다.
- GitHub secret scanning과 push protection을 활성화한 뒤 REST API로 상태를 재조회한다. 초기 경고는 비밀값 없이 분류하며 합성 corpus 전체를 넓게 제외하지 않는다.

원자 작업 순서: G1 사전 상태·선행 실패 확인(여기서 수행) → G2 공개 주소 치환(반복 치환만 codex:gpt-6-luna; 부모가 각 diff와 실제 private 주소 부재를 독립 확인) 및 G3 GitHub 보호 활성화(여기서 수행, G1에 의존) → G4 서버 감사 구현(여기서 수행, G1 명세·실패에 의존) → G5 통합 테스트·독립 검토·고정 커밋 배포·운영 재검증(여기서 수행, G2–G4에 의존). 소스·배포에 기존의 다른 작업 변경을 포함하지 않는다.
