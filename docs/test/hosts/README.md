# 호스트별 실행 문서

호스트마다 `<slug>.md` 한 개를 만든다. 형식은 [`../host-smoke.md`](../host-smoke.md) 4절 템플릿을 그대로 쓴다.
설치 명령과 설정 형식은 해당 호스트의 현재 문서나 실제 실행으로 확인한 값만 적는다. 추측하지 않는다.

| slug | 호스트 | 비고 |
|---|---|---|
| [`claude-code`](claude-code.md) | Claude Code CLI | 확인된 명령·설정·한도 기록 (2026-10-27) |
| [`codex`](codex.md) | Codex CLI | MCP 서브프로세스가 환경변수를 걸러낼 수 있다 (`host-smoke.md` 1절). 확인된 명령·설정·한도 기록 (2026-10-27) |
| [`meta-muse`](meta-muse.md) | Meta Muse (Muse Code, Meta Model API) | Muse Code와 Claude Code(Meta 모델) 경로는 확인, Codex(Meta 모델) 경로는 동작하지 않음 (2026-11-01, `../../plan/benchmark-readiness.md` W0-3) |
| [`arise`](arise.md) | Arise 브라우저 | 자체 stdio MCP와 앱 제어 소켓을 쓰는 host. WAM은 MCP 허브 연동 예정; 격리 하네스의 CDP와 구분 |
| [`chrome-mcp`](chrome-mcp.md) | Chrome + 브라우저 MCP | ChromeBridge/browser MCP와 Memory MCP를 분리 설치하는 host 조합 테스트 계약 |
| `generic-mcp` | 범용 MCP 클라이언트 | 프로토콜 준수 기준선, 완전 자동 |

각 slug는 `tests/env/host-profiles/<slug>/`의 어댑터 코드와 짝이 된다.

## Acceptance 범위

호스트 설치는 웹의 **복사 동작**과 분리해 추적한다. 복사만으로 설치 성공을 보고하지 않는다. 인수 테스트는 [`../host-smoke.md`](../host-smoke.md)의 `HOST-01~12`를 실행하고, 실행 host/version·template version·증거를 각 host 문서에 기록한다.

| ID | 호스트 경로 | 반드시 확인할 것 |
|---|---|---|
| HOST-01 | Claude Code + Memory MCP | `/guide` 복사 template이 현재 Claude 설정 문법으로 등록되고, guest recall과 linked member recall/write가 각각 status·권한 계약에 맞게 된다. |
| HOST-02 | Codex CLI + Memory MCP | 복사 명령/config로 등록하고 guest URL+keyword recall이 된다. host OAuth를 연결한 뒤 receipt/write 소유권이 맞는지 별도 확인한다. |
| HOST-03 | Claude Code + ChromeBridge/browser MCP | guest 및 linked 계정에서 Memory MCP와 browser MCP가 공존하고 fixture 작업 전에 URL+keyword recall이 수행된다. |
| HOST-04 | Codex CLI + ChromeBridge/browser MCP | guest 및 linked 계정에서 두 서버가 공존하며 fixture oracle, tool trace, receipt/no-score가 일치한다. |
| HOST-05 | Arise browser MCP + WAM MCP hub | HOST-10/11과 함께 guest·WAM-linked member를 각각 실행한다. 브라우저·외부 MCP tool 공존, URL/task/keyword scope, receipt와 score 권한을 확인한다. tab-aware local normalization은 native adapter test로 별도 판정한다. |
| HOST-06 | 제거·재연결 | 임시 host 설정에서 Memory MCP만 제거·재등록한다. 브라우저 MCP와 실제 사용자 설정은 손대지 않는다. |
| HOST-07 | MCP server 독립 등록·상태 | 각 MCP 서버의 목록·도구·연결 상태를 구분하고, 분리·장애가 다른 서버 상태에 영향을 주지 않는지 확인한다. |
| HOST-08 | Chrome 일반 browser MCP + WAM 경로 | browser MCP가 웹 작업, WAM이 recall을 맡는지 검증한다. WAM에 CDP 설치를 요구하지 않는다. |
| HOST-09 | Chrome tab-aware local adapter | 자동 현재 탭 회수를 광고하는 구현에서 connector API, URL 정규화, 권한 거부와 target 오류를 검증한다. 미구현이면 지원 주장도 하지 않는다. |
| HOST-10 | Arise MCP Hub 등록 | 실제 Arise session에서 browser와 WAM tools의 공존·호출을 확인하고 내부 socket 직접 연결은 금지한다. |
| HOST-11 | Arise/WAM 인증 분리 | Arise 로그인·쿠키·웹 로그인만으로 receipt가 생기지 않고 WAM OAuth 또는 별도 device link 뒤에만 권한이 생기는지 확인한다. |
| HOST-12 | 연동 장애·정리 | MCP/network/browser/permission 장애에서 false success와 권한 side effect가 없고 일회용 자원만 정리되는지 확인한다. |

호스트별 실제 실행 문서는 설치 인터페이스 구현과 검증이 시작된 뒤 추가한다. 현재 인덱스/acceptance spec은 실제 호스트에서 테스트를 완료했다는 표시가 아니다. Chrome·Arise 결정과 test detail은 [`../../design/browser-host-integration.md`](../../design/browser-host-integration.md)를 따른다.
