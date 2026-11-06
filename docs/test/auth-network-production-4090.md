# 로그인 네트워크 오류 복구 4090 배포

2026-11-06 16:29 KST(07:29Z), 로그인 연결 실패 안내·명시적 재시도·코드 검증 오류 분류를 수정한 `20261106T072521Z-main-10357de`를 운영에 활성화했다. 관련 이슈는 #366이며 최신 릴리스 안내의 오래된 포인터(#368)도 이 기록으로 갱신했다.

## 배포 결과

| 항목 | 결과 |
| --- | --- |
| 운영 로그인 | https://webagenticmemory.com/login |
| 소스 커밋 | `10357ded89acac07bd81e43802453cdcab10ec86` |
| 릴리스 | `20261106T072521Z-main-10357de` |
| 이전 릴리스 | `20261104T071435Z-main-fb6aac4` |
| 활성화 시각 | `2026-11-06T07:29:09Z` |
| API 바이너리 SHA-256 | `27110524f1ffc97e8b2cb36c5afd7eacb4f71fd8b4e956fd637ca634149df965` — 이전 운영 바이너리와 동일 |
| 데이터 보존 | 카탈로그 38개 버전 ID와 공식 점수 880점 동일 |
| 설정 보존 | 운영 비밀 설정·systemd·Caddy의 파일 11개 SHA-256 동일 |
| DB 백업 | `/var/backups/web-agentic-memory/20261106T072521Z-main-10357de/production-before.dump` |

초기 인증 설정·세션 조회가 연결 실패하면 오류와 `Retry sign-in check`를 표시한다. 코드 검증의 연결 실패·5xx는 실제 코드 오답/만료(400/401)와 구분하며, 입력 코드를 유지하고 사용자가 다시 시도한다. 정상적인 비로그인 세션 401은 오류로 표시하지 않는다. 이메일 start/verify는 브라우저 진단의 닫힌 `auth` 작업으로 분류한다. 인증 응답·쿠키·메일 발송 정책은 변경하지 않았다.

## 검증

| 검사 | 결과 |
| --- | --- |
| 운영 소스 비교 | 배포 커밋에 없는 운영 전용 파일 0개. 변경된 기존 파일은 인증 수정 6개뿐 |
| 깨끗한 checkout | `10357de`의 detached worktree에서 패키징. 다른 작업의 README·데모 변경 제외 |
| 웹 단위·타입·빌드 | 81/81 통과, Svelte 오류 0개·기존 SetupGuide 경고 2개, production build 성공 |
| 배포 웹 브라우저 | 실제 production 빌드에 fake API를 붙여 desktop/mobile Chrome 14/14 통과 |
| 브라우저 입력 | config/session connectionreset 후 재조회 복구, verify connectionreset·503·401 분류, 정상 session 401, 오답→정답·CSRF 로그아웃·접근성 검사 |
| Linux 고정 builder | 테스트 요약 59개, 441 통과·0 실패. 초기 실행 ignored 31개 중 별도 PostgreSQL application 18개와 email/admin 2개 실행 포함 |
| 무결성 | 소스·웹 archive, manifest, release 정보, 빌드·활성화 스크립트 등 업로드 SHA-256 전부 일치 |
| 활성화 점검 | DB 백업·배포 잠금·이전 릴리스 고정 검사, API readiness, 관리자 경계와 정적 페이지 검사 통과. 자동 롤백 없음 |
| 공개 HTTPS | login 200/no-store, apex/API healthz 200, auth config 200/email enabled, 비인증 session 401/no-store |
| 관리자 경계 | admin session·목록 401, 공개 origin의 review 목록 403 |
| 실제 JS 제공 | 로그인·telemetry·app·start 4개 파일 모두 HTTPS 200, 빌드 산출물과 SHA-256 일치 |

공개 번들에서 `_app/immutable/nodes/12.Dg9IfmDF.js`의 재시도/오류 안내와 `_app/immutable/chunks/DHVU-yvO.js`의 이메일 인증 경로 분류가 제공됨을 확인했다. 로그인 HTML도 새 `app.PV5gBkCc.js`를 참조한다.

이번 배포에서 실제 사용자에게 새 인증 메일을 발송하거나 운영 telemetry에 합성 오류를 전송하지 않았다. 과거 trace의 브라우저 fetch 거부 사유는 닫힌 로그만으로 복원할 수 없으며, 이 변경이 과거 연결 장애의 원인을 제거했다고 주장하지 않는다. 선행 실패와 전체 로컬 회귀 결과는 [이메일 인증 검사 기록](email-auth.md#네트워크-실패-회귀-입력과-기대-결과-366)을 따른다.

## 증거와 복구

서버의 `/srv/web-agentic-memory/builds/20261106T072521Z-main-10357de/`에 소스·웹 archive, manifest, SHA256SUMS, Linux `build.log`, 사전 기준선과 snapshot 도구가 있다. 로컬 Git 제외 경로 `build/deploy-4090/auth-network-10357de/`에는 동일 산출물과 `linux-build.log`, `production-browser.log`, `activation.log`, 배포 전후 snapshot 및 `post-deploy-check.json`을 보존한다.

로컬 production preview는 `http://127.0.0.1:5199/login`, listener PID `5719`(pnpm `5709`)로 detached 실행 중이다. 로그는 `/var/folders/hn/4d_172w53_58g8kg255mlw_w0000gn/T/meshcode-detached-35610-1793949991471.log`다.

복구는 [운영 릴리스 절차](../../deploy/4090/release/README.md)를 따른다. 이전 release·정적 자산·DB 백업을 보존했고, 이번 릴리스에는 DB 스키마·운영 설정 변경이 없다.
