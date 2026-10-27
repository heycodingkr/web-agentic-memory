# 고객별 설치·메모리 호출 여정 테스트

상태: 개발 전 인수 기준. `MCP-`, `CLI-`, `RCL-`, `LINK-`, `SCORE-`의 컴포넌트 테스트만으로는 가입 상태와 실제 호스트 조합이 끝까지 연결됐다고 볼 수 없다. 이 문서는 홈페이지 방문부터 설치, URL·키워드 recall, 브라우저 작업, 영수증·점수 권한까지의 교차 계층 경로를 고정한다.

## 1. 제품 경계와 공통 픽스처

- 홈페이지와 `/guide`의 공개 설치·recall 안내는 가입 없이 볼 수 있다. 설정 문자열 복사는 로컬 설치를 증명하지 않으며, Memory MCP 설치가 Chrome 브라우저 MCP까지 설치하지 않는다.
- Chrome 경로는 Claude Code 또는 Codex CLI에 Memory MCP와 ChromeBridge/Chrome browser MCP를 각각 연결하는 조합이다. 브라우저 MCP가 현재 Chrome 탭을 조작하고 Memory MCP가 검색·기여·영수증 도구를 제공한다.
- Arise 경로는 Arise의 자체 browser MCP와 외부 MCP 허브를 쓴다. 고객은 WAM remote/local MCP를 허브에 추가하고, Arise agent는 `mcp_tool_search`/`mcp_call_tool`과 browser 도구를 함께 사용한다. Arise 연동은 CDP가 아니며, 앱 내부 제어 소켓을 WAM이 직접 호출하지 않는다.
- 서버는 공개 승인 메모리의 익명 읽기를 허용한다. 웹사이트 계정이 있어도 해당 host/browser에 OAuth 또는 device link를 하지 않았다면 그 호출은 익명이다. 계정 로그인만으로 host 자격증명을 조용히 공유하지 않는다.
- 테스트는 loopback 서버, 합성 사용자, 일회용 Chrome/Claude/Codex/Arise 프로필, 독립 fixture oracle, disposable DB만 사용한다. 실제 Naver에 접근하거나 실제 글을 발행하지 않는다.

공통 seed는 별도 계정 A가 작성자로 등록된 public memory와, fixture server가 발급한 `http://naver.localhost:<port>/editor/<run_id>` URL이다. 정규화 결과는 `/editor/:id`, task는 `compose-draft`, 허용 어휘 키워드는 `naver`, `blog`, `write`로 둔다. URL의 run id와 query에는 가짜 센티널을 넣어도 정규화·sink 검색 뒤 남지 않아야 한다. fixture에 같은 host의 다른 route/task 메모리와 다른 host 메모리도 넣어 잘못된 범위 결과를 잡는다. 각 회원 run에는 별도 소비자 B와 B가 작성한 자기 메모리도 준비해 타인 사용과 self-use를 구분한다.

사용자 프롬프트는 “네이버 블로그 글 초안을 작성해 편집기에 저장해줘”로 둔다. 에이전트는 작업 의도에서 허용 어휘의 키워드만 뽑고 `task_category=compose-draft`를 전달한다. host-native adapter가 구현된 경우에도 같은 규칙을 적용한다. 원문 프롬프트와 전체 URL은 recall 요청에 포함하지 않는다.

## 2. 계정 상태 공통 계약

| 상태 | recall·expand | receipt / outcome | propose / 기여 | 점수 |
|---|---|---|---|---|
| 비회원·미연결 host | 공개 승인 메모리 조회 가능 | receipt 없음, outcome 거부 | 거부 | 익명 사용 집계만 가능, user 점수 0 |
| 웹 회원이지만 MCP/CLI/Arise 미연결 | 위와 동일. 웹 로그인만으로 인증된 호출로 바뀌지 않음 | receipt 없음, outcome 거부 | 거부 | 0 |
| host 인증까지 연결한 회원 | 동일한 공개 결과 + 계정에 묶인 receipt | 본인 receipt만 1회 outcome 보고 | 별도 명시적 동의 후 허용 | 다른 계정 작성 메모리의 검증된 성공 사용만 작성자에게 지급; self-use·실패·중복은 0 |

`web_memory_status`/`wam status`는 익명, 로그인 필요, 연결 완료, 만료·해지 상태를 구분하되 token, 이메일, 내부 user ID를 노출하지 않는다. `receipt_id`는 인증된 recall에만 있고 account는 server-side credential/device에서 정한다. 요청 입력으로 author/consumer ID를 바꿀 수 없다.

## 3. 고객 여정 케이스

