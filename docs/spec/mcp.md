# MCP: 웹 설치 가이드와 에이전트 연결

상태: 일부 구현. 로컬 stdio와 `POST /mcp` remote Streamable HTTP가 같은 initialize/tools/list/call dispatcher와 도구 catalog를 사용한다. Remote HTTP는 sessionless JSON response만 제공하며 64 KiB body limit, Accept/content-type 검증, supported protocol-version 확인, exact Origin allowlist를 적용한다. 마지막 focused remote MCP 확인은 `cargo test -p wam-server --test mcp` 3/3 통과였다. GET/SSE, OAuth discovery, production Gatekeeper publish/score, host 설치 검증은 미구현이다.

## 목표와 경계

사용자는 웹사이트의 공개 가이드에서 Claude Code, Codex 또는 범용 MCP 클라이언트를 선택하고 설정 문자열을 복사해 설치할 수 있어야 한다. 가입 없이 공개 메모리를 조회할 수 있다. 로그인·계정 생성은 기여·결과 보고·점수 귀속을 원할 때 한다. 이 MCP는 메모리 검색·상세 조회·기여 제출·영수증 결과 보고를 제공한다. ChromeBridge, Playwright, Chrome DevTools 같은 브라우저 MCP는 웹페이지를 조작하는 별도 서버다. 가이드가 두 서버를 같은 에이전트 환경에 연결하도록 설명하지만, 메모리 MCP가 Chrome이나 브라우저 MCP를 자동 설치했다고 표시하면 안 된다.

Arise는 자체 브라우저 MCP와 외부 MCP 허브를 제공한다. WAM MCP를 그 허브에 연결하면 Arise 브라우저 도구와 WAM 메모리 도구를 같은 agent session에서 쓸 수 있다. Chrome/Codex 같은 다른 host에서는 browser MCP와 Memory MCP를 각각 연결한다. 어떤 경로든 WAM 계정 인증은 브라우저 로그인과 별도이며 명시적으로 연결해야 한다.

### 원격 MCP URL 개인정보 경계

현재 local stdio MCP와 CLI는 공용 `wam-client`를 사용한다. client가 서버 요청을 만들기 전에 `wam-core` 규칙으로 HTTPS URL을 정규화하고 query·fragment와 민감 경로 조각을 제거하거나 거부한다. Remote MCP는 원격 host가 직접 WAM endpoint를 부르므로 client와 server ingress 양쪽에서 `wam-core` URL canonicalization을 적용한다. Remote MCP는 기존 bearer/session 인증과 scope를 재사용하지만 표준 MCP OAuth discovery는 아직 없다.

Remote host가 page URL을 먼저 model-visible tool argument로 만들 수 있으므로 URL query가 host process 밖으로 나가지 않는다고 보장하지 않는다. WAM server는 canonical URL만 처리하고 private/query-bearing URL을 보관하지 않는다. CUST/HOST 검증은 실제 tool-call argument와 server ingress를 검사한다. Arise 외부 MCP 허브도 모델이 원 URL을 조립해 전달하는 것만으로는 기기 내 query 제거를 보장하지 않는다. `/guide`는 검증되지 않은 host에 대해 query가 사용자 기기 밖으로 나가기 전에 제거된다고 표시하지 않는다.

### Chrome/Chromium: 브라우저 연결과 현재 탭 회수

WAM은 CDP 서버나 Chrome 제어 기능을 기본 설치물로 제공하지 않는다. 일반적인 고객 경로는 사용자가 선택한 Chrome browser MCP(예: ChromeBridge)를 별도로 연결하고 WAM MCP/CLI를 추가하는 것이다. 브라우저 MCP가 페이지 조작을 맡고 WAM MCP/CLI가 메모리 검색과 기여를 맡는다. 이미 browser MCP가 있으면 CDP를 또 설치할 필요가 없다.

현재 탭 URL을 자동으로 읽고 전송 전에 정규화하려면 로컬 WAM adapter가 선택한 browser connector의 지원 인터페이스에 연결해야 한다. 그 인터페이스는 connector가 제공하는 CDP, extension API, 다른 문서화된 local API일 수 있다. 선택한 connector가 안정적인 인터페이스를 제공하지 않을 때만 별도 local bridge를 고려한다. adapter는 기기 안에서 URL을 `wam-core` 규칙으로 정규화하고 키워드 allowlist와 task enum을 적용한 뒤 정제된 recall 요청만 서버로 보낸다. 모델에게 URL을 정규화하라고 지시하는 것만으로는 전송 전 개인정보 정규화를 보장하지 않는다.

외부 ChromeBridge가 자체 CDP 연결을 소유한다면 그 target/session을 WAM이 쓸 수 있다는 보장은 없다. 해당 connector의 문서화된 연결 방법을 확인하고 사용자의 브라우저 연결 승인을 받는다. 브라우저 제어 권한은 WAM 계정 로그인과 다르다. 직접 CDP endpoint를 쓰는 구현은 loopback으로 제한하고, 기존 개인 Chrome 프로필에 무단으로 붙지 않는다.

