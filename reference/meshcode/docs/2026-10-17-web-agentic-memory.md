# 웹 에이전틱 메모리 v1 — 로컬 검증

2026-10-17 KST. 구현·검증 완료, 운영 배포와 사용자 앱 교체는 수행하지 않았다. 서버의 `WEB_AGENTIC_MEMORY_ENABLED` 기본값은 off다. [명세](../../../../specs/web-agentic-memory.md).

## 검증 결과

| 대상 | 실행·범위 | 결과 |
| --- | --- | --- |
| 서버 기준선 | billing server/auth/telemetry, 변경 전 | 567 PASS / 0 FAIL |
| 서버 회귀·기능 | `BILLING_SCRYPT_N=1024 bun test test/server.test.ts test/auth test/auth-apple.test.ts test/auth-google.test.ts test/telemetry test/web-memory.test.ts` (packages/billing) | 578 PASS / 0 FAIL, 22파일 |
| 실제 PostgreSQL | `bash scripts/probe-web-memory-postgres.sh` (packages/billing) | 2 PASS / 0 FAIL |
| 서버 타입 | `bun run typecheck` (packages/billing) | PASS |
| 서버 빌드 | `bun build src/main.ts --target=bun --outdir /tmp/arise-web-memory-server-build` | PASS |
| 관리자 타입·UI 진단 | `bun run check` (apps/admin) | 0 errors / 0 warnings |
| 관리자 빌드 | `bun run build` (apps/admin) | PASS |
| Rust 기능 회귀 | memory tool contract7 + web catalog7 + workflow reports6 | 20 PASS |
| Rust 관련 라이브러리 | host110; chat693+3ignored; agent_browser176+2ignored; llm285; personal store18; personal context4 | PASS (명시한 ignored 제외) |
| 실제 MCP 전송 | `cargo test -p agent_browser --test mcp_embedded_socket` | 11 PASS |
| 최종 앱·MCP 빌드 | `cargo build -p agent_browser --bin agent_browser --bin agent_browser_app` | PASS, 기존 compiler/dependency 경고 존재 |
| 동적 설정 MCP 스모크 | `python3 scripts/probe_web_agentic_memory.py --out target/e2e-logs/web-memory-mcp-final` (agent-browser) | PASS |
| 관리자 브라우저 입력 스모크 | `python3 scripts/probe_web_memory_ui.py --out target/e2e-logs/web-memory-admin-ui-v3` (agent-browser) | PASS |

서버 전체 테스트가 아닌 관련 server/auth/telemetry 회귀와 신규 기능을 실행했다. Rust 결과도 변경 관련 라이브러리·메모리·MCP 범위이며 전체 GUI 통합 테스트 완료를 뜻하지 않는다.

## 재현과 확인한 동작

- 익명 사용자는 승인된 카탈로그만 읽고 최소 정보 보고서만 제출한다. 일반 사용자 JWT는 관리자 읽기·수정·검증·승인·롤백에403, 미인증은401이다. 악성 추가 필드·텍스트·URL·역할 데이터는400, 본문 초과413, 요청 제한429, 저장소 실패503이다.
- 후보는 승인 전까지 공개 조회에 나오지 않는다. 정확한 digest의 검증 자료가 있어야 승인되며, 다른 버전에서 만든 후보는409다. 빈 승인본은 번들 항목을 철회하고, 롤백도 새로운 revision을 만든다.
- 실제 PostgreSQL은 임시 loopback 클러스터를 새로 만들었다. 10개 병렬 변경 보존, 예외 시 transaction rollback, 같은 base의 두 후보 중 한 건만 게시되는 것, 기능 off에서404 후 다시 on일 때 게시본 보존을 확인했다. 종료 시 클러스터를 제거했다. 운영 DB에는 접속하지 않았다.
- 실제 stdio MCP 한 프로세스에서 off→on→off→on을 수행했다. 카탈로그 GET은 loopback 한 번이며, 개인 메모리 저장·조회·확장은 유지되고 web 항목 수정·삭제는 거부됐다. 개인 DB 손상 시에도 web 조회는 가능하며 손상된 파일을 덮어쓰지 않는 별도 회귀가 통과했다.
- 관리자 스모크는 합성 계정·임시 Arise 프로필·자체 로컬 서버를 사용했다. 실제 브라우저 도구의 좌표 클릭과 입력으로 작업 방법 작성→후보 등록→공개 미노출 확인→스모크 자료 입력→검증→승인→카탈로그 반영을 확인했다. `approved.png` 등 브라우저 캡처를 이미지 도구로 열어 확인했다.
- 앞선 UI harness 시도는 wheel scroll이 멈추기 전에 좌표를 읽어 버튼을 놓쳤다. 최종 harness는 새 snapshot, 200ms 간격의 안정된 위치, hit test를 확인한 뒤 한 번 클릭한다. 이 결과로 Medium 등 모든 편집기의 스크롤 문제가 해결됐다고 주장하지 않는다.

