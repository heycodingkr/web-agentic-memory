# 기능 인수 테스트: 웹 설치에서 기여 점수까지

상태: 통합 사용자 여정의 수용 기준. 이메일 로그인·기여/검수·durable publication 경로의 일부 구현과 자동 검증이 있지만, 아래 조건 전체의 통과나 운영 수용을 뜻하지 않는다. 공개 환경 결과는 [email-auth-production.md](email-auth-production.md)에, 내 기여 결과 상세는 [my-page.md](my-page.md)에 별도로 기록한다.

각 성공 경로의 실패·복구 주입은 [`failure-matrix.md`](failure-matrix.md)의 교차 계층 케이스와 함께 수행한다. 정상 경로에서 에러를 반환하는지뿐 아니라, 접수된 상태의 유실·중복·오인 성공도 확인한다.

## 1. 목표와 공통 환경

이 테스트 묶음은 웹 가이드에서 MCP 설정을 복사한 사용자가 Claude Code 또는 Codex에 메모리 MCP를 연결하고, Chrome 계열 브라우저 도구와 함께 실제 웹 fixture를 조작하는 흐름을 확인한다. 비회원, 웹 계정만 있고 host 미연결인 사용자, host까지 연결한 회원을 ChromeBridge/browser MCP, CLI/local stdio, Arise 경로에서 각각 확인한다. 사이트 로그인, 기여자 계정 귀속, Gatekeeper Agent의 URL/task별 판정, 버전 갱신, 로그인 사용자 성공 사용에 대한 점수, 익명 사용 무점수까지 서버와 DB 증거로 확인한다. 상세 실행 표는 [`customer-paths.md`](customer-paths.md)를 따른다.

테스트 서버와 웹앱은 loopback 또는 별도 staging 계정에서 실행하고, OAuth는 fake provider를 우선 사용한다. 브라우저는 공개 사이트에 발행·결제하지 않고 로컬 fixture에 초안 저장만 한다. Claude/Codex의 일반 사용 프로필과 비밀 정보가 든 사용자 설정은 건드리지 않고, 일회용 host profile과 합성 계정만 쓴다. Chrome browser MCP는 페이지 조작 도구이고 Web Agentic Memory MCP는 recall/propose/outcome 도구다. 둘은 별도 서버로 설치되며, 테스트는 두 서버가 한 에이전트 세션에서 공존하는지만 확인한다.

## 2. 웹 UI와 로그인 인수 기준

### ACC-WEB-01 — 방문, 가입/로그인, 인증 후 목적지와 소유권

1. 로그아웃 상태로 홈, `/memories`, `/guide`, `/leaderboard`를 연다. 공개 읽기와 계정 전용 동작을 구분한다.
2. 합성 이메일 A로 첫 인증을 한다. 자동화에서는 fake mailer로 OTP를 읽고, 실제 운영에서는 소유한 승인 QA 메일함만 쓴다. 오답/만료만으로 계정이 생기지 않고 첫 성공 검증 때 하나 생성되는지 확인한다.
3. 같은 이메일을 다시 인증해 동일 account/profile/score로 로그인한다. 설정된 경우 GitHub OAuth는 별도 provider로 검사하고, 다른 이메일/provider 계정과 자동 병합되지 않는지 확인한다.
4. 일반 로그인 성공은 `/dashboard`로 이동한다. `/link` 또는 MCP 승인 중 시작했다면 허용된 내부 요청으로 돌아와 권한 범위를 보여주고 명시적 승인 전에는 device/app token을 발급하지 않는다. 외부 return URL은 거부한다.
5. 브라우저 세션의 CSRF·만료·로그아웃을 확인한다. 로그아웃 후 개인 API/화면은 차단하되 별도 기기 token 상태를 문서화된 정책대로 확인한다.
6. 사용자 B의 세션으로 A의 contribution UUID, receipt ID, score cursor와 owner analytics를 직접 조회한다.

**합격:** public/read와 account/write가 분리되고 신규/기존 로그인은 같은 계정에 연결된다. A는 자기 contribution과 score ledger만 본다. B에게 A의 데이터는 404 또는 동등한 비식별 권한 오류다. 이메일, token, private trace가 화면·HTML source·일반 로그에 노출되지 않는다. 실제 OTP 수신/운영 화면 증거는 [EMAIL-PROD-01~34](email-auth-production.md), 목적지·owner dashboard는 [MYPAGE-01~06](my-page.md)와 맞아야 한다. 관련 기준은 `F-EMAIL-01~07`, `WEB-02`, `WEB-05`, `WEB-16`, `WEB-18~19`, `API-32~34`, `SEC-D`다.

### ACC-WEB-02 — 기여 동의, 전송 미리보기와 실제 접수 상태

1. 처음 가입한 A 계정에서 공개 공유 동의가 제출 버튼과 별개이며 기본 off인지 확인한다.
2. `/contribute`에서 fixture URL과 `compose-draft` task를 선택한다. 화면에는 query/fragment 없는 정규화 URL preview와 전송 필드가 보인다.
3. 동의하지 않고 submit한다. client 요청과 서버 queue entry 모두 0이어야 한다.
4. 미리보기 후 명시적으로 동의하고 한 번 제출한다. body에 prompt, 페이지 본문, DOM selector, screenshot, cookie, 원 URL query, client-supplied consumer/author ID가 없는지 확인한다.
5. 첫 owner 상태는 접수/대기(`queued`)다. 처리 근거가 있을 때만 `processing`을 보이고, 실제 retry·추가 검토·동의 필요·거절·게시·support 결과를 [My Page 상태 계약](../spec/my-page.md)에 따라 구분한다. `approved`나 HTTP 접수 성공을 게시 완료/점수 지급으로 표현하지 않는다.
6. 별도 reviewer 권한으로 claim/decision을 시도한다. 검증된 replay proposal 없이 승인하면 worker 검증을 요청할 뿐이며 public catalog나 점수를 바꾸지 않는지 확인한다. 철회와 동시 worker 처리는 generation/consent fence로 판정한다.
7. A의 private progress는 B에게 가려지고, A의 철회 뒤 payload와 pending job은 retention policy대로 차단/정리된다.

