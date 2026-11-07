# MCP recall 검증 사유 추적 4090 배포

2026-11-07 15:27 KST(06:27Z), issue #371의 [추적 개선](recall-invalid-input-tracing.md)을 운영에 활성화했다.

## 배포 결과

| 항목 | 결과 |
| --- | --- |
| 소스 커밋 | `9334e668e0f72cd76402fa8c5777ebe4080f0745` |
| 릴리스 | `20261107T062322Z-main-9334e66` |
| 이전 릴리스 | `20261106T072521Z-main-10357de` |
| 활성화 | `2026-11-07T06:27:37Z` |
| API 바이너리 SHA-256 | `4df8489a154f9c329c84f904951c3293f40499416a70e97089c77adc4f7135d6` |
| DB 백업 | `/var/backups/web-agentic-memory/20261107T062322Z-main-9334e66/production-before.dump` |
| 데이터 보존 | 카탈로그 38개 메모리·버전 ID와 공식 점수 880점 동일 |
| 설정 보존 | 비밀 설정·systemd·Caddy 파일 11개 SHA-256 동일 |

원격 MCP recall의 검증 사유를 `result._meta.error_reason`·진단 JSON 문서·알림 이메일에 함께 남긴다. 도구 응답에는 고정된 복구 안내를 반환하고 메일에는 HTTP 200과 도구 실패의 차이를 설명한다. 사유는 고정 enum만 허용하며 URL·키워드·메시지·스택 등 원시 입력을 새로 저장하지 않는다. 기존 문서는 계속 읽으며 DB 열·마이그레이션·운영 설정 변경은 없다.

## 검증

| 검사 | 결과 |
| --- | --- |
| 로컬 서버 전체·정적 검사 | 268 passed/0 failed/25 ignored, Clippy all-targets `-D warnings`, fmt·diff check 통과 |
| 운영 소스 비교 | 배포 커밋에 없는 운영 전용 파일 0개 |
| 격리·무결성 | 이번 8개 파일만 clean worktree에 커밋, manifest 637개와 source archive 470개 파일을 pinned commit에 대조, 업로드 11개 SHA-256 검사 통과 |
| Linux 고정 builder | 테스트 요약 59개, 444 passed/0 failed. 기본 ignored 31개 중 PostgreSQL application 18개·email/admin 2개를 별도 실행한 결과 포함 |
| 활성화 | 공유 잠금·이전 릴리스 확인·DB 백업·API readiness·관리자 경계·공식 메모리/점수·정적 페이지 검사 통과. 자동 rollback 없음 |
| 공개 HTTPS | apex/API `/healthz`, 사이트 `/` 모두 200 |
| 실제 JS 제공 | HTML의 app/payload/start 엔트리 3개가 packaged build와 SHA-256 일치 |
| 실제 MCP | JSON/SSE 입력 거절과 정상 recall의 응답·헤더·PostgreSQL trace 일치, 실패 사유 일치, 합성 비공개 sentinel 배제 |

운영에서 주입한 실패는 다음 두 건뿐이다. 알림 설정을 유지한 상태에서 수행했으므로 테스트 오류가 기존 알림 배치로 처리된다. 실제 사용자에게 인증 메일이나 별도 수동 이메일을 보내지 않았다.

| 형식 | trace | 결과 |
| --- | --- | --- |
| JSON | `dc4df5a8067e4590968f294b40d4c83a` | HTTP200/`isError=true`, 본문·헤더·DB trace 동일, `invalid_limit` |
| SSE | `263ab20520f24c529a81093f19696ee4` | HTTP200/`isError=true`, 본문·헤더·DB trace 동일, `invalid_task_category` |
| JSON 정상 | `27371e1643234c4e934c0fec9f224be4` | HTTP200/`isError=false`, 저장 오류·사유 없음 |

운영 `email_checkpoint`의 커서가 두 테스트 오류까지 전진한 것을 조회해, 기존 알림 작업자가 메일 제공자에 해당 배치를 접수한 뒤 체크포인트를 저장했음을 확인했다. 메일 본문의 사유 코드와 HTTP 상태 설명은 배포된 소스의 단위 테스트로 검증했다. 받은편지함 수신 여부는 별도 주장하지 않는다.

## 산출물과 복구

서버 `/srv/web-agentic-memory/builds/20261107T062322Z-main-9334e66/`에 소스·웹 archive, manifest, SHA256SUMS, 빌드·활성화 스크립트, Linux `build.log`와 사전 기준선을 보존한다. 로컬 Git 제외 경로 `build/deploy-4090/recall-reason-20261107/`에는 동일 산출물, `package.log`, `linux-build.log`, `activation.log`, `smoke.log`, 배포 전후 기준선, `post-deploy-check.json`, `post-deploy-database.json`과 검증 스크립트가 있다.

공유 원본의 README·데모 등 별도 작업 파일 20개는 배포 전후 SHA-256을 확인했다. 상위 agent-browser는 수정하지 않았다. 과거 원본 trace의 구체적인 거절 인수는 복원할 수 없으며, 이번 배포는 다음 오류부터 그 항목을 추적할 수 있게 한다.

복구는 [운영 릴리스 절차](../../deploy/4090/release/README.md)를 따른다. 이전 릴리스와 DB 백업을 보존한다.