구현 전 실패로 확인한 세부 오류: 첫 게시 뒤 초기 seed로 롤백할 기록 누락, streaming BodyLimitError를400으로 삼키는 처리, Rust와 서버의 www 도메인 정규화 불일치, 실제 등록 이름과 다른 navigation/wait allowlist, 개인 저장소 오류가 독립 web 조회까지 막는 처리. 각 회귀를 통과시키는 수정 후 최종 결과를 수집했다.

## 아티팩트와 재검토 환경

서버 로그:

- `/tmp/arise-web-memory-server-final.log`
- `/tmp/arise-web-memory-postgres-final.log`
- `/tmp/arise-web-memory-server-types-final.log`
- `/tmp/arise-web-memory-server-build.log`
- `/tmp/arise-web-memory-admin-check.log`
- `/tmp/arise-web-memory-admin-build.log`

agent-browser 로그와 결과:

- `/tmp/arise-web-memory-regressions-final.log`
- `/tmp/arise-web-memory-mcp-integration-final.log`
- `/tmp/arise-web-memory-build-final.log`
- `/tmp/arise-web-memory-mcp-final.log`
- `target/e2e-logs/web-memory-mcp-final/result.json`
- `target/e2e-logs/web-memory-admin-ui-v3/result.json`, `approved.png`, `after.png`

로컬 관리자 preview: http://127.0.0.1:5714/web-memory (frontend launcher PID64944). API fixture: http://127.0.0.1:65467 (PID83051). 둘 다 detached 개발 프로세스이며 인메모리 데이터·합성 계정만 사용한다. 로그인 정보는 `/tmp/arise-web-memory-stage.json` 및 fixture 소스에 있다. 테스트가 만든 Arise/CEF 프로세스는 종료했고 사용자가 실행하던 앱·프로필은 보존했다.

Preview 로그:

- frontend: `/var/folders/hn/4d_172w53_58g8kg255mlw_w0000gn/T/meshcode-detached-68789-1792161659707.log`
- fixture: `/var/folders/hn/4d_172w53_58g8kg255mlw_w0000gn/T/meshcode-detached-68789-1792163533268.log`

## 한계와 롤백

네이티브 설정의 label·값·저장·독립성은 코드 테스트로 확인했다. MeshCode desktop screenshot은 GUI IPC Connection refused로 실패하여 네이티브 설정 화면의 외형은 시각 확인하지 못했다. Arise CEF 브라우저 PNG 검증과 구분한다.

기존 대화에 들어간 메모리는 설정 off로 삭제되지 않는다. 이미 시작된 업로드는 동의 철회 뒤에도 끝날 수 있다. 보고서는 관찰된 도메인과 완료된 도구 결과의 제한된 표본이며, 사이트 저장 성공의 증거가 아니다. 검증 자료는 관리자가 외부 스모크 결과를 확인한 기록으로, 서버는 제출된 명령을 실행하지 않는다. 후보 작성은 수동이며 자유 텍스트·이미지 자동 수집/후보 생성은 구현하지 않았다.

서버 flag off 또는 repository 주입 제거는 신규 경로를404로 돌리고 기존 데이터를 유지한다. 클라이언트의 웹 메모리 off는 서버 조회·향후 주입만 중지하며 개인 메모리와 데이터 제공 동의는 독립적이다. 본 작업에서 운영 flag 활성화·운영 배포·실사용 데이터 수집은 수행하지 않았다.
