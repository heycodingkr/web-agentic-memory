# 브라우저 오류 추적성 개선 4090 배포

2026-11-03 18:40 KST(09:40Z), 브라우저의 미처리 오류가 원래 API 요청과 연결되도록 `20261103T094011Z-main-2e62600`을 4090에 배포했다.

## 배포 결과

| 항목 | 결과 |
| --- | --- |
| 운영 주소 | https://webagenticmemory.com/ · https://api.webagenticmemory.com/healthz |
| 소스 커밋 | `2e62600e08c30e4cee0433f116f6cde32785cfd6` |
| 릴리스 | `20261103T094011Z-main-2e62600` |
| 이전 릴리스 | `20261103T080921Z-main-17dff80` |
| API 바이너리 | sha256 `d4e401ea6db1049e3e0d6983ad95a82bbd357f1843012a506feb7eba6c5b05bb` |
| 운영 데이터 | 카탈로그 38개·공식 점수 880점 유지. 기존 메모리 버전 ID 38개 전부 유지 |
| 운영 설정 | 설정 파일 8개의 SHA-256이 배포 전후 동일. 설정·DB 스키마 변경 없음 |

브라우저의 `unhandledrejection` 처리기가 거부 원인을 넘기고, API 오류는 원래 요청의 trace ID·작업 종류·상태 코드를 다시 사용한다. UI 오류는 정규화한 페이지 종류만 기록한다. 메시지, 스택, URL과 쿼리는 수집하지 않는다. 과거 이벤트에는 저장되지 않은 원인과 스택을 복원할 수 없다.

## 검증

| 검사 | 결과 |
| --- | --- |
| 운영 소스 보존 | `compare-with-production.py 2e62600`: 운영 전용 미커밋 파일 0개 |
| production web build | 통과. `SetupGuide.svelte`의 기존 경고 2개, 빌드 오류 0개 |
| 고정 Linux builder | 59개 테스트 요약, 437 통과, 0 실패, 31 ignored. 일회용 PostgreSQL 검사와 release 빌드 포함 |
| 업로드 무결성 | 소스·웹 archive, manifest, 빌드·활성화 스크립트 SHA-256 모두 일치 |
| 활성화 점검 | API readiness, 관리자 경계 401/403, 공식 작성자·점수 일관성, 정적 페이지 점검 통과 |
| 공개 HTTPS | API `/healthz` 200, 사이트 `/` 200 |
| 추적성 코드 제공 | `app.D1TCiA-7.js`와 `Dvq1zVse.js`가 200을 반환했고 둘 다 빌드 산출물과 SHA-256 일치 |
| 데이터·설정 보존 | 배포 전후 버전 ID 38개 일치, 공식 점수 880점 유지, 설정 해시 8개 일치 |

운영 오류 알림 메일을 만들지 않도록 테스트 오류 이벤트를 telemetry endpoint로 보내지 않았다. 클라이언트 코드가 실제 공개 번들에 포함됐는지는 HTTPS로 내려받은 파일과 빌드 산출물의 해시를 비교했다.

## 증거와 복구

소스·웹 archive, SHA-256 목록, 빌드 스크립트, 배포 전 기준선(`pre-deploy-baseline.json`)과 배포 후 검사(`post-deploy-check.json`)는 서버 `builds/20261103T094011Z-main-2e62600/` 및 로컬 gitignored `build/deploy-4090/telemetry-2e62600/`에 보관한다. 활성화 스크립트가 DB 백업을 만들었다. 배포 점검은 성공했고 자동 롤백은 일어나지 않았다.

복구가 필요하면 [운영 릴리스 절차](../../deploy/4090/release/README.md)에 따라 `current`를 이전 릴리스로 돌리고 API를 재시작한다. 이 릴리스는 DB 스키마와 운영 설정을 바꾸지 않았다.