**합격:** contributor는 서버 세션의 A로 귀속되고, consent 전 요청은 0건이다. 제출 직후에는 접수 상태만 보이며 published/memory/version link/+20은 실제 worker 결과가 있을 때만 나타난다. POST receipt를 받은 뒤 목록 refresh가 실패해도 접수 성공을 제출 실패로 오인시키지 않고, 재시도는 GET만 수행한다. review approval 문자열만으로 공개 상태를 추정하지 않는다. 관련 기준은 `WEB-17~22`, `F-QUE-01~10`, `MYPAGE-07~19`, `PRIV-01~08`, `API-20~25`, `DB-02~04`, `SEC-A`다.

### ACC-WEB-03 — 이메일 가입부터 검수·게시·사용 반영까지

1. 격리 자동화 환경에서 A가 이메일 OTP로 가입/로그인하고 `/dashboard`에 도착한다. 운영 수용은 소유한 승인 QA 메일함으로만 실행한다.
2. A가 공개 공유에 동의하고 고정 URL pattern/task/action fixture를 제출한다. 새 receipt와 queued work가 A에 귀속되고, 제출 직후 public catalog와 A의 score는 변하지 않는다.
3. 독립 trusted replay 검증을 통과한 신규 workflow를 처리한다. 그 다음 별도 fixture로 개선 version, 기존 memory support, replay 실패/저신뢰 추가검토, consent 철회 경로를 각각 처리한다. `review approved`만 있는 legacy record도 별도로 둔다.
4. 게시 케이스에서 memory/version, author attribution, `Published` decision, +20 score event, outbox가 한 번만 연결되는지 확인한다. 개선은 새 immutable version으로 연결하고 support/reject/retry/needs-review/legacy approved는 서로 다른 결과로 남는다.
5. A가 `/dashboard` 또는 My Page를 새로고침한다. 접수별 현재 상태/안전한 대기 이유/처리 시각과 실제 결과 memory/version을 확인한다. published item은 `/memories` 및 public recall에 나오고, support는 기존 memory 링크를 쓰며 신규 게시/+20으로 오인하지 않는다.
6. 별도 사용자 B가 해당 version을 recall하고 실제 작업을 완료한 뒤 `used_version_ids`가 있는 성공 outcome을 명시적으로 한 번 보고한다. A의 author ledger에 적격 +2가 한 번만 기록되는지 확인한다. A의 self-use, anonymous call, 미사용 후보, 실패/중복 보고는 점수를 만들지 않는다.
7. 같은 요청·review decision/outcome을 재전송하고 API/DB 재시작 후 확인한다. 결과 링크, 공개 상태, attribution, 원장 사이 불일치나 중복 게시/점수는 없어야 한다.
8. 기여 POST는 receipt를 반환하게 하고 바로 뒤의 목록 GET은 한 번 503을 반환하도록 주입한다. 화면은 확인된 receipt와 접수 상태를 유지하고 목록 갱신 실패를 구분해 안내한다. Refresh를 누르면 GET만 다시 보내고 POST는 한 번만 전송됐는지 확인한다.

**합격:** 결과 판정의 근거는 owner API·worker decision/audit·catalog/version·attribution·score ledger·public recall이 모두 일치해야 한다. worker/replay/provider가 꺼져 있으면 접수는 pending으로 남고 운영 end-to-end는 fail/blocked이며 fake replay로 통과 처리하지 않는다. `approved` 문자열을 게시·기여자 크레딧의 증거로 쓰지 않는다. 로컬 자동/DB 범위는 `application_lifecycle.rs`와 `postgres_application.rs`, 운영 범위는 [EMAIL-PROD](email-auth-production.md), [MYPAGE-07~40](my-page.md), 배포 worker 상태는 이슈 #159를 함께 확인한다. 관련 기준은 `F-EMAIL-01~07`, `F-QUE-01~16`, `GK-30~48`, `SCORE-01~24`, `MYPAGE-07~19`, `MYPAGE-25`, `MYPAGE-39~40`, `DB-10~25`, `PRIV-01~08`이다.

#### 로컬 실행 기록 — 2026-11-01

`pnpm --dir web run test:smoke` 1/1 통과. 별도 빈 temp 디렉터리와 loopback API/web 서버에서 synthetic email OTP로 신규 계정을 만들고, 공개 기여 동의→queued receipt→fixture worker/replay→게시 memory/version→public recall→개선 version→점수 원장→철회/계정 삭제를 확인했다. OTP는 smoke fixture의 fake sender에만 전달되고, 별도 keyed loopback test endpoint로만 브라우저 검사가 읽는다. 이메일 API 응답에는 OTP가 없다. worker/replay 증거는 의도적으로 고정된 fixture다. Support/reject/wait/owner isolation 및 다른 계정의 +2는 `cargo test --locked --workspace`의 durable lifecycle 검사에서 별도로 확인했다. 이메일 브라우저의 화면·오답·재전송·로그아웃 검사는 fake API 기반 desktop/mobile 20/20이다. 이 조합은 분리된 로컬 합성 경로이며 실제 운영 inbox, 배포 worker, live provider/replay, production DB의 ACC-WEB-03 전체 수용을 뜻하지 않는다.

### ACC-ADMIN-01 — 관리자 가입자·기여 검수와 Muse 게시

