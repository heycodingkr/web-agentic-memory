# EMAIL-AUTH: Resend 이메일 인증 로그인

상태: 2026-11-01 선행 기능 명세. 서버·웹 구현과 로컬/격리 PostgreSQL 검증을 완료했다. `https://webagenticmemory.com`과 API의 공개 HTTPS 응답, `/v1/auth/config`의 `email_auth_enabled: true`는 읽기 전용으로 확인했다. 실제 OTP 수신, 신규/기존 계정 로그인·로그아웃, 로그인 뒤 기여·검수·게시 여정은 아직 운영 수용하지 않았다. 자동 검사 계획은 [email-auth.md](../test/email-auth.md), 운영 기준과 이번 공개 점검은 [email-auth-production.md](../test/email-auth-production.md)에 기록한다.

## 사용자 경험

`/login`에서 이메일을 입력하면 Resend로 6자리 일회용 인증 코드를 보낸다. 동일 화면에서 코드를 입력해 소유권을 확인한다. 처음 인증하면 내부 계정을 만들고, 이후에는 같은 계정으로 로그인한다. 비밀번호는 받지 않는다. GitHub 로그인은 설정되어 있으면 계속 제공한다. 이메일과 GitHub 계정은 자동으로 합치지 않는다. UI 기본 언어는 기존 웹 정책에 맞춰 영어로 한다.

코드는 10분 유효하며 최초 요청 브라우저에서만 검증한다. 재전송은 60초 뒤 가능하다. 새 코드는 이전 코드를 무효화한다. 틀린 코드·만료·다른 브라우저·이미 사용한 코드는 로그인시키지 않는다. 실패 후 재시도·이메일 변경·발송 오류를 화면에 표시한다. 인증 후 기존 dashboard, device link, MCP 승인 화면으로 이어지며 임의 외부 return URL을 받지 않는다.