| ID | 고객·호스트 경로 | 실행 | 합격 oracle |
|---|---|---|---|
| CUST-01 | 비회원 · 홈페이지 `/guide` → Claude Code/Codex + ChromeBridge | 공개 방문자가 `/guide`의 **원격 MCP** 카드를 골라 토큰 없는 템플릿을 그대로 복사·등록하고 ChromeBridge/browser MCP는 별도로 연결한다. 두 서버가 같은 agent session에 나타나는지 확인하고, 프롬프트에서 recall을 지시하지 않은 채 fixture 작업을 요청한다. | 가입·로그인 없이 `/guide`와 복사가 된다. clipboard가 canonical remote template과 일치하고 UI는 복사만 완료됐다고 표시한다. agent는 browser action 전에 canonical URL pattern + `naver/blog/write` + `compose-draft`로 recall하고 정확 scope의 승인 hint를 받는다. 원 URL/query는 recall server에 보내지 않는다. Chrome fixture oracle에는 초안이 저장된다. receipt·propose·outcome·score event는 0이며 메모리 설치가 browser MCP 설치로 오인되지 않는다. Claude와 Codex에서 각각 실행한다. |
| CUST-02 | 회원 · Claude Code/Codex + ChromeBridge · 원격 MCP | 테스트 웹 계정만 로그인하고 host OAuth를 하지 않은 상태로 시작한다. CUST-01의 원격 MCP 연결에서 익명 recall과 쓰기 거부를 확인하고, 같은 host에서 명시적으로 OAuth를 완료해 같은 recall/task를 반복한다. | 링크 전은 CUST-01과 같고 status가 익명/미연결이다. 웹 계정 로그인·Chrome 쿠키만으로 인증되지 않는다. host OAuth 승인 뒤에만 status가 연결됨을 표시하고, 이메일·내부 ID 없이 recall에 연결 계정 소유 receipt가 붙는다. fixture 성공을 receipt 소유자가 한 번 보고할 수 있다. 작성자와 소비자가 다른 계정일 때만 정책 점수가 작성자 원장에 추가되며 self-use와 receipt 재전송은 추가 지급하지 않는다. 한 host가 익명 read + 원격 OAuth write를 지원하지 않으면 이 케이스는 통과가 아니라 미지원/실패로 기록하고, local stdio 대체 경로는 CUST-04에서 별도 판정한다. |
| CUST-03 | 비회원 · 홈페이지 CLI 카드 → `wam` CLI / Claude/Codex instruction + CLI | 공개 `/guide`에서 OS별 CLI 설치 명령을 복사해 일회용 경로에 설치·검증한다. agent가 fixture 작업 전에 `wam recall --stdin --json`으로 현재 URL과 `naver/blog/write`만 담은 입력 객체를 보낸다. 같은 안전 fixture URL을 positional 형식으로도 호출해 MCP 결과와 비교한다. | 로그인 없이 설치 카드·checksum 안내·첫 public recall을 이용할 수 있다. 복사만으로 설치 완료라 표시하지 않는다. 공개 hint와 같은 순서를 받으며 키워드 allowlist 밖 값은 요청에서 빠진다. receipt가 없고 `wam propose`/`wam outcome`은 인증 오류(exit code 3)로 끝나며 서버 write 및 score event는 0이다. 민감 URL을 사용한 stdin 입력이 원 URL/query를 서버·cache·WAM 로그·process args에 남기지 않는다. 자유형 사용자 프롬프트는 CLI 입력에 포함하지 않는다. |
| CUST-04 | 회원 · `wam link` / 로컬 stdio MCP / CLI | 웹 회원이 device code를 `/link`에서 승인하고 local `wam`이 토큰을 OS secure storage에 저장한다. `wam status`, `wam recall`, 필요한 경우 로컬 MCP tool call과 `wam outcome`을 실행한다. | 계정 연결 전에는 익명, 승인 후에는 연결된 계정으로 전환된다. 토큰은 설정·argv·로그에 없고 DB에는 hash만 있다. remote MCP와 같은 version/hint/순위를 주고 receipt와 outcome 소유권은 연결 계정에 묶인다. 다른 사용자의 receipt는 거부되고 중복 결과는 멱등이다. |
| CUST-05 | 비회원 · Arise Browser + MCP 허브 | 일회용 Arise 프로필에서 `/guide`의 WAM MCP 설정을 Arise MCP 허브에 추가한다. browser 도구와 `mcp_tool_search`/`mcp_call_tool`을 같은 agent session에 노출한 뒤 fixture tab 작업을 요청한다. | 로그인 없이 WAM 공개 recall이 된다. agent는 browser 도구로 tab URL을 읽고 WAM recall을 browser action 전에 호출한다. canonical URL pattern·허용 키워드·task만 전달하고 hint로 fixture 작업을 끝낸다. receipt·outcome·propose·user 점수는 0이다. WAM은 Arise 내부 Unix socket에 직접 연결하지 않는다. |
| CUST-06 | 회원 · Arise Browser + MCP 허브 | WAM 웹 계정만 로그인하고 Arise MCP 허브에는 credential이 없는 상태에서 CUST-05를 실행한다. 이후 Arise MCP 허브의 WAM OAuth 로그인으로 같은 task를 수행하고 revoke/expiry도 확인한다. OAuth를 지원하지 않는 호스트는 CUST-06을 통과한 것으로 보지 않는다. local WAM stdio + `wam link`는 별도 Arise host 조합으로 실행·기록해야 하며 Claude/Codex의 CUST-04 통과로 대체할 수 없다. | 웹 로그인·Arise 로그인·사이트 쿠키만 있는 첫 run은 익명이며 receipt/write/score가 없다. WAM OAuth 승인 뒤에만 WAM 계정 receipt가 생긴다. MCP/CLI와 같은 catalog version, URL 범위, 키워드 순위, 권한 정책을 쓴다. oracle이 성공을 입증한 뒤 receipt 소유자가 결과를 보고할 수 있고, 작성자는 다른 사용자일 때만 점수를 얻는다. revoke/expiry credential은 인증 오류이며 익명으로 조용히 전환하지 않는다. 명시적 unlink 뒤에만 guest read를 재개한다. |

