# LINK: 계정 로그인과 MCP 기기 연결

상태: 1차 상세 명세. 인증 서버, device flow, token storage는 미구현이다.

## 범위

계정 로그인은 웹 세션과 사용자 소유권을 만든다. Claude/Codex에서 사용할 원격 MCP는 지원되는 OAuth authorization flow로 연결하고, OAuth가 없는 로컬 stdio 호스트는 device authorization으로 계정과 기기를 연결한다. 공개 메모리 읽기는 익명으로 가능하지만 contribution, outcome, dashboard, 점수 기능은 인증된 계정에 한정한다.

## 인증 모드

- **웹:** GitHub OAuth를 초기 provider로 사용한다. 내부 user identity는 `(provider, provider_subject)` 조합에 연결하고 email은 공개하지 않는다.
- **Remote MCP:** 가능한 host는 OAuth 기반 인증을 쓴다. memory-read와 contribution/outcome 쓰기 권한은 scope로 구분한다.
- **Local stdio:** MCP 설정 스니펫에는 endpoint와 server name만 넣는다. `wam link` 또는 local helper가 device start/poll을 호출하고, one-time code를 웹의 `/link`에서 승인한다. 장기 토큰은 OS secure storage에 넣는다.
- **Arise Browser:** 내장 어댑터는 `/link` device flow로 현재 브라우저 프로필을 명시적으로 연결한다. 연결 전에는 익명 public read만 된다. Arise에서 웹 페이지에 로그인했거나 브라우저에 사이트 쿠키가 있다는 이유만으로 memory API identity를 연결하지 않는다. 기기 이름과 `read:memories`, `report:outcome`, `write:contributions` 권한을 연결 화면에서 확인한다.
- **Anonymous:** 읽기 전용 recall/detail만 가능하다. user-specific receipt, write endpoint, contribution author attribution은 만들지 않는다.

## Device authorization 흐름

1. CLI가 `POST /v1/link/start`를 호출한다. 서버는 짧은 `user_code`, 내부 `device_code`, 승인 URL, 만료 시각, polling interval을 반환한다.
2. 사용자는 웹에 로그인한 뒤 `/link`에서 code와 요청된 권한을 확인한다.
3. 승인 시 device row를 session user에 연결한다. 거부·만료·오입력 시 권한은 발행되지 않는다.
4. CLI poll은 승인 전 `pending`, 거부/만료 시 terminal error, 승인 후에는 한 번만 token을 반환한다.
5. client는 token을 OS secure storage에 저장하고, status로 account, scope, expiry/revocation만 확인한다.
6. `/settings/devices`에서 사용자는 host name, last seen, scope를 보고 특정 device를 해지한다.

Device secret은 DB에는 hash로만 보관한다. device code는 짧은 수명, 재사용 금지, 추측 제한을 갖는다. revocation은 다음 인증 요청부터 즉시 반영한다. token의 사용자 계정은 body나 tool argument의 ID가 아니라 검증된 credential에서 정한다.

## 권한과 세션 규칙

`read:memories`는 공개 anonymous read와 authenticated read에 쓸 수 있다. `write:contributions`는 로그인과 명시적 consent가 모두 있어야 한다. `report:outcome`은 같은 account에 발급된 유효 receipt가 있어야 한다. `read:dashboard`와 device management는 웹 session의 소유 계정만 허용한다. 관리자 review scope는 저신뢰·충돌·abuse 예외에만 발급한다.

웹 session cookie는 Secure, HttpOnly, SameSite 정책을 따른다. OAuth state/PKCE/redirect URI를 검증하고 CSRF를 방어한다. 로그아웃은 web session을 종료하며 별도로 연결된 MCP device를 자동 삭제하지 않는다. 기기 해지는 user가 선택적으로 수행하고, 해지한 credential은 propose/outcome에서 즉시 실패해야 한다.

## 요구사항과 테스트

| ID | 요구사항 | 합격 기준 | 관련 테스트 |
|---|---|---|---|
| F-LINK-01 | OAuth account binding | 한 provider subject가 안정된 내부 user에 연결되고 email은 public response에 없음 | WEB-02, WEB-16, API-32, SEC-D |
| F-LINK-02 | 안전한 device start | user/device secret 없이 짧은 code와 만료 정보만 반환 | LINK-01, SEC-D |
| F-LINK-03 | login 후 사용자 승인 | 승인된 session user에 device row가 귀속; 거부 시 token 없음 | LINK-02~05, WEB-03~04 |
| F-LINK-04 | one-time polling | code/token 재사용, 만료, brute force가 차단됨 | LINK-04~06, SEC-D |
| F-LINK-05 | secure token storage | 평문 token은 process args/log/config/DB에 없고 OS secure store만 사용 | LINK-08, SMK-08, SMK-14 |
| F-LINK-06 | scope enforcement | anonymous/propose/outcome/dashboard 권한이 분리 적용됨 | LINK-09~10, API-30~38, MCP-22~23 |
| F-LINK-07 | device revocation | account owner가 해지하면 다음 write 요청은 거부 | LINK-07, API-38 |
| F-LINK-08 | user isolation | 본인 아닌 session으로 contribution/dashboard/device 조회 불가 | WEB-18~19, API-33~34 |
| F-LINK-09 | Arise/browser account linking | browser profile은 명시적 device 승인 전 익명이며, 승인 뒤에만 user-bound receipt/write scope를 얻는다 | LINK-01~11, CUST-05~06 |

관련 end-to-end 경로는 [`../test/feature-acceptance.md`](../test/feature-acceptance.md)의 `ACC-WEB-01`, `ACC-MCP-01`, `ACC-MCP-03`, `ACC-MCP-05`와 [`../test/customer-paths.md`](../test/customer-paths.md)의 `CUST-01~06`이다.
