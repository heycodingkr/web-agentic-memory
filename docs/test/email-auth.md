# EMAIL-AUTH 검증 계획과 실행 기록

2026-11-01, 구현 전에 작성. 기능 계약은 [명세](../spec/email-auth.md). 아래는 선행 자동 검사와 이전 실행 기록이며, 후속 4090 직접 배포·실메일 최초/기존 로그인·세션 유지·로그아웃 결과는 [직접 배포 검증](deployment-4090.md)을 따른다. 현재 공개 서비스는 Tunnel 없이 4090에서 실행한다.

## 선행 테스트와 자동화

먼저 기존 router에 이메일 설정 필드와 start 경로의 미설정 응답을 검증하는 테스트를 추가하고, 구현 전 실패를 기록한다. 이후 동일 파일에 fake mail sender를 사용한 실제 router 통합 검사를 확장한다. 테스트는 실사용자에게 메일을 보내지 않으며 코드를 API 응답에서 얻지 않고 fake sender의 전달 기록에서 읽는다.

| ID | 검증 | 기대 결과 |
|---|---|---|
| EMAIL-01 | 미설정 auth config/start | email_auth_enabled=false; start=503; 기존 GitHub 필드 유지 |
| EMAIL-02 | 올바른 주소 start | 202·no-store·HttpOnly challenge cookie; 6자리 코드를 fake sender에 정확히 한 번 전달; 세션은 아직 없음 |
| EMAIL-03 | 올바른 cookie+code verify | 200·12시간 세션·CSRF; 내부 계정 생성; 기존 authenticated API 접근 |
| EMAIL-04 | 오답·누락/다른 browser cookie | 세션 생성 없음; 5회 오답 뒤 올바른 코드도 거부 |
| EMAIL-05 | 만료 경계·사용한 코드 재사용·병렬 verify | 600초에 만료; 재사용 실패; 병렬 검증 성공은 하나 |
| EMAIL-06 | 재전송 | 60초 전 429; 이후 이전 코드 무효; 시간당 5개 제한; 원본 코드/브라우저를 바꿔 제한 우회 불가 |
| EMAIL-07 | 같은 정규화 주소 재로그인·서버 재시작 | 동일 내부 계정·기존 세션 유지; GitHub identity와 자동 병합하지 않음 |
| EMAIL-08 | 이메일 검증과 개인정보 | CRLF·주소 내부 공백·다중 @·배열/다중 수신자·너무 긴 주소 거부; 저장 JSON/공개 DTO에 원문 주소·코드·challenge·session 원문 없음 |
| EMAIL-09 | Origin·JSON 상한·잘못된 body | 다른/누락 Origin 403; 4KiB 초과·잘못된 JSON 거부; 정제된 오류·no-store |
| EMAIL-10 | 공급자 오류 | Resend timeout/4xx/5xx/redirect=503; challenge 취소; key·응답 body 노출 없음 |
| EMAIL-11 | Resend wire contract | Bearer, POST /emails, from/to/subject/text, challenge별 idempotency key; production endpoint는 HTTPS |
| EMAIL-12 | secret 설정·이메일 전용 production | 부분 설정 실패; GitHub 없이 email auth·MCP OAuth 활성화 가능 |
| EMAIL-13 | 삭제·만료·구버전 snapshot | 이메일 binding·pending challenge 삭제와 purge, 기존 identity JSON 읽기 |
| EMAIL-14 | 브라우저 | 이메일 입력→코드 입력→오답→정답→로그아웃, 재전송 countdown/메일 변경, 접근성 이름·모바일 레이아웃; 기존 GitHub 버튼 유지 |

backend에서는 결정적인 시계 인자로 만료·제한을 검사한다. HTTP 통합 검사와 별도로 실제 Resend client를 loopback HTTP fixture에 연결해 payload와 오류 처리를 확인한다. production URL을 임의로 주입하는 설정은 제공하지 않는다.

## 실행 명령