1. 합성 allowlisted admin email과 비허용 email을 준비한다. fake email adapter로 허용 주소만 실제 링크를 받고 두 주소의 시작 응답이 구분되지 않는지 확인한다.
2. 허용 링크를 같은 브라우저에서 한 번 교환한다. 다른 브라우저·만료·재사용 링크는 실패하고 session cookie 속성/2시간 TTL을 확인한다.
3. 관리자 계정 목록에서 가입 시각·인증 여부·기여 건수를 확인하고 목록 마스킹, 상세 권한, cursor 이동을 검사한다. 계정/기여 API 응답과 HTML에 token·secret·raw IP가 없어야 한다.
4. synthetic contribution을 선택해 요청한 canonical 웹 URL, task/action sequence, memory proposal, replay 근거, 현재 version 및 대기 사유가 함께 보이는지 확인한다.
5. test Muse Gateway provider로 중복, 새 후보, 개선 후보, 모호 후보, malformed response, provider outage를 각각 보낸다. metadata의 provider/model ID를 요청 fixture와 대조한다.
6. replay 전, replay 실패, schema/scope/version mismatch에서는 public catalog와 score ledger가 바뀌지 않는지 확인한다. valid candidate와 replay가 통과한 건만 관리자가 게시하고 active immutable version, contribution reflection, audit/outbox가 같은 transaction으로 기록되는지 확인한다.
7. decision을 stale generation으로 재전송하고 동시 double click/worker retry를 만든다. stale 요청은 conflict, 동일 요청 재시도는 멱등이며 공개 version/점수 중복은 없어야 한다.
8. reject, duplicate/support, needs-human-review, provider outage 요청을 검색해 상태·reason이 보이고 새 public version이 없는지 확인한다. 승인된 synthetic 기여자의 My Page에서 실제 게시 memory/version link와 정책상 점수가 1회만 보이는지 확인한다.
9. 로그아웃 후 새로고침·보호 API·두 번째 탭으로 접근한다. session과 page data가 복구되지 않아야 한다. login, list, detail, pending, published 화면을 개인정보 없는 합성 fixture로 캡처한다.

**합격:** 실제 관리자 이메일 이외에는 로그인 메일이 보내지지 않고 시작 응답은 주소 존재 여부를 노출하지 않는다. URL/task를 통해 요청 의도를 확인할 수 있으며, Muse 결과는 서버 검증/replay를 거쳐서만 public version이 된다. 게시·reflection·score/audit은 원자성·멱등성을 유지한다. fake provider 테스트는 계약 통과를 증명할 뿐 실제 production email 및 Muse 연결을 대체하지 않는다. Production pass에는 실제 허용 사서함의 버튼 수신/사용, 배포가 지정한 Muse Gateway model ID의 decision, trusted worker replay, 공개 catalog/version과 My Page 반영까지 end-to-end 증거가 필요하다. 관련 기준은 `F-ADMIN-01~10`, `ADMIN-01~32`, `SEC-D15~21`, `CUR-17`, `MODEL-09`, `ACC-WEB-03`이다.

### ACC-PRIV-01 — 계정 삭제와 기여 귀속 철회

1. 합성 계정 A에 로그인 세션과 연결 MCP device를 만들고, pending contribution 하나와 A 단독 근거 메모리, A와 B의 독립 근거가 있는 메모리를 준비한다.
2. 설정 화면에서 계정 삭제와 attribution 철회를 요청하고 확인 단계를 완료한다.
3. 기존 세션과 device로 dashboard, propose, outcome을 호출해 접근이 차단되는지 확인한다.
4. pending payload, contribution attribution, published record evidence를 확인하고 Gatekeeper 재평가 결과를 기다린다.
5. 공개 catalog/recall, leaderboard, audit sink에서 A의 신원과 삭제된 자료가 남았는지 검사한다.

**합격:** 세션과 device가 해지되고 private dashboard 및 새 write 요청이 차단된다. pending payload와 계정 연결 데이터는 보존 정책에 따라 삭제/비식별화된다. B의 독립 근거가 유효한 공유 메모리만 유지할 수 있고, A 단독 근거 메모리는 재검증 또는 retire 전까지 recall에서 숨긴다. 법률상 보존이 필요한 경우를 제외한 공개 응답·로그에는 A를 식별할 정보가 없으며 최소 감사 근거만 남는다. 관련 기준은 `PRIV-09`, `FLOW-07`, `WEB-22`, `LINK-07`, `API-38`, `DB-13~14`, `SEC-G`다.

## 3. MCP 웹 가이드 및 브라우저 실행 인수 기준

### ACC-MCP-01 — 복사에서 tools/list까지

1. `/guide`를 열어 `Claude Code`, `Codex CLI` 중 한 호스트를 선택하고, 호스트 이름·지원 transport·config template version·현재 MCP endpoint·인증 방식이 표시되는지 확인한다.
2. 복사 버튼을 누르고 clipboard 내용을 템플릿과 비교한다. 문자열에는 endpoint와 서버 이름만 포함하며 access token, API secret, 계정 ID, device code, contribution nonce는 없어야 한다.
3. UI 상태가 “복사됨”인지 확인한다. host 설치를 확인하지 않은 상태에서 “연결됨/설치 완료”로 표시되면 실패다.
4. 임시 Claude/Codex host profile에서 복사 명령/config를 등록하고 MCP 목록을 새로고침한다. 연결이 안 되면 UI는 transport, OAuth, endpoint, tools/list 중 실패한 단계를 구분해 안내한다.
5. guest, 웹 계정만 로그인했지만 host가 미연결인 상태, host 연결 완료 상태를 각각 확인한다. `web_memory_status`는 `anonymous`/`linked` 상태와 권한만 보여주며 이메일·내부 ID·token 값을 반환하지 않는다.

**합격:** copy text가 호스트별 문법으로 parse되고 remote MCP endpoint에 연결된다. 웹 복사 이벤트는 실제 기기 설치와 구분된다. 환경별로 설치 확인 신호가 없으면 연결 완료를 거짓 표시하지 않는다. 호스트 명령은 구현 시점에 공식 문서에서 확인해 기록한다. 관련 기준은 `WEB-09`, `MCP-16~19`, `SMK-01~05`, `SEC-B`다.