### Arise: 기존 MCP 허브와 앱 제어 채널

Arise의 `agent_browser --mcp-server`는 stdio MCP로 실행되고 앱이 소유한 로컬 제어 채널을 통해 앱 브라우저를 조작한다(Unix 계열은 Unix 도메인 소켓, Windows는 별도 local transport). Arise의 MCP 허브는 외부 MCP 서버를 연결·호출하므로 WAM은 우선 그 허브에 remote 또는 local WAM MCP로 등록한다. WAM이 Arise 내부 소켓에 직접 붙는 것은 지원 계약이 아니다. 현재 탭에 자동으로 결합된 recall은 Arise가 공개·버전 관리하는 host adapter가 필요하다.

이 저장소의 격리 Arise 테스트 하네스가 CDP를 사용한다는 사실은 Arise 제품 연동이 CDP라는 뜻이 아니다. `/guide`는 Chrome CDP connector, Arise MCP 허브, 직접 remote MCP 입력을 별도 경로로 설명한다.

호스트 연결 결정과 통합 전용 인수 테스트는 [`../design/browser-host-integration.md`](../design/browser-host-integration.md)에 모았다. MCP 도구·인증 계약은 이 문서, 실제 고객 여정은 `CUST-01~06`, host integration acceptance는 `HOST-01~12`가 기준이다.

## 사용자 설치 흐름

1. 사용자가 가입 없이 `/guide`를 열고 호스트와 형태(remote MCP, local stdio, CLI)를 선택한다. Arise는 MCP 허브 등록 절차를 선택한다.
2. 웹은 remote MCP 설정에는 배포 endpoint와 호스트별 템플릿을, CLI에는 지원 OS별 검증된 `wam` 다운로드/설치 명령과 checksum 검증 정보를 보여준다.
3. 화면은 복사할 명령 또는 설정 조각, 붙여넣을 위치, 필요한 재시작·재인증 절차를 구분해 보여준다. CLI 카드에는 `wam --version` 확인과 첫 익명 URL+키워드 recall 예시도 있다.
4. 복사 문자열에는 endpoint와 서버 이름만 포함한다. 계정 토큰, 장기 API 키, 개인 ID, 기여 nonce를 포함하지 않는다.
5. 사용자는 클라이언트 설정에 문자열을 붙여넣거나, 해당 호스트가 안전한 install deep link를 지원할 때만 공식 링크를 사용한다.
6. 첫 연결은 로그인 없이 공개 익명 읽기를 할 수 있다. 기여·결과 보고·사용 영수증이 필요한 사용자는 remote WAM MCP의 host OAuth 또는 local stdio WAM의 `wam link` device flow를 명시적으로 완료한다. Arise 계정·쿠키·브라우저 control socket이나 웹 홈페이지 로그인만으로 WAM host가 연결되지는 않는다.
7. 호스트에서 status 및 tools/list를 확인한 뒤 URL과 task를 넣어 recall을 시험한다.
8. 브라우저 MCP를 함께 사용하는 경우 browser tool과 memory tool이 모두 노출되는지 확인하고, fixture 사이트에서 실제 흐름을 수행한다.

복사 버튼은 클립보드에 문자열이 기록됐다고만 표시한다. 사용자의 Claude/Codex 로컬 설정을 실제 읽거나 설치 상태를 확인할 수 없으면 설치 완료라고 단정하지 않는다. 설치 상태는 사용자가 클라이언트에서 실행한 status 결과 또는 명시적인 OAuth 연결 신호로만 업데이트한다.

## 호스트별 템플릿