브라우저 검사는 별도 terminal/host detached shell에서 먼저 `pnpm --dir web exec vite dev --host 127.0.0.1 --port 5197 --strictPort`를 실행한 frontend를 재사용한다. 다른 주소는 `WAM_EMAIL_WEB_ORIGIN`으로 지정한다. 브라우저 API는 fake이며 실제 backend 검증과 구분한다.

```sh
cargo test --locked -p wam-server --test email_auth
# Run the PostgreSQL case only with an owned disposable database.
# WAM_TEST_DATABASE_URL=... cargo test --locked -p wam-server --test email_auth_postgres -- --ignored
cargo test --locked -p wam-server identity
cargo test --locked -p wam-server --test oauth_flow --test mcp_oauth
pnpm --dir web test
pnpm --dir web check
pnpm --dir web build
pnpm --dir web exec playwright test --config playwright.email-auth.config.ts
```

## 운영 검증

배포 후 **가입·기존 로그인·비밀번호 없는 재로그인·메일 미수신/접근 상실·세션·MCP/기기 연결**의 구체적인 절차와 합격 기준은 [email-auth-production.md](email-auth-production.md)의 EMAIL-PROD-01~34를 따른다. 아래는 요약이며 과거 로컬 검증을 새 운영 케이스의 통과로 표시하지 않는다.

1. Cloudflare DNS와 Resend verified 발신 도메인, 실제 배포 호스트, HTTPS same-origin을 확인한다. 도메인·key 원문·받은편지함 개인정보는 공개 증거에 넣지 않는다.
2. `https://webagenticmemory.com/v1/auth/config`가 JSON으로 email_auth_enabled=true를 반환하고 SPA fallback과 혼동되지 않는지 확인한다.
3. 운영 login 페이지에서 승인된 테스트 수신 주소로 1회 발송하고 Resend 접수와 실제 수신을 각각 기록한다. 브라우저에서 코드 검증·dashboard·새로고침·로그아웃·401을 확인한다.
4. HTTPS cookie flags, no-store, Origin 거부, 오답·재사용 실패와 정제된 오류를 확인한다. Resend message id와 코드 원문을 공개 보고하지 않는다.
5. 배포 artifact/commit·호스트·시각·결과와 실행하지 못한 항목을 아래에 기록한다.

## 실행 기록