### ACC-MCP-02 — Chrome browser MCP와 메모리 MCP 동시 사용

1. 테스트 호스트에 Web Agentic Memory MCP를 추가한다.
2. 별도 Chrome/Playwright/Chrome DevTools browser MCP를 같은 에이전트 세션에 추가한다.
3. `tools/list`와 세션 trace에서 memory 도구와 browser 도구의 이름·server origin을 구분한다.
4. 로컬 editor fixture의 현재 URL과 task로 `web_memory_recall`을 호출한다. 성공 memory가 있는 경우 정확한 route/task hint가 돌아오는지, 없는 경우 empty result인지 확인한다.
5. 에이전트에게 fixture 초안 저장을 요청하되 작업 프롬프트에서 “메모리를 조회하라”고 직접 지시하지 않는다. MCP instructions가 recall을 유도하고, 이후 browser MCP가 편집·save를 수행하는지 확인한다.
6. fixture oracle을 새 페이지에서 읽어 title/body가 일치하는지 확인한다. 게시/공유 행동은 실행하지 않는다.
7. 사용자가 동의한 경우에만 propose를 호출하고, 생성된 queue ID를 본인 dashboard에서 확인한다.

**합격:** 두 MCP의 기능이 공존하며 browser MCP가 메모리 서버를 대신하거나 반대 경우가 없다. 정확한 URL pattern과 task에 맞는 승인 메모리만 주입된다. Agent의 작업 전 recall 호출은 trace에 남고, 초안 결과는 독립 oracle에서 확인된다. “설정 복사”만으로 browser driver가 설치되었다고 보고하지 않는다. 관련 기준은 `MCP-01~06`, `MCP-20~24`, `BEH-01`, `FLOW-01~03`, `SMK-02~06`, `SMK-01~14`이다.

### ACC-MCP-03 — 익명과 로그인 도구 권한

1. 인증 정보 없이 public memory recall을 수행한다.
2. 반환된 힌트가 승인/활성 version에 한정되는지 확인한다. user-specific receipt, account identifier, success-report 가능한 handle이 없어야 한다.
3. 같은 익명 연결에서 `propose`, `web_memory_outcome`을 호출한다.
4. OAuth 로그인 후 같은 도구를 호출하고, 명시적 consent가 없는 상태와 있는 상태를 각각 비교한다.

**합격:** 익명 recall은 정책상 허용될 수 있지만 user-level receipt, contribution, outcome, points는 만들지 않는다. 인증된 사용자도 consent 없이 기여할 수 없다. 사용자 ID는 MCP argument가 아니라 server session/device token에서 가져온다. 관련 기준은 `MCP-22~23`, `API-30~38`, `SCORE-15~20`, `DB-07~09`다.

### ACC-MCP-04 — URL + 키워드 recall로 작업 시작

1. fixture 사이트에 같은 host의 레코드 3개를 시드한다: `/editor/post/:id`의 `compose-draft` workflow(키워드 `draft`, `초안`), 같은 host의 `site_note`(화면 유형과 `iframe-editor` trait), 다른 route의 workflow.
2. 테스트 호스트(Claude Code 또는 Codex)에 메모리 MCP와 Chrome browser MCP를 연결하고, 사용자가 한국어로 '이 에디터에 초안 저장해줘'만 요청한다. recall이나 URL·키워드를 지시하지 않는다.
3. trace에서 recall 호출 인수를 확인한다: canonical URL pattern과 어휘 안의 키워드만 있고 원본 URL, 쿼리, 프롬프트 문장은 없어야 한다.
4. 반환 결과를 확인한다: 정확 route의 L1이 1순위이고 `site_note`가 함께 올 수 있다. 최대 3개, 힌트당 ≤400 tokens, 다른 host 항목 0건.
5. L1만으로 충분한 작업에서는 `expand`가 호출되지 않고, 힌트가 부족하거나 틀린 작업에서는 `expand(level=2)` 뒤에 진행하는지 확인한다.
6. fixture oracle로 초안 저장 결과를 확인한다. 같은 세션에서 같은 route를 다시 열면 5분 안에 재호출하지 않고, 다른 route 패턴으로 이동하면 다시 호출하는지 trace로 확인한다.
7. 같은 질의를 `wam recall <url> 키워드…`로 실행해 MCP 결과와 같은지 비교한다.

**합격:** 서버로 나간 것은 안전 URL 패턴과 어휘 키워드뿐이다. 순위는 route, task, 키워드 순으로 결정적이며 교차 host가 없다. 호출 시점 규칙이 trace에 나타난다. 모듈 경계 parity와 고정 label corpus는 `RCL-17~22`, 성능·구조 검증은 [`retrieval-benchmark.md`](retrieval-benchmark.md)에서 확인한다. 관련 기준은 `RCL-01~22`, `MEM-04~05`, `MCP-20~24`, `BEH-01`, `BEH-08~09`, `SMK-06`, `HOST-03~04`다.

### ACC-MCP-05 — 고객 가입 상태 × host 경로 매트릭스

[`customer-paths.md`](customer-paths.md)의 `CUST-01~06`을 각각 독립 run으로 실행한다.

