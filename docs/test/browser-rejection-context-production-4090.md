# 브라우저 오류 진단 문맥 4090 배포

2026-11-20 11:42:39 KST(02:42:39Z), 이슈 #657의 진단 보완을 운영에 배포했다. 사용자 승인을 받아 소스 커밋을 `origin/main`에 먼저 푸시하고, 고정된 커밋의 API와 정적 웹을 함께 전환했다.

## 배포 결과

| 항목 | 결과 |
| --- | --- |
| 운영 주소 | https://webagenticmemory.com/ · https://api.webagenticmemory.com/healthz |
| 소스 커밋 | `adea96b68c1e56c1c1e8098375ff707dca75c63a` |
| 릴리스 | `20261120T023439Z-main-adea96b` |
| 이전 릴리스 | `20261111T030635Z-main-53d1723` |
| 활성 API SHA-256 | `d0b384fabc04082000ec046cb8273bce5de925893d16f426d2b6ff9913504471` |
| 실행 프로세스 | PID `3194775`, 실행 파일 경로와 해시가 새 릴리스와 일치 |
| 데이터 보존 | 메모리·현재 버전 ID 38쌍 및 공식 점수 880점 유지 |
| 설정 보존 | 파일 12개의 SHA-256·권한·UID·GID 전부 동일 |

새 알림은 메시지·스택·URL·실제 경로를 수집하지 않고 `web_page`, `web_error_type`만 추가한다. API trace·상태·경과 시간, 기존 클라이언트 호환성, 알림 대상 판정은 유지한다. [명세](../spec/diagnostics.md)와 [입력·기대 결과 및 로컬 검사](browser-rejection-context.md)를 따른다.

## 검증

| 검사 | 결과 |
| --- | --- |
| 커밋 범위 | 진단 구현·테스트·문서 12개 파일만 커밋. 기존 README·영상 작업 보존 |
| 운영 소스 보존 | manifest 639개 중 627개 동일, 12개는 Git 이력에 있는 이전 버전. 운영 전용 미커밋 파일 0개 |
| 패키지 | 깨끗한 고정 checkout, production origin으로 웹 빌드. 소스 manifest 640개와 두 archive의 파일 내용을 대조 |
| 업로드 | archive·manifest·릴리스 정보·빌드/활성화 스크립트·expected-previous 등 9개 SHA-256 일치 |
| Linux | 고정 builder, 릴리스 전용 target. 테스트 결과 요약 60개: 456 통과·0 실패·기본 실행 ignored 31개. 스크립트가 지정한 postgres_application·email_auth_postgres·postgres_diagnostics는 일회용 PostgreSQL에서 별도 실행해 통과 |
| 로컬 웹/Rust | 웹 84 통과, 서버 278 통과·외부 환경용 25 ignored. check 오류 0·기존 SetupGuide 경고 2. build·Clippy warnings-as-errors·format 통과 |
| 활성화 | 공유 잠금·이전 릴리스 일치·DB 백업·readiness·관리자 401/검수 403·공식 작성자·정적 페이지 검사 통과. rollback 발생 없음 |
| 실제 배포 파일 | Linux 소스 473개·새 웹 138개 해시 일치. 실행 바이너리 SHA와 metadata 일치 |
| 공개 HTTPS | health·홈·가이드·메모리 200, 관리자 session 401 |
| 제공 번들 | 현재 HTML의 새 app entry와 문맥 chunk를 HTTPS로 내려받아 빌드 SHA와 비교, 둘 다 일치 |
| 데이터·설정 | 배포 전후 ID·점수 및 설정 내용/권한/소유자 비교 통과 |

macOS tar의 `._*` 메타데이터를 제외하고 웹 archive를 다시 만들었다. Linux가 무시한 provenance 확장 헤더는 파일 내용에 영향을 주지 않았으며, 실제 웹 파일 138개 모두 빌드와 일치했다.

## 실제 intake와 PostgreSQL 확인

고정 checkout의 실제 JS reporter로 합성 `http_error`/404 이벤트를 만들었다. 거부 값은 TypeError, 페이지는 가이드이며, `web_context={page:guide,error_type:type_error}`만 포함한다. 비공개 sentinel 메시지·쿼리는 전송 JSON에 없음을 먼저 검사했다.

- 배포 전: 새 선택 필드를 이전 intake가 400으로 거부했다.
- 배포 후: 같은 이벤트가 202로 수락되고 재전송도 202였다.
- PostgreSQL 읽기 전용 확인: trace `7edf5d796c1e81dcd5b6e231576a485d`의 web client_error는 정확히 한 건이며 문맥·event ID·trace·상태·경과 시간이 전송 값과 일치했다.
- 추가 중첩 필드, 미등록 오류 종류, 다른 source는 모두 400으로 거부됐고 sentinel을 반사하지 않았다.
- 공개 응답·번들 검사를 포함해 12개 검사에 통과했다.

이 probe는 기존 규칙상 메일 대상에서 제외되는 HTTP 404다. 테스트용 미처리 오류 알림이나 로그인 메일을 요청하지 않았고, 실제 메일함 수신을 검증했다고 주장하지 않는다. 알림 formatter의 새 필드는 자동 테스트로 검증했다.

현재 제공 파일:

| 파일 | SHA-256 |
| --- | --- |
| `_app/immutable/chunks/DS7_xrFl.js` | `561ae1601bcc762ea260a47f4e7d25d2f65fcea48272135bbdb8da4debbc9756` |
| `_app/immutable/entry/app.DVGpFFZ4.js` | `67c1d9c75226a45c063e0a0acd37ee435e3255a68dbe1aaeeca2b86ad7ccc799` |

## 증거·복구·남은 범위

서버 `builds/20261120T023439Z-main-adea96b/`에 패키지·manifest·고정 소스·Linux 로그·바이너리를 보존한다. 로컬 Git 제외 `build/deploy-4090/browser-context-20261120/`에는 패키지 검증, 배포 전후 기준선, 공개 probe, DB·실행 프로세스·데이터/설정 비교와 원본 로그가 있다.

DB 백업은 `/var/backups/web-agentic-memory/20261120T023439Z-main-adea96b/production-before.dump`이며 비어 있지 않고 권한은 0600이다. 이전 릴리스·백업을 보존했다. 복구는 [운영 릴리스 절차](../../deploy/4090/release/README.md)를 따른다. 스키마·운영 설정·기본 메모리를 갱신하지 않았다.

원래 trace `08a35e974f5820abfab8818a758717b9`의 거부된 promise는 아직 특정하지 못했다. 새 분류는 향후 원인을 좁히는 단서이며, 과거 런타임 오류 자체가 수정됐다는 증거가 아니다. #657은 검토 대기로 유지한다.

푸시 과정에서 GitHub가 별도로 보고한 의존성 경고 6개(2 high·3 moderate·1 low)는 #659에 등록했다. 세부 영향은 미확인이고, 이번 배포에는 의존성·lockfile 변경을 섞지 않았다.
