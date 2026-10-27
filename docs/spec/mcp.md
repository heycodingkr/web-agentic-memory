# MCP: 웹 설치 가이드와 에이전트 연결

상태: 1차 상세 명세. 운영 MCP 서버 및 고객별 설치 검증은 아직 없다.

## 목표와 경계

사용자는 웹사이트의 공개 가이드에서 Claude Code, Codex 또는 범용 MCP 클라이언트를 선택하고 설정 문자열을 복사해 설치할 수 있어야 한다. 가입 없이 공개 메모리를 조회할 수 있다. 로그인·계정 생성은 기여·결과 보고·점수 귀속을 원할 때 한다. 이 MCP는 메모리 검색·상세 조회·기여 제출을 제공한다. ChromeBridge, Playwright, Chrome DevTools 같은 브라우저 MCP는 웹페이지를 조작하는 별도 서버다. 가이드가 두 서버를 같은 에이전트 환경에 연결하도록 설명하지만, 메모리 MCP가 Chrome이나 브라우저 MCP를 자동 설치했다고 표시하면 안 된다.

Arise처럼 자체 브라우저 어댑터가 있는 호스트는 동일한 API와 계정에 연결해 자동 recall할 수 있다. 어댑터가 계정에 연결되지 않았으면 공개 익명 read로 동작하고, 계정 연결은 명시적으로 승인한다. Claude/Codex에서는 MCP 서버를 먼저 연결하고, 브라우저 도구도 별도로 활성화한다. 두 도구가 같이 보이는지, 사이트 작업 전에 메모리 recall이 호출되는지 확인해야 설치 성공이다.

### 원격 MCP URL 개인정보 경계

원격 MCP endpoint는 고객 브라우저의 현재 탭을 직접 읽지 않는다. Claude/Codex host가 브라우저 도구에서 얻은 주소를 바탕으로 `web_memory_recall` tool input을 만든다. 계약은 canonical URL pattern만 허용하고 서버는 비정규 입력을 거부하지만, host가 query나 동적 ID를 넣으면 그 값은 거부되기 전에 네트워크 요청에 실릴 수 있다. 따라서 서버 검증만으로 “원 URL이 기기 밖으로 나가지 않는다”고 보장할 수 없다.

Chrome 경로에서 이 보장을 하려면 지원 host 조합이 tool 호출 전에 안전하게 정규화한다는 실행 증거가 필요하다. CUST/HOST 테스트는 모델의 설명이 아니라 실제 tool-call argument와 서버 ingress를 검사한다. 해당 host에서 정규화를 입증하지 못하면 URL 개인정보 요구는 미통과이며, deterministic client normalizer가 있는 local stdio/CLI 또는 Arise adapter를 사용해야 한다. `/guide`는 검증되지 않은 host에 대해 query가 서버 전송 전에 제거된다고 표시하지 않는다.

## 사용자 설치 흐름

1. 사용자가 가입 없이 `/guide`를 열고 호스트와 형태(remote MCP, local stdio, CLI)를 선택한다.
2. 웹은 remote MCP 설정에는 배포 endpoint와 호스트별 템플릿을, CLI에는 지원 OS별 검증된 `wam` 다운로드/설치 명령과 checksum 검증 정보를 보여준다.
3. 화면은 복사할 명령 또는 설정 조각, 붙여넣을 위치, 필요한 재시작·재인증 절차를 구분해 보여준다. CLI 카드에는 `wam --version` 확인과 첫 익명 URL+키워드 recall 예시도 있다.
4. 복사 문자열에는 endpoint와 서버 이름만 포함한다. 계정 토큰, 장기 API 키, 개인 ID, 기여 nonce를 포함하지 않는다.
5. 사용자는 클라이언트 설정에 문자열을 붙여넣거나, 해당 호스트가 안전한 install deep link를 지원할 때만 공식 링크를 사용한다.
6. 첫 연결은 로그인 없이 공개 익명 읽기를 할 수 있다. 기여·결과 보고·사용 영수증이 필요한 사용자는 MCP host OAuth 또는 로컬/Arise device link를 명시적으로 완료한다. 웹 홈페이지에 로그인한 것만으로 host가 연결된 것은 아니다.
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
| web_memory_propose | 정규화 URL, task_category, trace, outcome, consent | 로그인·동의가 확인되면 기여 접수 ID와 상태 링크 반환 |
| web_memory_outcome | receipt_id, success/partial/failed | 영수증 소유 계정의 결과 1회 기록. 성공만 점수 후보가 됨 |

익명 recall은 허용되지만 사용자별 영수증을 발급하거나 결과 보고·기여 제출·점수 발생 기능을 허용하지 않는다. 웹 계정이 있어도 host가 연결되기 전까지는 익명 호출이다. 인증된 recall의 영수증은 사용자·memory version·시간·nonce에 서버가 바인딩하고, 요청 payload에서 전달한 사용자 ID는 무시한다. 이미 제시된 credential이 만료·해지됐으면 인증 오류를 반환하고 익명 read로 조용히 낮추지 않는다. 사용자가 명시적으로 unlink한 뒤에는 공개 익명 연결을 새로 시작할 수 있다.

## 실패와 복구

오래된 템플릿, 잘못된 endpoint, TLS 오류, OAuth 거절, 권한 부족, tools/list 실패를 서로 구분해 표시한다. 네트워크 불가 시 승인된 로컬 캐시가 있으면 recall만 제한적으로 제공하고 contribution 및 점수 보고는 업로드 완료 전까지 대기 상태로 둔다. 캐시로 성공한 결과를 서버 확인 없이 새 점수로 기록하지 않는다.

## 요구사항과 테스트

MCP-16~24는 복사 문자열, client config parse, 익명/로그인 상태, 도구 발견, browser MCP 공존, URL/task recall, 갱신·해지까지 확인한다. CLI 설치 카드의 복사·설치·첫 recall은 `INST-48`로 확인한다. 고객 경로 교차 검증은 [`../test/customer-paths.md`](../test/customer-paths.md)의 `CUST-01~06`에서 Claude Code/Codex + ChromeBridge, CLI/local stdio, Arise를 각각 guest와 linked member 상태로 실행한다. SMK-01~14는 호스트 연결부터 fixture 작업 완료까지의 증거를 기록한다. 라이브 사이트에서 발행·결제하지 않고 로컬 fixture 또는 초안 저장만 사용한다.