1. 비회원이 홈페이지와 `/guide`를 열어 Claude Code 또는 Codex 템플릿을 복사하고, 일회용 host profile에 Memory MCP를 등록한다. ChromeBridge/browser MCP도 별도로 등록한다.
2. 알려진 fixture에 한국어 작업을 요청하고 작업 전 recall, browser action, 독립 저장 oracle을 검사한다. URL 패턴과 `naver`, `blog`, `write` 키워드로 정확한 메모리가 선택되는지 확인한다.
3. 새 웹 계정을 만들거나 로그인하되 MCP/Arise host를 연결하지 않은 상태에서 같은 요청을 반복한다. host가 계속 anonymous로 판정하고 receipt/write를 허용하지 않는지 검사한다.
4. 각 지원 경로에 맞춰 host OAuth 또는 device link를 명시적으로 승인하고 반복한다. status의 연결 상태와 receipt 소유권이 바뀌되 공개 메모리의 ID·version·순위는 그대로인지 확인한다.
5. 비회원과 linked member에 대해 직접 CLI, local stdio MCP, Arise MCP 허브 통합을 별도 실행한다. Arise host-native adapter가 구현되면 그것도 별도 경로로 추가한다. generic MCP 성공을 Arise/CLI 성공으로 대체하지 않는다.
6. guest의 propose/outcome, 다른 계정의 receipt 사용, self-use, 만료·해지 credential, receipt 재전송을 시도하고 API/DB/score ledger를 비교한다.

**합격:** 비회원은 가입 강요 없이 public read와 fixture 작업을 끝내며 user-level receipt, write, score가 0이다. 웹 계정만 만든 host도 익명이다. 명시적 host link 뒤에는 동일 user에 묶인 receipt만 허용하고, 성공·중복방지 규칙대로 메모리 작성자 점수만 변한다. 만료·해지 자격증명은 익명으로 조용히 downgrade하지 않는다. Chrome, CLI, Arise 간 결과는 같고 인증 차이는 receipt·권한에만 나타난다. 관련 기준은 `CUST-01~06`, `HOST-01~12`, `LINK-01~11`, `MCPF-05~17`, `RCL-01~22`, `SCORE-15~24`, `FAIL-04/21/28`이다.

## 4. 서버, Gatekeeper, URL/task 판정 인수 기준

### ACC-SERVER-01 — 중복/신규/갱신/폐기

1. catalog에 URL pattern `/editor/post/:id`, task `compose-draft`, version v1을 seed한다.
2. 로그인 사용자 A가 같은 pattern/task에 같은 성공 절차를 기여한다. gatekeeper는 기존 항목과 비교한다.
3. 같은 절차를 단어 순서나 도구 이름만 바꿔 다시 제출한다. canonical action sequence와 fingerprint 기준으로 중복 또는 support merge가 되는지 확인한다.
4. 기존 v1이 실패하는 변경 UI fingerprint를 넣고, 대체 candidate v2는 결정적 검사와 격리 replay를 통과하도록 한다.
5. gatekeeper decision, reason code, policy version, confidence band, evidence digest, memory version relation을 조회한다.
6. confidence 미달 및 충돌 입력을 제출해 사람 검토 예외 큐로 이동하는지 확인한다.
7. retire/rollback 실행 뒤 public recall, version table, audit log를 확인한다.

**합격:** 정상 케이스는 Gatekeeper Agent가 `duplicate_support`, `create_new`, `supersede`, `retire` 중 최종 결정을 내린다. 모든 제출에 관리자 사전 승인을 요구하지 않는다. 다만 결정적 secret/consent/scope 검사는 agent 판단보다 우선하며, 낮은 confidence나 상충 증거는 public state를 바꾸지 않고 사람 큐로 보낸다. hard delete 대신 soft retirement와 immutable version을 쓴다. 관련 기준은 `GK-39~48`, `DEDUP-01~18`, `DB-05~06`, `FLOW-04~05`, `SEC-B`, `SEC-C`다.

### ACC-MODEL-01 — Meta 모델로 메모리 판정 후보 생성

1. disposable environment에서 검증된 Meta Model API 또는 Meta open-weight endpoint를 선택하고 provider/model ID와 schema version을 run metadata에 기록한다.
2. 로그인 사용자 A의 안전 정규화 contribution과 같은 URL/task의 기존 active memory summary를 준비한다.
3. Gatekeeper가 URL/task 중복 또는 stale/개선 후보를 Meta model에 보내고 structured decision candidate를 받는다.
4. 요청 전송 trace와 model response를 schema 검사한다. trace에는 모델 식별자와 digest가 있지만 원 URL, query, prompt, page body, user input, secret, chain-of-thought는 없어야 한다.
5. 모델 응답이 `duplicate_support`, `create_new`, `supersede`, `retire`, `needs_human_review` 중 하나인지 확인하고 deterministic security checks 및 replay evidence와 대조한다.
6. timeout, quota/HTTP error, invalid JSON, prompt injection이 섞인 output을 각각 주입한다.

**합격:** 실제 demo run에서 Meta provider가 decision candidate 생성에 사용되고 model ID/version이 추적 가능하다. Gatekeeper policy engine만 최종 persistence/publication을 수행한다. model failure나 비정상 응답은 catalog, active pointer, score ledger를 바꾸지 않는다. fake provider는 contract/CI test에 쓸 수 있지만 실제 Meta 연결 acceptance를 대신하지 않는다. 관련 기준은 `MODEL-01~10`, `GK-44~48`, `DB-11`, `DB-15~16`, `SEC-A`, `SEC-B`다.

### ACC-SERVER-02 — URL 범위와 최신성

1. `/editor/post/123?token=fake#body`, `/search?q=private`, `https://user:pass@example.test/...`를 각기 recall/propose한다.
2. 정규화 key, database dump, API response, reverse proxy/access log, tracing sink를 검색한다.
3. `/editor/post/:id`와 `/search`의 동일 task, 그리고 동일 route의 `search`/`compose-draft` task를 각각 seed한다.
4. 각 경우를 정확 route, safe parent route, host-wide fallback 순서로 조회한다.
5. old `last_verified_at`, 증가한 failure signals, 새 UI fingerprint, replay 결과가 각각 다를 때 Gatekeeper lifecycle decision을 비교한다.