- 사전 확인: Resend key를 `credentials/resend.txt`에 저장, 권한 600, git check-ignore 통과. `/domains` 조회는 restricted_api_key 응답으로 발송 전용 권한임을 확인했다. 도메인은 사용자 확인값 `webagenticmemory.com`이다.
- 선행 실패: `cargo test --locked -p wam-server --test email_auth` 실행. 0 passed / 1 failed. `email_auth_disabled_contract`에서 auth config의 email_auth_enabled가 Null(필드 없음)인 것을 false와 비교하다 실패했다. 기록: `/tmp/wam-email-auth-red.log`.
- Backend: `cargo test --locked -p wam-server --lib --test email_auth --test oauth_flow --test mcp_oauth` 통과. library 55개, 이메일 HTTP/config/adapter 7개, MCP challenge 1개, 기존 GitHub/MCP OAuth 16개: 합계 79 passed / 0 failed. timeout·redirect 전달 차단·잘못된 Resend receipt, email-only production·부분/잘못된 설정도 검사했다. 기록: `/tmp/wam-email-final-tests.log`.
- PostgreSQL: 소유한 일회용 native PostgreSQL에서 `email_auth_postgres` ignored case를 명시적으로 실행했다. 서로 다른 repository/서버 인스턴스의 주소별 rate limit 공유, 동시 verify 한 번만 성공, 세션/CSRF·계정 재연결을 확인했다. 1 passed / 0 failed. 생성한 schema·DB process·임시 디렉터리 정리 확인. 기록: `/tmp/wam-email-postgres.log`.
- 정적 검사: `cargo clippy --locked -p wam-server --lib --tests -- -D warnings` 통과. `git diff --check` 통과. 기록: `/tmp/wam-email-clippy.log`.
- Web: 최종 `pnpm --dir web test` 41 passed / 0 failed. `pnpm --dir web check` 0 errors / 0 warnings, production build 성공. 로그인·기여 안내 카피와 My Page 결과 렌더링 검사가 포함된다. 최신 실행은 2026-11-01.
- Browser: Chrome desktop/mobile 각 10개, 총 20 passed / 0 failed. 오답→정답(선행 0 포함)→CSRF 로그아웃, countdown/재전송·주소 변경·delivery error·첫 가입/메일함 도움말·email-only MCP 승인 복귀·기기 코드 보존·가로 overflow·axe WCAG A/AA를 검사했다. API는 fake다. 생성된 desktop/mobile 스크린샷은 이미지 도구로 직접 확인했다. 최신 실행: 2026-11-01, `web/test-results/email-auth/`.
- 실 Resend 접수: 공식 `delivered+...@resend.dev` 테스트 수신 주소로 직접 API 요청 HTTP 200 receipt를 확인했다. 이어 로컬 production web → Caddy → 실제 Rust API → Resend 경로에서 start=202, HttpOnly challenge cookie, no-store, 코드 미반환을 확인하고 잘못된 verify=400을 확인했다. 이 주소는 [Resend의 전달 시뮬레이션](https://resend.com/docs/dashboard/emails/send-test-emails)이며 실제 사용자 받은편지함 수신 검증이 아니다.
- 비밀값 검사: production HTML/JS/CSS/JSON 71개에서 실제 Resend key/identity secret 미검출. 두 credential 파일 모두 600·Git ignore·미추적 확인. live identity snapshot에도 원문 테스트 수신 주소·provider key·identity secret이 없다.
- 배포 설정: `deploy/Caddyfile`은 실제 Caddy validate/adapt 통과. systemd unit은 Linux 호스트 설치 전이며 실행 검증은 하지 않았다. [배포 안내](../../deploy/README.md) 참조.
- 공개 배포 대기: Cloudflare 공개 DNS NS는 emma/louis, A 응답은 없음. Arise Cloudflare는 로그인 화면이며 DNS 편집 권한·배포 호스트·운영 DB/Redis 입력이 없다. 발송 전용 key로 Resend 도메인 상태를 조회할 수 없으므로 domain verified나 실제 사용자 수신을 완료로 판정하지 않았다. 공유 이슈 #157에 남겼다. 공개 HTTPS·실제 사용자 수신/검증/로그아웃·production artifact/commit 확정은 미실행이다.

## 로컬 미리보기 실행 상태

`http://127.0.0.1:5198/login`은 완성된 production 웹 빌드와 실제 Resend sender를 연결한 로컬 미리보기다. API/file identity store는 development mode이며 공개 배포가 아니다. API와 proxy 모두 loopback에만 bind하고 host detached shell로 실행하여 이 턴 이후에도 유지한다. API 저장소는 Git 제외된 `build/email-auth-preview/`를 사용한다.

| 프로세스 | PID | 로그 |
|---|---|---|
| 실제 Rust API, 3497 | 36464 | `build/email-auth-api.log` → `/var/folders/hn/4d_172w53_58g8kg255mlw_w0000gn/T/meshcode-detached-98074-1793511489419.log` |
| Caddy production web/proxy, 5198 | 37799 | `build/email-auth-web.log` → `/var/folders/hn/4d_172w53_58g8kg255mlw_w0000gn/T/meshcode-detached-98074-1793511647506.log` |
| Vite browser QA frontend, 5197 | 34519 | `/var/folders/hn/4d_172w53_58g8kg255mlw_w0000gn/T/meshcode-detached-98074-1793511197707.log` |

Caddy 미리보기 설정은 `build/email-auth-preview.Caddyfile`, 별도 runtime state 경로는 `build/email-auth-caddy/`다. 기존 사용자의 API나 다른 세션의 프로세스는 재시작하지 않았다.

