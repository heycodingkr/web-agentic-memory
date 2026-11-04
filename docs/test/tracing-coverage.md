# 오류 추적 보완: 웹 · 원격 MCP · CLI · stdio MCP

2026-11-04, issue #296. 사전 명세와 의존 순서는 issue 댓글에 구현 전에 기록했다. 기존 데모·README 변경과 분리한 worktree에서 수정·검증한다.

## 원인과 설계

보고된 운영 trace `f621e78e0471982e0c0f6d4ca2d41b7a`를 PostgreSQL에서 읽기 전용으로 조회했다. 같은 trace에는 `web/unknown/unhandled_error/404` client event와 telemetry intake 202 두 행만 있었다. 원래 URL·메시지는 수집하지 않아 해당 방문 경로를 복원할 수 없다.

설치된 SvelteKit 3.1은 framework/app 오류도 클라이언트 오류 훅에 전달한다. 기존 훅은 kind를 무시해 정상 404도 unhandled로 보고하고, 이동 중에는 이전 window 위치로 작업을 분류했다. 이제 framework/app 4xx는 기본 응답을 유지하며 일반 HTTP 오류로 기록하고, 이동 대상 route를 사용한다. unknown 오류와 5xx는 계속 추적·알림한다.

CLI/stdio MCP에는 기존에 HTTP·파싱 실패와 제한된 offline spool이 있었다. 확인된 공백은 서버 응답 trace 무시, 일부 CLI 및 stdio I/O 실패의 참조 미표시, 기기 연결의 unknown 분류, API 실패의 invalid_input 재분류였다. 원격 MCP는 HTTP 헤더만 제공하던 오류에도 본문 참조를 추가했다. 원격 dispatcher가 닫힌 operation/error를 response extension으로 넘겨 finite JSON/SSE 도구 오류를 기록한다. SSE 본문이나 idle 연결의 EOF를 읽지 않는다.

## 입력과 기대 결과

| ID | 입력 | 독립 oracle · 기대 결과 |
| --- | --- | --- |
| TRC-01 | framework 404 / app 403, 비공개 경로·메시지 sentinel | 기본 status/message 유지; client event는 http_error; 원시 값 없음; 기존 메일 필터가 일반 4xx 제외 |
| TRC-02 | 이전 페이지 dashboard에서 memories로 이동 중 framework 503 / unknown 오류 | 대상 catalog/device 분류와 unhandled_error; 4xx 속성만 가진 unknown 오류도 계속 추적 |
| TRC-03 | API 오류에 기존 trace/status/duration 부착 | navigation hook이 동일 참조·작업·경과 시간을 보존 |
| TRC-04 | 서버가 다른 유효 trace를 반환: JSON, empty response, recall, 잘못된 JSON | 사용자/telemetry trace가 서버 trace와 일치; invalid response는 별도 범주 |
| TRC-05 | 잘못된/대문자 서버 trace, 실제 link/device 경로 | 요청 trace로 정확히 대체; 기기 연결은 device; 메모리 목록은 catalog |
| TRC-06 | 잘못된 CLI stdin / 로컬 인증 부재 / MCP UTF-8 I/O 실패 | 원래 종료 코드·stdout 계약 유지; stderr 참조와 SQLite event가 일치; sentinel 없음 |
| TRC-07 | 실제 /mcp expand(level=999), JSON 및 SSE Accept 각각 | HTTP200/isError; 본문 _meta.trace_id = 헤더 = DB; expand/invalid_input; payload sentinel 없음 |
| TRC-08 | 원격 RPC parse 오류 / Stub API Server 실패 | error.data.trace_id로 조회 가능; 서버 실패가 internal_error로 유지 |
| TRC-09 | idle SSE GET / 기존 offline·opt-out 회귀 | 헤더·첫 frame 즉시 전달; 재전송은 같은 event; opt-out은 전송·spool 비활성 |

## 검증 근거

- 변경 전 focused tests에서 TRC-01/02, 서버 trace, device 분류, CLI 참조, 원격 MCP 본문 참조와 API 오류 분류가 실제로 실패했다.
- MCP invalid UTF-8 입력은 실제 실행 파일에서 종료 1·stdout 0 bytes·참조 없는 stderr를 재현한 뒤 보완했다.
- web unit tests: 80 통과. check: 오류 0, 기존 SetupGuide 경고 2개.
- 실제 Chrome + 격리 API + 실제 wam/wam-mcp smoke: 3 통과. 참조를 SQLite 행과 대조하고 404 화면·분류·비공개 marker 제외를 확인했다.
- Rust affected crates 전체: 50개 요약, 351 통과, 실패 0, 외부 서비스용 ignored 26개. `cargo clippy --locked -p wam-client -p wam-cli -p wam-mcp -p wam-server --all-targets -- -D warnings` 통과.
- 운영 pinned Linux build와 배포 후 검사 결과는 별도 배포 증거 문서에 기록한다.

코드 위치: `web/tests/client-hooks.test.mjs`, `web/e2e/local-smoke.spec.ts`, `crates/wam-client/src/tests.rs`, `crates/wam-client/src/telemetry.rs`, `crates/wam-cli/tests/input.rs`, `crates/wam-mcp/src/lib.rs`, `crates/wam-server/tests/diagnostics.rs`.

## 범위

정상 404 기록은 남긴다. 임의 경로는 기존 closed schema의 unknown일 수 있으며 URL·메시지·스택을 추가 수집하지 않는다. 유효한 endpoint를 만들 수 없는 설정 오류는 원격 업로드할 수 없어 로컬 stderr에 남는다. bounded spool은 무손실 journal이 아니며 이미 설치된 native client는 새 소스로 업데이트해야 한다. 운영 서버/웹 배포만으로 기존 사용자 컴퓨터의 실행 파일을 바꾸지는 않는다.