**합격:** query·fragment·userinfo·tracking은 제거되고 dynamic ID/account path는 안전한 template로 바뀐다. path/task가 다르면 자동 병합하지 않는다. URL이 불분명하면 no-result 또는 quarantine이고 원문 URL 저장으로 후퇴하지 않는다. 나이만으로 메모리를 바로 삭제하지 않으며, freshness/replay evidence로 `needs-reverify`, `supersede`, `retire`를 구분한다. 관련 기준은 `CORE-22~26`, `DEDUP-13~18`, `API-04`, `DB-03`, `DB-15`, `SEC-A11`이다.

### ACC-SERVER-03 — Muse 큐레이터의 도구 루프

1. `/editor/post/:id`, `compose-draft`, v1이 active인 catalog를 seed한다.
2. 같은 절차를 기여한다. 모델이 `job_get`, `memory_search`, `memory_compare` 뒤에 `decide_skip`(`duplicate_support`)을 부르고 새 레코드가 생기지 않는지 확인한다.
3. 더 짧고 성공률이 같은 절차를 독립 기여자 2명이 제출한다. `decide_replace`로 새 version이 active가 되고 v1이 superseded이며 recall이 v2를 돌려주는지 확인한다.
4. 레코드가 없는 다른 route를 기여해 `decide_create`가 첫 version을 만드는지 확인한다.
5. 상충하거나 확신이 낮은 기여를 제출해 `decide_escalate`로 예외 큐에만 쌓이고 공개 상태가 그대로인지 확인한다.
6. fake provider로 불변조건 위반을 주입한다: 이미 있는 scope에 create, 다른 host 접근, 결정 도구 2회, 오래된 `expected_version`, 위험 문장이 든 레코드. 모두 오류 코드로 거부되고 상태가 바뀌지 않아야 한다.
7. 실제 Meta/Muse provider로 3번과 4번 중 하나 이상을 수행하고, audit에 provider·model ID, 지침·도구 버전 해시, 입력·출력 digest가 있는지 확인한다.

**합격:** 모델은 도구로만 행동하고 모든 상태 변경은 서버가 검증한 결정 도구에서만 일어난다. 불변조건은 모델 출력과 무관하게 유지된다. 실제 provider가 한 번 이상 판정에 쓰였다. 관련 기준은 `CUR-01~19`, `MODEL-01~10`, `GK-39~48`, `DEDUP-13~18`, `SEC-I`다.

### ACC-SERVER-04 — 기여 폭주와 큐

1. 같은 scope에 기여 50건과 서로 다른 scope에 기여 50건을 10초 안에 접수한다.
2. 큐 깊이와 가장 오래된 job의 나이 지표를 확인한다. 같은 scope의 active version은 1개이고 모델 호출 수가 같은 scope 기여 수보다 적은지(병합) 확인한다.
3. 한 기기가 상한 이상으로 폭주하는 동안 다른 사용자의 기여가 계속 처리되는지 확인한다.
4. 깊이 상한 초과를 만들어 429와 `Retry-After`를 확인하고, 이미 202를 받은 job이 모두 남아 있는지 확인한다.
5. 모델 429, 장애, 예산 소진을 주입한다. job이 `retry` 또는 `waiting_budget`에 남고 카탈로그가 바뀌지 않으며, 복구 뒤 처리가 재개되는지 확인한다.
6. worker를 lease 중 강제 종료한다. lease 만료 뒤 재처리되고 중복 게시나 중복 점수가 없는지 확인한다.
7. 대기 중인 기여를 철회해 job이 취소되고 payload가 지워지는지 확인한다.

**합격:** 202를 받은 job은 유실되지 않고 같은 scope에 active 메모리가 중복되지 않는다. 모델 장애와 예산 소진은 공개 카탈로그와 recall에 영향을 주지 않는다. 폭주 사용자가 다른 사용자를 굶기지 않는다. 관련 기준은 `QUE-01~18`, `DB-10~11`, `DB-22~24`, `GK-37~38`, `PRIV-03`, `MODEL-05~06`, `SEC-I03`이다.

## 5. 데이터베이스, 점수, 리더보드 인수 기준

### ACC-DB-01 — 원장 근거와 익명 호출 무점수

1. A가 새 유효 메모리를 제출하고 Gatekeeper Agent가 게시한다.
2. B는 로그인해 해당 version을 recall하고 fixture 성공 뒤 서버 발급 receipt로 `success`를 보고한다.
3. dashboard에서 A의 신규 승인과 B의 성공 사용이 각각 별도 event로 기록되는지 확인한다.
4. 같은 소비자·version을 재호출하고 receipt를 replay한다. 다시 점수를 올릴 수 없어야 한다.
5. 익명 사용자 C가 같은 memory를 여러 번 호출한다. 익명 usage 통계가 별도 집계되더라도 score_events, A의 points, leaderboard aggregate가 변하지 않는지 조회한다.
6. A 본인의 다른 device에서 호출/성공을 보고한다. account identity 기준으로 self-use 점수가 없어야 한다.
7. 공개 leaderboard opt-in/off 계정, 계정 email, device ID, 익명 호출을 비교한다.
8. version을 revoke하거나 rollback해 회수가 필요한 경우 기존 점수를 수정/삭제하지 않고 reversal event를 생성한다.

**합격:** 기여자가 받는 사용 점수는 다른 로그인 계정의 유효한 성공 영수증에서만 나온다. recall 횟수는 점수가 아니다. request body의 author ID, consumer ID, points는 신뢰하지 않는다. score event는 unique idempotency key와 원인 version/receipt를 갖고, event 합계 재생으로 dashboard total을 재현할 수 있다. 공개 순위는 opt-in handle과 합계만 노출한다. 관련 기준은 `SCORE-01~24`, `DB-07~17`, `API-26~39`, `WEB-18~24`, `FLOW-06`, `SEC-C`다.

### ACC-DB-02 — 동시성·재시도·롤백