`CUST-01~06` 전체에서 같은 seed·URL·키워드 조합의 public hint ID, version, 순위가 MCP remote, MCP stdio, CLI, Arise 간 일치해야 한다. 인증 유무가 바꾸는 것은 receipt 및 쓰기 권한뿐이다. 미등록 host/route/task 키워드 조합은 잘못된 hint를 대신 반환하지 않고 빈 결과와 사유를 낸다.

## 4. 증거와 관련 케이스

각 run은 고객 상태(guest / web account only / host linked), host와 버전, 설치 템플릿 버전, auth 방식, 실제 recall tool-call argument와 server ingress의 redacted 요약, agent tool trace, fixture saved-oracle, receipt count, contribution/job count, score ledger delta를 저장한다. 원 URL/query 센티널이 tool-call argument에 들어가거나 ingress에 닿으면 해당 원격 경로는 개인정보 합격이 아니다. 비밀값·전체 URL·사용자 프롬프트는 evidence에 복사하지 않는다. CUST-01~06이 한 번이라도 건너뛰면 전체 고객 경로가 검증된 것으로 보고하지 않는다.

하위 컴포넌트 계약은 `WEB-02/09/16~19`, `INST-13~35/48`, `MCPF-05~17`, `CLI-07~13/23/27`, `SNIP-04~06`, `MCP-19~24`, `LINK-01~11`, `RCL-01~22`, `API-30~40`, `SCORE-15~24`, `DB-07~09/17/21`, `BEH-01/08~09`, `SMK-01~14`, `HOST-01~12`, `SEC-A/C/D/H`를 따른다. 오류·복구는 [`failure-matrix.md`](failure-matrix.md)의 해당 FAIL 케이스를 적용한다.

## 5. 출시 게이트

- CUST-01~02는 Claude Code와 Codex 각각에서 실행한다. CUST-02는 원격 MCP OAuth 자체를 검증하며, 지원하지 않는 host는 실패/미지원으로 기록한다. local stdio + device link는 CUST-04에서 별도로 통과해야 한다.
- CUST-03~04는 직접 CLI 및 로컬 stdio MCP 결과를 기록한다. 한 경로만 통과한 결과로 다른 형태까지 합격 처리하지 않는다.
- CUST-05~06은 Arise 실제 호스트의 MCP 허브와 browser 도구를 함께 실행한다. generic MCP 테스트를 Arise 통합 통과로 대체하지 않으며, WAM은 내부 Arise socket에 직접 연결하지 않는다. 세부 acceptance는 `HOST-10~12`와 [`../design/browser-host-integration.md`](../design/browser-host-integration.md)를 따른다.
- 공개 hint가 비회원도 이용 가능하고, 가입만 했지만 연결하지 않은 host가 익명으로 남으며, 링크한 회원만 receipt/write 권한을 얻는 세 경계가 모두 입증돼야 한다.
- Naver 유사 fixture 테스트는 로컬 픽스처에서만 수행한다. 실제 Naver 계정·게시물·Chrome 사용자 프로필은 사용하지 않는다.
