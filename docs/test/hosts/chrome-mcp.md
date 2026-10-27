# 호스트 조합: Chrome + Browser MCP

상태: 테스트 계약 초안. Chrome browser MCP/ChromeBridge의 선택 패키지, 버전, transport는 이 저장소에서 아직 고정·확인하지 않았다. 여기서 말하는 Chrome 고객은 Chrome을 browser MCP로 Claude Code 또는 Codex CLI에 연결한 경로다. Memory MCP는 agent host에 별도로 등록한다.

## 지원 범위

- `/guide`는 Memory MCP의 설치 문자열을 제공하고, browser MCP의 연결 절차를 별도로 안내한다. Memory MCP 복사만으로 ChromeBridge가 설치되었다고 표시하지 않는다.
- ChromeBridge/browser MCP는 Chrome 탭·페이지 조작 도구를 제공한다. Memory MCP는 승인 메모리 recall/expand와 권한이 있는 propose/outcome을 제공한다.
- 테스트는 Claude Code와 Codex CLI를 각각 host로 실행하고, Chrome은 일회용 프로필에서 local fixture에만 접속한다. 실제 개인 Chrome 프로필, 쿠키, Naver 계정/게시물은 사용하지 않는다.
- host auth는 browser MCP의 Chrome 권한과 독립적이다. Chrome 사이트 세션이나 사용자가 웹사이트에 로그인한 상태만으로 Memory MCP 계정이 연결되지 않는다.

## 실행 경로

`HOST-03`은 Claude Code + Memory MCP + ChromeBridge/browser MCP, `HOST-04`는 Codex CLI + 같은 두 도구 조합이다. 각 host에서 `CUST-01`의 guest run과 `CUST-02`의 account-only → explicitly linked run을 모두 한다.

자동 현재 탭 회수는 `HOST-03/04`에서 **로컬 tab-aware adapter 변형도 필수**다. WAM이 CDP를 별도로 설치할 필요는 없으며 선택한 browser connector가 지원하는 CDP/extension/local interface를 사용한다. 모델이 만들어 준 URL을 원격 MCP에 전달하는 것만으로는 전송 전 정규화의 개인정보 보장을 통과하지 않는다.

1. `/guide`에서 Memory MCP 템플릿을 복사하고 임시 host profile에 등록한다. browser MCP는 같은 세션에 별도로 등록한다.
2. MCP server list와 `tools/list`에서 memory/browser 도구 및 server origin을 구분한다. 설치 문구가 두 도구를 혼동하지 않는지 확인한다.
3. Memory MCP는 guest 상태로, ChromeBridge는 일회용 Chrome fixture 탭으로 연결한다. Naver 유사 fixture에 한국어 작성 요청을 주고 browser action 전 recall을 확인한다.
4. 실제 `web_memory_recall` tool-call arguments와 server ingress를 모두 검사한다. canonical route, `compose-draft`, `naver/blog/write`만 있어야 한다. 원 URL/query sentinel, prompt, page text, cookies가 tool input 또는 ingress에 나타나면 개인정보 합격이 아니다. 서버 측 거부만으로는 전송 전 정규화를 입증한 것으로 보지 않는다.
5. fixture saved-oracle에서 작업 결과를 확인한다. guest에는 receipt/outcome/propose/score event가 없어야 한다.
6. 웹 계정만 로그인하고 host link를 하지 않은 익명 run을 먼저 확인한다. 원격 MCP 회원 run은 CUST-02에서 Claude와 Codex 각각 host OAuth를 별도로 검증한다. 이 endpoint/host 조합이 익명 read와 host OAuth write를 지원하지 않으면 CUST-02를 성공 처리하지 않고, local stdio + `wam link` 대체 경로는 CUST-04에서 별도로 실행·판정한다. 연결 후 recall receipt 소유권과 outcome score ledger를 확인한다.
7. Memory MCP/browser MCP 각각을 끊거나 승인 거부해도 다른 server를 설치 완료로 오인하지 않고 false success가 없어야 한다.

## 로컬 tab-aware adapter 변형

1. 일회용 Chrome 프로필에서 선택한 browser connector를 실행한다. 사용자의 기본 Chrome profile이나 cookies를 가져오지 않는다. connector가 직접 CDP endpoint를 요구할 때만 loopback 임시 포트를 쓴다.
2. 사용자가 해당 브라우저 연결을 승인한 뒤, 로컬 WAM MCP adapter가 connector의 문서화된 local interface(CDP, extension API 등)를 통해 현재 선택된 page URL만 읽는다. ChromeBridge 내부 session을 별도 프로세스가 공유한다고 가정하지 않는다. 실제 연결 경로를 기록한다.
3. query·fragment·userinfo·동적 식별자를 `wam-core`로 정규화하고, 허용 키워드 및 `task_category`만 붙여 recall 서버로 보낸다. DOM, 페이지 본문, 쿠키, 전체 URL은 전송하지 않는다.
4. agent tool trace, local adapter 입력/출력 경계, server ingress를 비교해 원 URL sentinel이 정규화 전 outbound에 포함되지 않았음을 확인한다. 원 URL을 거부한 서버 로그만으로는 이 검사를 통과하지 못한다.
5. guest, 웹 계정만 로그인한 상태, device-linked member를 각각 실행한다. browser permission 거부·target 소실·잘못된 탭 선택 시 recall/write/receipt/score가 0이고 재시도 전 확인이 필요하다.
6. 직접 CDP endpoint를 사용하면 loopback에만 열고 run 종료 뒤 process/profile/endpoint를 정리한다. debugging port를 네트워크에 노출하거나 기본 프로필로 조용히 fallback하면 실패다.

## 통과 증거

| 관측 | 근거 |
|---|---|
| host/브라우저 격리 | 임시 host config diff, Chrome profile 경로, 테스트 프로세스 정리 결과 |
| 도구 분리 | host server list, 각 server의 `tools/list`, agent trace |
| recall 정확성 | canonical URL+keyword request 요약, returned memory ID/version/rank |
| browser 작업 | action trace와 독립 fixture saved-oracle |
| guest/linked 권한 | auth status, receipt 수와 소유권, propose/outcome 결과, DB score delta |
| 개인정보 보호 | request, process args, logs, cache, evidence에서 센티널·query·prompt·cookie 검색 |

## 미확인 사항

- 릴리스에서 지원할 ChromeBridge/browser MCP 제품·버전·설치 명령과 Claude/Codex별 설정 형식.
- ChromeBridge가 기존 Chrome profile을 사용할지, 격리 profile을 직접 만들 수 있는지. 테스트가 임시 profile을 만들 수 없다면 격리 브라우저 runner를 쓴다.
- browser MCP의 승인 UX 및 tool naming/version. adapter mapping은 실제 선택 뒤에만 고정한다.

이 문서는 host protocol을 실제 검증했다고 뜻하지 않는다. 선택된 browser MCP가 정해지면 버전, source, 설치 절차와 실행 증거를 이 문서에 기록한다. 관련 테스트는 [`../customer-paths.md`](../customer-paths.md), [`../host-smoke.md`](../host-smoke.md), `HOST-03~04`, `CUST-01~02`, `MCPF-05~07`, `MCP-20/24`, `BEH-01/08`, `FAIL-20/24/25`다.