1. 동일 URL/task/intent를 대상으로 여러 contribution을 동시에 큐잉하고 서로 다른 worker가 claim한다.
2. replay worker 종료/재시작, transaction 중간 오류, 동일 idempotency key 재시도를 각각 주입한다.
3. 최신 version pointer와 decision history, score event 수, audit trail을 확인한다.

**합격:** 활성 메모리가 중복 생성되지 않고 job 유실이나 두 번 지급이 없다. 외부 LLM/replay를 기다리며 DB transaction을 붙잡지 않는다. worker 재처리는 결정 digest와 idempotency key에 묶인다. rollback은 이전 immutable version을 보존하며, 권한이 있는 예외 조치만 audit trail을 추가한다. 세부 DB oracle은 [`database.md`](database.md)의 `DB-05~16`을 따른다.

### ACC-DB-03 — 기여 메모리 접근 분석과 차트

1. A가 authored version v1을 게시하고, 다른 로그인 계정 B/C가 recall 결과를 받고 일부는 상세를 연다. B는 유효한 `success`, C는 `partial`을 보고한다.
2. 익명 계정 없는 호출도 v1을 recall하고 상세를 연다. A 본인의 인증된 recall과 상세 접근도 만든다.
3. B가 다시 recall하고, 다른 author D가 같은 record의 새 version v2를 게시한 뒤 D의 version에도 별도 호출을 만든다.
4. A의 `/v1/me/memories/analytics`와 dashboard에서 일/주 bucket을 확인하고, D 및 다른 계정으로 각각 조회한다.
5. 차트와 표의 bucket 값, 기간 합계, 마지막 갱신 시각을 API aggregate 및 DB row와 대조한다.

**합격:** v1의 지표는 해당 version author A에게만 보이고 v2의 지표와 섞이지 않는다. authenticated recall 전달, 상세 응답, distinct non-author account, accepted outcome 결과 수와 anonymous aggregate가 서로 구분된다. 작성자 본인의 인증 사용은 타인 수와 점수에서 제외된다. 익명 수는 고유 인원으로 표시되지 않고, 상세/recall count는 실제 사람이 읽거나 성공했다는 보장으로 설명되지 않는다. 분석 응답·aggregate에 reader identity, receipt ID, device ID, email, raw URL이 없고 단순 접근은 점수를 만들지 않는다. 차트 수치는 API와 표에서 동일하며 선택 기간 고유 계정 합계는 bucket별 수의 합이 아닌 중복 제거 값이고, cross-account 조회는 404다. 관련 기준은 `WEB-25`, `API-40`, `DB-19`, `SCORE-15~24`, `SEC-C`, `PRIV-08`이다.

### ACC-PROFILE-01 — 내 순위와 공개 프로필 편집

1. 합성 사용자 A/B/C에게 서로 다른 score ledger와 opt-in 상태를 준비한다. 동점과 비공개 고득점 계정을 각각 포함한다.
2. A가 `/dashboard`에 로그인해 전체 순위와 공개 cohort 순위를 조회한다. 클라이언트가 rank/points를 보내지 않았는지 확인한다.
3. `/settings/profile`에서 유효 handle, H2/굵게/목록/inline HTTPS link가 있는 bio, 서비스 링크를 입력해 저장 후 다시 연다.
4. 공개 leaderboard에서 handle을 클릭해 `/u/{handle}`로 이동하고 profile 내용을 확인한다. 외부 링크를 새 탭에서 열어 destination과 `noopener noreferrer`를 검사한다.
5. handle/링크에 저장형 XSS, javascript/http, userinfo, internal host, token query를 넣고 실패 상태와 데이터 미저장을 확인한다.
6. B가 A 프로필을 PUT하고, A가 opt-out한 뒤 목록/프로필을 다시 조회한다. DB/API/cache를 함께 검사한다.

**합격:** 개인 전체 순위는 A에게만 보이고 공개 순위는 opt-in cohort의 경쟁 순위다. handle과 profile 링크는 공개 동의가 있을 때만 제공된다. editor schema 외 HTML은 실행되지 않고, 서비스 링크는 HTTPS public destination만 허용한다. 타인 편집/handle 소유권 우회는 실패하며 opt-out 뒤 공개 응답과 캐시에서 프로필이 제거된다. 관련 기준은 `PROFILE-01~10`, `FLOW-08`, `SCORE-15~24`, `SEC-A`, `SEC-C`, `SEC-D`다.

## 6. 릴리스 전 추적표

