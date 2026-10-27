# 호스트별 실행 문서

호스트마다 `<slug>.md` 한 개를 만든다. 형식은 [`../host-smoke.md`](../host-smoke.md) 4절 템플릿을 그대로 쓴다.
설치 명령과 설정 형식은 해당 호스트의 현재 문서나 실제 실행으로 확인한 값만 적는다. 추측하지 않는다.

| slug | 호스트 | 비고 |
|---|---|---|
| [`claude-code`](claude-code.md) | Claude Code CLI | 확인된 명령·설정·한도 기록 (2026-10-27) |
| [`codex`](codex.md) | Codex CLI | MCP 서브프로세스가 환경변수를 걸러낼 수 있다 (`host-smoke.md` 1절). 확인된 명령·설정·한도 기록 (2026-10-27) |
| `meta-muse` | Meta Muse CLI | MCP 지원 여부부터 조사한다 (`../../plan/benchmark-readiness.md` W0-3) |
| [`arise`](arise.md) | Arise 브라우저 | 자동 주입 호스트. 격리 실행 레시피와 함정 확인됨 |
| [`chrome-mcp`](chrome-mcp.md) | Chrome + 브라우저 MCP | ChromeBridge/browser MCP와 Memory MCP를 분리 설치하는 host 조합 테스트 계약 |
| `generic-mcp` | 범용 MCP 클라이언트 | 프로토콜 준수 기준선, 완전 자동 |

각 slug는 `tests/env/host-profiles/<slug>/`의 어댑터 코드와 짝이 된다.

## Acceptance 범위

호스트 설치는 웹의 **복사 동작**과 분리해 추적한다. 복사만으로 설치 성공을 보고하지 않는다. 인수 테스트는 [`../host-smoke.md`](../host-smoke.md)의 `HOST-01~06`을 실행하고, 실행 host/version·template version·증거를 각 host 문서에 기록한다.

| ID | 호스트 경로 | 반드시 확인할 것 |
|---|---|---|
| HOST-01 | Claude Code + Memory MCP | `/guide` 복사 template이 현재 Claude 설정 문법으로 등록되고, guest recall과 linked member recall/write가 각각 status·권한 계약에 맞게 된다. |
| HOST-02 | Codex CLI + Memory MCP | 복사 명령/config로 등록하고 guest URL+keyword recall이 된다. host OAuth를 연결한 뒤 receipt/write 소유권이 맞는지 별도 확인한다. |
| HOST-03 | Claude Code + ChromeBridge/browser MCP | guest 및 linked 계정에서 Memory MCP와 browser MCP가 공존하고 fixture 작업 전에 URL+keyword recall이 수행된다. |
| HOST-04 | Codex CLI + ChromeBridge/browser MCP | guest 및 linked 계정에서 두 서버가 공존하며 fixture oracle, tool trace, receipt/no-score가 일치한다. |
| HOST-05 | Arise adapter | 미연결/익명과 device-linked 계정을 각각 실행해 URL/task/keyword scope, receipt, score 권한이 MCP와 일치한다. |
| HOST-06 | 제거·재연결 | 임시 host 설정에서 Memory MCP만 제거·재등록한다. 브라우저 MCP와 실제 사용자 설정은 손대지 않는다. |

호스트별 실제 실행 문서는 설치 인터페이스 구현과 검증이 시작된 뒤 추가한다. 현재 인덱스/acceptance spec은 실제 호스트에서 테스트를 완료했다는 표시가 아니다.