Remote Streamable HTTP가 기본 전송이다. 스니펫은 호스트별 템플릿 파일에서 생성되고 템플릿 버전과 최종 검증 날짜를 갖는다. Claude Code 예시는 공식 문서의 claude mcp add --transport http 형태를 따른다. Codex 예시는 공식 codex mcp add NAME --url URL 또는 Codex MCP 설정 형식에 맞춘다. 웹에서 배포 URL을 채우며 도메인을 소스에 하드코딩하지 않는다. [Claude Code MCP 문서](https://code.claude.com/docs/en/mcp)와 [Codex MCP 연결 예시](https://developers.openai.com/learn/docs-mcp)를 템플릿 갱신 시 확인한다.

설치 카드마다 다음을 제공한다.

- 호스트 이름과 지원 범위, 지원되는 transport
- 복사 명령 또는 붙여넣을 설정 파일의 위치
- 인증 방식과 브라우저 로그인 단계
- 필요한 권한: recall은 읽기, propose와 outcome report는 로그인 사용자 전용
- 브라우저 MCP를 별도로 연결하는 방법
- 도구 목록과 URL/task를 이용한 테스트 호출
- 삭제·재인증·연결 해지 방법

클라이언트가 OAuth를 지원하지 않으면 로컬 wam-cli가 device flow로 기기를 연결하는 방식만 허용한다. 이 경우 단기 device code만 브라우저 승인 화면에 표시하고, 발급된 토큰은 운영체제 보안 저장소에 둔다. 토큰을 셸 인자, 설정 조각, 로그, 웹페이지 HTML에 넣지 않는다.

## MCP 도구 계약

| 도구 | 입력 | 결과와 권한 |
|---|---|---|
| web_memory_status | 없음 | endpoint, auth 상태(`anonymous`, `linked`, `reauth_required`), consent 상태, catalog 버전. 비밀값·이메일·내부 user ID 없음 |
| web_memory_recall | canonical URL pattern, keywords?, task_category?, kind? | 원 주소가 아닌 query/userinfo 제거·동적 ID 치환을 마친 URL pattern을 받는다. 서버는 비정규 URL을 거부한다. 해당 범위의 승인된 L1 힌트 최대 3개, 적용 범위, 버전, `matched_on`, linked user의 경우 사용 영수증 ([`recall.md`](recall.md)) |
| web_memory_expand | memory_id, version_id, level (1 또는 2) | 해당 승인 버전의 L1 또는 L2 상세 절차와 검증 요약 ([`memory.md`](memory.md)) |
| web_memory_leaderboard | 없음 | 공개 opt-in 계정의 순위·handle·점수 합계 |
| web_memory_my_rank | 없음 | 인증 caller의 비공개 전체 순위와 opt-in 공개 순위; 인증 필요 |
| web_memory_propose | URL, 현재 task enum, 1~8개 `navigate/edit/save` action, `consent=true`, 동의 버전 | WAM client가 query/fragment를 제거한 뒤 로컬 파일 검수 큐에 전송한다. 페이지 원문·프롬프트는 보내지 않으며 `queued` 접수 ID를 반환한다. WAM_TOKEN 필요 |
| web_memory_my_contributions | 없음 | 본인 로컬 파일 접수 내역. WAM_TOKEN 필요 |
| web_memory_withdraw | contribution_id | 본인의 미게시 local 접수 철회. WAM_TOKEN 필요 |
| web_memory_outcome | receipt_id, success/partial/failed, used_version_ids? | receipt 소유자와 `report:outcome` scope 필요. 실제 사용한 receipt 소속 버전만 중복 없이 최대 3개 지정한다. 생략/빈 목록은 점수가 없다. 같은 결과·버전 집합(순서 무관)은 멱등 처리하고 변경은 거부한다. success는 지정 버전의 다른 계정 author에게만 +2 local score event를 기록하며 최초 보고일의 quota를 유지한다. production ledger/outbox는 미구현 |

익명 recall은 허용되지만 사용자별 영수증, outcome, 점수를 만들지 않는다. 기여 제출과 내역 조회·철회는 WAM_TOKEN 인증을 요구한다. 로컬 접수와 검수 상태는 `.wam-data/contributions.json`에 보존되며 웹의 별도 review token 화면에서 바뀐다. 수동 검수 승인은 production Gatekeeper publish와 연결되지 않지만, seeded memory에서 발급된 인증 receipt의 성공 outcome은 local score ledger에서 처리된다. 웹 계정이 있어도 host credential이 연결되기 전까지는 익명 호출이다. 인증된 recall의 영수증은 사용자·memory version·author·시간에 서버가 바인딩하고, 요청 payload에서 전달한 사용자 ID는 무시한다. 이미 제시된 credential이 만료·해지됐으면 인증 오류를 반환하고 익명 read로 조용히 낮추지 않는다. 사용자가 명시적으로 unlink한 뒤에는 공개 익명 연결을 새로 시작할 수 있다.

## 실패와 복구

오래된 템플릿, 잘못된 endpoint, TLS 오류, OAuth 거절, 권한 부족, tools/list 실패를 서로 구분해 표시한다. 네트워크 불가 시 승인된 로컬 캐시가 있으면 recall만 제한적으로 제공하고 contribution 및 점수 보고는 업로드 완료 전까지 대기 상태로 둔다. 캐시로 성공한 결과를 서버 확인 없이 새 점수로 기록하지 않는다.

## 요구사항과 테스트

MCP-16~24는 복사 문자열, client config parse, 익명/로그인 상태, 도구 발견, browser MCP 공존, URL/task recall, 갱신·해지까지 확인한다. CLI 설치 카드의 복사·설치·첫 recall은 `INST-48`로 확인한다. 고객 경로 교차 검증은 [`../test/customer-paths.md`](../test/customer-paths.md)의 `CUST-01~06`에서 Claude Code/Codex + ChromeBridge, CLI/local stdio, Arise를 각각 guest와 linked member 상태로 실행한다. 실제 호스트 통합은 `HOST-01~12`와 [`../design/browser-host-integration.md`](../design/browser-host-integration.md)도 따른다. SMK-01~14는 호스트 연결부터 fixture 작업 완료까지의 증거를 기록한다. 라이브 사이트에서 발행·결제하지 않고 로컬 fixture 또는 초안 저장만 사용한다.
