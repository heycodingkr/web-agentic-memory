# 웹 · CLI · MCP 오류 추적 보완 4090 배포

2026-11-04, issue #296. [추적 명세·재현 결과](tracing-coverage.md)의 코드를 운영에 배포했다.

| 항목 | 결과 |
| --- | --- |
| 소스 커밋 | `fd3b35c6ab62abb5aa86494bcf6054ee7e20303c` (origin/main에 push) |
| 운영 릴리스 | `20261104T055411Z-main-fd3b35c` |
| 이전 릴리스 | `20261104T043407Z-main-04cb133` |
| API 바이너리 SHA-256 | `add0276617fbce72c9d4c8b896e423112317ca501479cbcd43ba4bc8d2b594c6` |
| DB 백업 | `/var/backups/web-agentic-memory/20261104T055411Z-main-fd3b35c/production-before.dump` |
| 운영 전용 미커밋 코드 | compare-with-production: 0개 |
| 데이터·설정 보존 | 메모리 38개, 기존 version ID 38개 전부 일치, 공식 점수 880, 설정 파일 해시 8개 일치 |

## 변경과 검증

정상 SvelteKit 3 framework/app 4xx는 기본 응답을 유지하면서 `http_error`로 기록한다. 알림 대상으로 잘못 분류하던 문제와 이동 전 페이지로 작업을 분류하던 문제를 고쳤다. CLI/stdio MCP는 서버 trace 보존·기기 작업 분류·로컬 실패 참조 표시를 보완했다. 원격 MCP는 JSON/SSE의 도구 오류에 본문 참조와 닫힌 작업·오류 범주를 연결한다. raw URL·메시지·stack·계정·토큰 수집을 추가하지 않았다.

| 검사 | 결과 |
| --- | --- |
| 변경 전 재현 | 웹 hook 4건, native trace/device/CLI 참조, MCP 오류 분류·원격 본문 참조의 focused red 확인. stdio UTF-8 I/O도 실제 실행 파일에서 참조 미표시 재현 |
| 로컬 Rust affected crates 전체 | 50개 요약, 351 통과, 실패 0, 외부 서비스용 ignored 26 |
| Clippy | affected 4 crates/all-targets, warnings-as-errors 통과 |
| web unit/check | 80 통과, check 오류 0 · 기존 SetupGuide 경고 2개 |
| 실제 Chrome + API + CLI/stdio MCP | smoke 3 통과. 메모리 게시·갱신·삭제와 오류 참조/SQLite 행 일치, 정상 404 분류·화면 확인 |
| 운영용 web build | production origin으로 고정한 clean commit 빌드 통과 |
| pinned Linux builder | 59개 테스트 요약, 439 통과, 실패 0, ignored 31. 명시 실행한 disposable PostgreSQL 검사와 release build 통과 |
| 업로드 무결성 | source/web archive, manifest, build/activation script, 이전 release 포인터 SHA-256 전부 일치 |
| 활성화 | 공유 배포 잠금·DB 백업·API/admin/review 경계·공식 작성자/점수·정적 페이지 검사 통과. 자동 rollback 없음 |
| 공개 HTTPS | 사이트 / 및 API /healthz 모두 200 |
| 실제 제공 client hook | `/_app/immutable/entry/app.waYCOfip.js` SHA-256 = packaged build |
| 운영 MCP 참조 | parse 400 본문·헤더·PostgreSQL trace 일치. JSON/SSE status 성공 각각 dashboard/200으로 저장 |

배포된 client hook bundle의 SHA-256은 `a73eeabe013e2e0a688d6f59bd1dc1019f135f5554a128352de67a84f3bf530f`다.

운영에서 직접 대조한 trace는 다음과 같다. 모두 요청 본문·사용자 정보를 제외한 진단 메타데이터다.

| trace | 응답 · 저장 결과 |
| --- | --- |
| `0613731f449f49cab8ba61100d815d9b` | /mcp parse 400, error.data.trace_id = 응답 헤더 = PostgreSQL, mcp/unknown/http_error/400 |
| `df134b1dd0a34c14a0229179cbde589c` | JSON status 성공, mcp/dashboard/200, 오류 없음 |
| `f3d689b93f144eaf9ea06cac05213fb4` | finite SSE status 성공, mcp/dashboard/200, 오류 없음 |

운영 검사는 정상 요청과 일반 HTTP 400만 사용했다. 알림을 만들 수 있는 합성 unhandled/tool 오류는 로컬 fixture에서만 검증했다. 운영 JSON/SSE 도구 실패의 별도 재주입은 하지 않았다.

## 산출물과 복구

서버 `/srv/web-agentic-memory/builds/20261104T055411Z-main-fd3b35c/`에 source/web archive·manifest·검증 script·서버 바이너리가 있다. 로컬 별도 worktree의 gitignored `build/`에는 local/Linux test 로그, package·activation 로그, 배포 전후 기준선, 공개 번들 해시와 trace 대조 JSON을 보관한다.

활성 배포는 API/정적 웹이다. CLI/stdio MCP 수정은 같은 커밋으로 push했고 실제 native 실행 파일로 로컬 검증했다. 기존 사용자 설치본은 소스를 업데이트해 다시 설치해야 반영된다. 로컬 테스트 프로세스와 일회용 fixture 데이터는 종료·정리했다. 원래 작업 트리의 데모·README 변경 6개는 fast-forward 전후 byte 단위 해시가 일치했다.

문제가 생기면 [운영 릴리스 절차](../../deploy/4090/release/README.md)에 따라 current를 이전 release로 돌리고 API를 재시작한다. 이 변경은 DB 스키마와 운영 설정을 바꾸지 않았다.