2026-11-06 네트워크 오류 복구 계약 (#366): 로그인 초기 설정·세션 조회가 연결 실패하면 확인 실패를 표시하고 `Retry sign-in check` 버튼으로 같은 화면에서 다시 조회할 수 있어야 한다. 세션 조회의 정상적인 401은 비로그인 상태이며 오류 안내를 표시하지 않는다. 코드 검증의 연결 실패·503은 코드 오답/만료와 구분해서 표시하고 입력한 코드를 보존한다. 코드 발송·검증 POST는 자동 재전송하지 않으며 사용자가 다시 시도한다. 진단의 이메일 start/verify 경로는 닫힌 `auth` 작업으로 분류하고 주소·코드·URL 원문은 기록하지 않는다.

## 가입·로그인·복구 정책

다음은 배포 전 확정할 제품 계약이다. 기존 코드의 계정 생성/세션 동작과 새로 필요한 안내 문구·미수신 도움말을 구분하며, 후자의 구현·운영 검증 완료를 뜻하지 않는다. 배포 수용 기준은 [운영 테스트 케이스](../test/email-auth-production.md)를 따른다.

| 요구사항 | 정책 | 관련 운영 테스트 |
|---|---|---|
| F-EMAIL-01 가입 | 별도 가입 API/비밀번호 입력 없이 동일 이메일 인증 화면을 사용한다. 처음 입력한 주소는 **코드 검증이 성공한 transaction에서만** 내부 계정과 기본 비공개 프로필을 만든다. 발송 요청·오답·만료만으로 계정을 만들지 않는다 | EMAIL-PROD-06, 07, 10 |
| F-EMAIL-02 로그인 | 이미 인증한 같은 정규화 이메일은 같은 내부 account_id로 연결한다. 새 로그인으로 기여·점수·프로필을 새 계정에 복제하거나 초기화하지 않는다 | EMAIL-PROD-08, 09 |
| F-EMAIL-03 통합 진입 | `/login` 하나에서 신규/기존 모두 진행한다. 기본 CTA는 `Continue with email`, 안내는 `We’ll email you a one-time code. If you’re new, an account will be created after verification.`로 한다. 인증 전에 계정 존재 여부를 표시하지 않는다. 가입 링크가 필요하면 같은 화면을 가리키며 새 인증 방식을 만들지 않는다 | EMAIL-PROD-10, 31 |
| F-EMAIL-04 비밀번호 찾기 | 비밀번호를 만들거나 저장하지 않으므로 비밀번호 찾기·재설정·변경 기능과 링크는 제공하지 않는다. 기존 메일함에 접근할 수 있으면 새 인증 코드로 다시 로그인한다. 세션 만료도 같은 흐름이다 | EMAIL-PROD-21, 30 |
| F-EMAIL-05 메일 미수신 | 코드 입력 화면에서 수신 주소 확인·스팸함 확인·60초 후 재전송·주소 오타 수정 경로를 제공한다. 메일 발송 접수 성공을 받은편지함 도착 완료라고 표현하지 않는다. 새 주소 입력은 기존 계정의 이메일 변경이 아니다 | EMAIL-PROD-05, 15, 18, 30 |
| F-EMAIL-06 메일 접근 상실 | 현재 출시 범위에는 로그인 이메일 변경·대체 이메일 연결·기존 계정 소유권 복구를 포함하지 않는다. 먼저 이메일 제공자의 메일함 복구를 안내한다. 다른 이메일 로그인은 별도 계정이며 기존 점수/기여를 자동 이전하지 않는다. 이름·handle·기기 토큰만으로 운영자가 계정을 넘기지 않는다. 실제 계정 복구를 추가하려면 소유권 검증·binding 변경·세션/기기 철회 정책을 별도로 명세하고 구현한다 | EMAIL-PROD-28, 30 |
| F-EMAIL-07 인증 후 목적지 | 일반 가입/로그인 성공 시 [My Page](my-page.md)인 `/dashboard`로 자동 이동하는 것을 목표로 한다. 현재 완료 화면의 버튼 진입에서 보완할 사항이다. MCP 승인과 CLI 기기 연결에서 시작하면 원래 요청으로 돌아가 사용자가 권한을 확인하고 명시적으로 승인/거부한다. 로그인 성공 자체로 앱/기기 접근 권한을 부여하지 않는다. 복귀 값은 허용된 내부 경로/요청 ID만 받는다 | EMAIL-PROD-26, 27; [MYPAGE-01, 02](../test/my-page.md) |

`Use a different email`은 인증 전 수신 주소를 다시 입력하는 동작이다. 로그인한 기존 계정의 이메일을 바꾼다는 의미로 사용하지 않는다. 신규 계정의 공개 프로필은 기본 opt-out이고 점수는 0이며, 이메일 인증이 프로필 공개나 기여 제출 동의를 대신하지 않는다.

## API 계약

| 요청 | 입력 | 성공 | 실패 |
|---|---|---|---|
| `GET /v1/auth/config` | 없음 | 기존 필드와 `email_auth_enabled: boolean` | 비밀값·발신 설정을 반환하지 않음 |
| `POST /v1/auth/email/start` | JSON `{email}` 및 정확한 웹 `Origin` | 202 `{sent:true, expires_in:600, retry_after:60}`, HttpOnly challenge cookie | 잘못된 주소 400, 다른/누락 Origin 403, 제한 429 + Retry-After, 미설정/Resend 오류 503 |
| `POST /v1/auth/email/verify` | JSON `{code}`, challenge cookie, 정확한 웹 `Origin` | 200 기존 session DTO와 HttpOnly session cookie, challenge 삭제 | 잘못된/만료/재사용 코드 400/401, 다른/누락 Origin 403, 시도 초과 429 |

모든 인증 응답은 `Cache-Control: no-store`를 유지한다. API는 기존 JSON 오류 정제·IP 제한을 통과한다. 요청 body 상한은 4 KiB다. 주소 존재 여부에 따른 성공 응답 차이를 만들지 않는다. 발송 성공은 Resend 접수 성공이며 실제 받은편지함 도착과 구분한다.

## 보안과 저장

- 코드는 OS CSPRNG로 편향 없이 생성한다. 인증 challenge는 별도 256비트 난수이며 browser cookie에만 원문을 전달한다. production cookie는 Secure, HttpOnly, SameSite=Lax이고 Domain 속성을 설정하지 않는다.
- 이메일은 앞뒤 공백 제거 후 ASCII 소문자로 정규화한다. ASCII dot-atom 주소만 받으며 길이·local/domain label·제어문자·다중 수신자·헤더 삽입을 거부한다. Gmail의 점·plus alias를 합치지 않는다.
- 이메일 식별자는 독립적인 32바이트 이상 `WAM_EMAIL_IDENTITY_SECRET`으로 HMAC-SHA256 한다. 이메일 원문은 Resend 요청 중에만 사용하고 저장소·프로필·진단 로그에 쓰지 않는다. 이 secret은 계정 식별에 필요하므로 백업하고 임의로 교체하지 않는다.
- challenge와 코드는 해시로 저장하며 코드 검증·소비·계정 binding·세션 생성은 한 저장 transaction으로 처리한다. 같은 코드를 병렬 검증해도 하나만 성공한다.
- 주소별 재전송 60초, 시간당 최대 5회 발송, challenge당 실패 최대 5회. 발송 예약을 먼저 저장해 동시 요청 제한을 보장하고 Resend 실패 시 해당 challenge를 취소한다. 실패해도 발송 제한은 소모하여 공급자 오류 시 무한 발송을 막는다.
- 기존 GitHub account binding과 provider namespace를 분리한다. 스냅샷 구버전 역직렬화와 계정 삭제·만료 정리를 지원한다. local file과 PostgreSQL의 기존 identity repository 경로를 재사용한다.
- Resend HTTP client는 timeout과 redirect 거부를 적용한다. 비밀 API key와 코드가 응답·오류·telemetry·로그에 반사되지 않아야 한다. `Idempotency-Key`는 challenge별로 고정한다.

## 설정과 배포

이메일 로그인을 활성화하려면 서버 전용 `RESEND_API_KEY`, `WAM_EMAIL_FROM`, `WAM_EMAIL_IDENTITY_SECRET`을 모두 설정해야 한다. 부분 설정·공백 값은 시작 실패다. 진단 메일은 `WAM_DIAGNOSTICS_EMAIL_TO` 수신 주소와 기존 Resend 키·발신 주소를 사용하며 이메일 로그인을 활성화하지 않는다. Production 서버는 기존 규칙대로 GitHub OAuth 또는 이메일 로그인 중 하나를 인증 수단으로 설정해야 한다. 이메일만 설정한 production도 실행 가능하며 GitHub OAuth 자격증명을 강제하지 않는다. MCP OAuth도 이메일 로그인과 함께 사용 가능해야 한다.

키는 Git 제외된 `credentials/resend.txt`에 저장하고 파일 권한 600으로 유지한다. 로컬 실행용 추가 secret도 같은 폴더에만 생성한다. 웹 번들, Docker image build arguments, git에는 넣지 않는다.

운영 `WAM_PUBLIC_ORIGIN`은 host와 `/` 이외의 path, userinfo, query, fragment가 없는 순수 HTTPS origin이어야 한다. GitHub OAuth가 설정된 경우 `WAM_GITHUB_REDIRECT_URI`는 정확한 `/v1/auth/github/callback` path를 사용하고 userinfo, query, fragment를 포함하지 않아야 한다. 잘못된 주소는 시작 시 거부하여 사용자 비밀·callback parameter가 redirect나 생성 링크에 섞이지 않게 한다. loopback HTTP callback은 비 production 개발에서만 허용한다. 회귀 케이스는 [email authentication config tests](../test/email-auth.md)와 issue #99를 참조한다.

Cloudflare 도메인 구매와 Resend 발신 도메인 검증은 별개다. `webagenticmemory.com`의 Resend 도메인을 등록하고 제공된 DKIM/SPF/MX 레코드를 Cloudflare DNS에 정확히 등록한 뒤 verify 상태를 확인한다. 기존 메일용 MX/TXT를 덮어쓰지 않는다. HTTPS 웹과 Rust API는 동일 origin으로 배포하고 인증·MCP 응답을 CDN cache에서 제외한다. 기존 [배포 절차](../operations/deployment.md)를 따른다. Resend 발송 전용 key는 도메인 조회/등록·DNS 관리 권한이 없다.

## 완료 기준

선행 테스트의 실패와 수정 후 통과를 기록한다. backend·web typecheck/build·기존 GitHub/MCP 회귀와 브라우저 로그인 검사를 수행한다. 실제 HTTPS 배포에서는 auth config, 코드 발송 접수, 실제 수신·검증·세션·로그아웃을 확인한다. 외부 권한이나 호스팅 입력이 없으면 그 단계와 필요한 입력을 명시하며 로컬 성공을 배포 성공으로 표시하지 않는다.