| 요구 기능 | 웹 확인 | 서버/MCP 확인 | DB 확인 | 관련 시나리오 |
|---|---|---|---|---|
| 공개 MCP/CLI 카드 복사와 설치 연결 | `WEB-09`, `WEB-16` | `MCP-16~24`, `INST-48`, `SMK-01~14` | endpoint/secret 저장 검사 | `ACC-MCP-01~02`, `CUST-01~04` |
| 브라우저 도구와 memory 도구 공존 | `/guide`의 browser MCP 분리 안내 | `tools/list`, recall 후 browser fixture 동작 | 사용 receipt version 참조 | `ACC-MCP-02` |
| 이메일 로그인, 기여, 검수와 게시 후 My Page | `F-EMAIL-01~07`, `WEB-17~24`, `MYPAGE-01~22` | `API-20~40`, authenticated contribute/review/outcome, trusted replay worker | FK, nonce, attribution/version, score ledger, outbox | `ACC-WEB-01~03`, `EMAIL-PROD-01~34`, `MYPAGE-07~40` |
| 관리자 가입자/기여 조회와 Muse 검수·게시 | `F-ADMIN-01~10`, `ADMIN-01~32` (`../spec/admin.md`) | 전용 email session, Muse provider, trusted replay/publish transaction | cursor/index, generation fence, immutable version, exactly-once score/audit | `ACC-ADMIN-01`, `SEC-D15~21`, `CUR-17`, `MODEL-09` |
| Agent URL/task 중복·최신성 결정 | `WEB-20~22` 상태와 사유 | `GK-39~48`, `DEDUP-13~18`, `MODEL-01~10` | active pointer와 immutable version | `ACC-SERVER-01~02`, `ACC-MODEL-01` |
| 타인 성공 사용 점수 | `WEB-23~24` | `API-36`, `SCORE-15~24` | receipt uniqueness와 ledger replay | `ACC-DB-01` |
| 기여 memory version별 접근 분석 | `WEB-25` | `API-40` | `memory_usage_aggregates` reconciliation 및 author scope | `ACC-DB-03` |
| 공개 프로필과 rich-text bio | `/settings/profile`, `/leaderboard`, `/u/{handle}` | `GET/PUT /v1/me/profile`, 공개 profile read | handle unique, opt-in/score ledger | `ACC-PROFILE-01`, `FLOW-08` |
| anonymous abuse 차단 | 익명 대시보드 없음 | 익명 recall은 user-level outcome 없음 | score_events 변경 0건 | `ACC-MCP-03`, `ACC-DB-01` |
| URL + 키워드 recall로 작업 시작 | `/guide`의 사용법 | `RCL-01~22`, `MEM-04~05` | `memory_index` 일치 | `ACC-MCP-04` |
| 가입 상태 × Chrome/CLI/Arise 고객 경로 | 공개 guide, 계정 연결 상태 | guest / account-only / linked 상태별 실제 호스트 호출 | receipt 소유권·익명 score 0·작성자 점수 | `ACC-MCP-05`, `CUST-01~06` |
| Muse 큐레이터 도구 루프 | `WEB-20~22` 상태와 사유 | `CUR-01~19`, `MODEL-01~10` | decision·version 이력 | `ACC-SERVER-03` |
| 기여 폭주와 큐 | 기여 상태 표시 | `QUE-01~18` | `gatekeeper_jobs` 상태 | `ACC-SERVER-04` |
| 계정 삭제와 기여 귀속 철회 | device 해지·dashboard 차단 | private write 차단·Gatekeeper 재평가 | pending payload purge 및 attribution 비식별화 | `ACC-PRIV-01`, `FLOW-07` |

## 7. 합격 상태 기록

케이스는 `pass`, `fail`, `blocked`, `not-run` 중 하나로 기록한다. `pass`에는 웹 화면 캡처가 아니라 API trace, 실제 MCP tool trace, 독립 fixture oracle, DB row/digest 중 해당하는 근거를 연결한다. 호스트별 설치 스모크가 되지 않은 항목은 일반 MCP 계약 통과로 대신하지 않는다. 실행 기록의 host version, template version, server build, schema migration, gatekeeper policy version을 함께 남긴다. 현재 문서화 작업만으로는 어떤 제품 동작도 `pass`로 기록할 수 없다.

## 8. Google Docs 양식 메모리 확인 (2026-11-01, #182)

실제 Arise 0.1.62의 한국어 Google Docs에서 새 구매 요청서 문서를 만들었다. native 키보드로 문서 이름을 확정하고, Heading 1 제목·신청자/날짜/번호·사유·검토/승인란 및 실제 4열 3행 품목 표를 입력했다. 마지막 셀 뒤의 Tab을 생략해 추가 빈 행을 만들지 않았고, 새 탭으로 다시 연 뒤 saved-to-Drive 상태와 제목·본문·표를 확인했다. 빈 칸 표시는 재사용할 양식의 의도된 필드다. 별도로 템플릿 갤러리에서 프로젝트 제안서 생성·이름 변경·저장도 확인했지만, 갤러리 예제의 모든 샘플 필드를 교체하는 전체 과정을 검증한 것은 아니다.

개인 문서 URL·계정 식별자·원문은 공개 seed나 이 기록에 넣지 않는다. 단계별 확인 범위는 `crates/wam-server/seed/document_template_sources.json`에 남긴다. 새 요청서·갤러리 레코드는 `Example:` 제목과 무작성자 규칙을 유지하며 검수된 운영 메모리나 속도 개선 수치로 표시하지 않는다.

- 서버 검증: `cargo test -p wam-server --lib --test examples --test productivity_examples --test diagnostics --test shutdown --locked`. 레코드 스키마, Docs/Sheets 경로 격리, request/purchase/form/proposal 키워드 recall, 갱신의 버전 추가·재시작·잠금·기존 작성자/검수/퇴역 행 보존을 포함한다.
- 웹 검증: `pnpm --dir web test`, `pnpm --dir web check`, `pnpm --dir web build`. 랜딩의 양식 링크는 메모리 라이브러리로 이어진다. 직접 공유하는 `#document-templates` 앵커는 정적 HTML에도 존재하며 데이터 수신 후 이동한다. catalog에 해당 예제가 없는 경우 예제 상세 링크를 만들지 않는다.
- 브라우저 검증: `playwright.memories.config.ts`의 랜딩→양식 카드→실제 레코드 상세, 한글 구매 양식 검색, 1440/390/320px 및 axe 검사. `playwright.remaining.config.ts`는 한·영 랜딩의 대비와 로그인된 1024~1440px 메뉴를 검사한다.
- 기존 저장소의 예제 갱신은 서버 시작 시 `WAM_EXAMPLES=refresh`로 명시한다. 일반 ensure는 계속 additive다. 실제 repository 잠금/트랜잭션 안에서 payload가 바뀐 무작성자 example-seed에만 새 버전을 추가하고, 작성자·검수·지원/재생 증거·퇴역 상태가 있는 행은 보존한다.
- `#27`의 같은 공개 세션 이름인데도 소유 탭 재선택을 `tab_busy`로 거부하는 현상이 재현되어 작업 호출을 새 소유 탭의 연속 `run_script`에 묶었다. Arise 근본 수정 완료로 처리하지 않는다.
