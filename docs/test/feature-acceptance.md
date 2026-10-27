# 기능 인수 테스트: 웹 설치에서 기여 점수까지

상태: 기능 개발 전 수용 기준. 이 문서는 요구사항을 사용자 관점에서 처음부터 끝까지 확인한다. 제품 구현과 실행 결과가 아직 없으므로 아래의 합격 조건은 테스트 명세이며, 실제 통과 이력을 뜻하지 않는다.

각 성공 경로의 실패·복구 주입은 [`failure-matrix.md`](failure-matrix.md)의 교차 계층 케이스와 함께 수행한다. 정상 경로에서 에러를 반환하는지뿐 아니라, 접수된 상태의 유실·중복·오인 성공도 확인한다.

## 1. 목표와 공통 환경

이 테스트 묶음은 웹 가이드에서 MCP 설정을 복사한 사용자가 Claude Code 또는 Codex에 메모리 MCP를 연결하고, Chrome 계열 브라우저 도구와 함께 실제 웹 fixture를 조작하는 흐름을 확인한다. 비회원, 웹 계정만 있고 host 미연결인 사용자, host까지 연결한 회원을 ChromeBridge/browser MCP, CLI/local stdio, Arise 경로에서 각각 확인한다. 사이트 로그인, 기여자 계정 귀속, Gatekeeper Agent의 URL/task별 판정, 버전 갱신, 로그인 사용자 성공 사용에 대한 점수, 익명 사용 무점수까지 서버와 DB 증거로 확인한다. 상세 실행 표는 [`customer-paths.md`](customer-paths.md)를 따른다.

테스트 서버와 웹앱은 loopback 또는 별도 staging 계정에서 실행하고, OAuth는 fake provider를 우선 사용한다. 브라우저는 공개 사이트에 발행·결제하지 않고 로컬 fixture에 초안 저장만 한다. Claude/Codex의 일반 사용 프로필과 비밀 정보가 든 사용자 설정은 건드리지 않고, 일회용 host profile과 합성 계정만 쓴다. Chrome browser MCP는 페이지 조작 도구이고 Web Agentic Memory MCP는 recall/propose/outcome 도구다. 둘은 별도 서버로 설치되며, 테스트는 두 서버가 한 에이전트 세션에서 공존하는지만 확인한다.

## 2. 웹 UI와 로그인 인수 기준

### ACC-WEB-01 — 방문, 로그인, 계정 소유권

1. 로그아웃 상태로 홈, `/memories`, `/guide`, `/leaderboard`를 연다.
2. 공개 승인 메모리와 호스트별 설치 설명은 볼 수 있지만 기여 제출, 개인 dashboard, receipt/outcome은 로그인 안내를 표시하는지 확인한다.
3. GitHub OAuth 대역으로 사용자 A에 로그인한다. `/dashboard`에서 사용자 ID가 아닌 공개 handle과 본인의 점수/기여 요약이 표시되는지 확인한다.
4. 브라우저 세션의 CSRF 및 만료 동작을 확인하고 로그아웃한다. 보호 화면 재방문 시 세션이 사라져야 한다.
5. 사용자 B로 로그인한 새 세션에서 A의 contribution UUID와 receipt ID에 직접 접근한다.

**합격:** public/read 경로와 account/write 경로가 분리된다. A는 자기 contribution과 score ledger만 보고 B의 레코드는 404 또는 동등한 권한 오류로 가려진다. URL 전체, 이메일, 토큰, private trace가 화면·HTML source·일반 로그에 노출되지 않는다. 관련 기준은 `WEB-02`, `WEB-05`, `WEB-16`, `WEB-18~19`, `API-32~34`, `SEC-D`다.

### ACC-WEB-02 — 기여 동의, 전송 미리보기, 상태 확인

1. 처음 가입한 A 계정에서 contribution consent 기본값이 off인지 확인한다.
2. `/contribute`에서 fixture URL과 `compose-draft` task를 선택한다. 화면에는 원 URL query가 아니라 정규화된 `/editor/post/:id` preview와 전송 필드 목록만 표시한다.
3. 동의하지 않은 채 submit을 누른다. 클라이언트 요청이 발생하지 않고 queue도 비어 있어야 한다.
4. preview를 확인하고 별도 동의 후 한 번 제출한다. 요청 body에 원문 prompt, 입력 본문, DOM selector, screenshot, cookie, query, consumer/author ID가 없는지 검사한다.
5. 대시보드에서 `received → validating → comparing → replaying → published/other decision` 상태를 확인한다. 다른 계정 B로 같은 페이지를 열어 A의 비공개 상태를 확인한다.
6. Gatekeeper 처리 전 contribution 철회를 수행하고, 추가 업로드와 pending job이 차단되는지 확인한다.

**합격:** contributor는 서버 세션의 A이며 UI나 body에서 다른 ID를 지정할 수 없다. 동의 전 POST는 0건이다. 사용자는 어떤 필드가 전송되는지 제출 전에 알 수 있고, pending row는 철회 후 policy에 맞게 purge된다. 이미 처리된 public version을 지우려면 별도 Gatekeeper lifecycle 판정과 audit이 필요하다. 관련 기준은 `WEB-17~22`, `PRIV-01~08`, `API-20~25`, `DB-02~04`, `SEC-A`다.

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

**합격:** 서버로 나간 것은 안전 URL 패턴과 어휘 키워드뿐이다. 순위는 route, task, 키워드 순으로 결정적이며 교차 host가 없다. 호출 시점 규칙이 trace에 나타난다. 관련 기준은 `RCL-01~16`, `MEM-04~05`, `MCP-20~24`, `BEH-01`, `BEH-08~09`, `SMK-06`, `HOST-03~04`다.

### ACC-MCP-05 — 고객 가입 상태 × host 경로 매트릭스

[`customer-paths.md`](customer-paths.md)의 `CUST-01~06`을 각각 독립 run으로 실행한다.

1. 비회원이 홈페이지와 `/guide`를 열어 Claude Code 또는 Codex 템플릿을 복사하고, 일회용 host profile에 Memory MCP를 등록한다. ChromeBridge/browser MCP도 별도로 등록한다.
2. 알려진 fixture에 한국어 작업을 요청하고 작업 전 recall, browser action, 독립 저장 oracle을 검사한다. URL 패턴과 `naver`, `blog`, `write` 키워드로 정확한 메모리가 선택되는지 확인한다.
3. 새 웹 계정을 만들거나 로그인하되 MCP/Arise host를 연결하지 않은 상태에서 같은 요청을 반복한다. host가 계속 anonymous로 판정하고 receipt/write를 허용하지 않는지 검사한다.
4. 각 지원 경로에 맞춰 host OAuth 또는 device link를 명시적으로 승인하고 반복한다. status의 연결 상태와 receipt 소유권이 바뀌되 공개 메모리의 ID·version·순위는 그대로인지 확인한다.
5. 비회원과 linked member에 대해 직접 CLI, local stdio MCP, Arise 내장 adapter를 별도 실행한다. generic MCP 성공을 Arise/CLI 성공으로 대체하지 않는다.
6. guest의 propose/outcome, 다른 계정의 receipt 사용, self-use, 만료·해지 credential, receipt 재전송을 시도하고 API/DB/score ledger를 비교한다.

**합격:** 비회원은 가입 강요 없이 public read와 fixture 작업을 끝내며 user-level receipt, write, score가 0이다. 웹 계정만 만든 host도 익명이다. 명시적 host link 뒤에는 동일 user에 묶인 receipt만 허용하고, 성공·중복방지 규칙대로 메모리 작성자 점수만 변한다. 만료·해지 자격증명은 익명으로 조용히 downgrade하지 않는다. Chrome, CLI, Arise 간 결과는 같고 인증 차이는 receipt·권한에만 나타난다. 관련 기준은 `CUST-01~06`, `HOST-01~06`, `LINK-01~11`, `MCPF-05~17`, `RCL-01~16`, `SCORE-15~24`, `FAIL-04/21/28`이다.

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

**합격:** 실제 demo run에서 Meta provider가 decision candidate 생성에 사용되고 model ID/version이 추적 가능하다. Gatekeeper policy engine만 최종 persistence/publication을 수행한다. model failure나 비정상 응답은 catalog, active pointer, score ledger를 바꾸지 않는다. fake provider는 contract/CI test에 쓸 수 있지만 실제 Meta 연결 acceptance를 대신하지 않는다. 관련 기준은 `MODEL-01~09`, `GK-44~48`, `DB-11`, `DB-15~16`, `SEC-A`, `SEC-B`다.

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

**합격:** 모델은 도구로만 행동하고 모든 상태 변경은 서버가 검증한 결정 도구에서만 일어난다. 불변조건은 모델 출력과 무관하게 유지된다. 실제 provider가 한 번 이상 판정에 쓰였다. 관련 기준은 `CUR-01~17`, `MODEL-01~09`, `GK-39~48`, `DEDUP-13~18`, `SEC-I`다.

### ACC-SERVER-04 — 기여 폭주와 큐

1. 같은 scope에 기여 50건과 서로 다른 scope에 기여 50건을 10초 안에 접수한다.
2. 큐 깊이와 가장 오래된 job의 나이 지표를 확인한다. 같은 scope의 active version은 1개이고 모델 호출 수가 같은 scope 기여 수보다 적은지(병합) 확인한다.
3. 한 기기가 상한 이상으로 폭주하는 동안 다른 사용자의 기여가 계속 처리되는지 확인한다.
4. 깊이 상한 초과를 만들어 429와 `Retry-After`를 확인하고, 이미 202를 받은 job이 모두 남아 있는지 확인한다.
5. 모델 429, 장애, 예산 소진을 주입한다. job이 `retry` 또는 `waiting_budget`에 남고 카탈로그가 바뀌지 않으며, 복구 뒤 처리가 재개되는지 확인한다.
6. worker를 lease 중 강제 종료한다. lease 만료 뒤 재처리되고 중복 게시나 중복 점수가 없는지 확인한다.
7. 대기 중인 기여를 철회해 job이 취소되고 payload가 지워지는지 확인한다.

**합격:** 202를 받은 job은 유실되지 않고 같은 scope에 active 메모리가 중복되지 않는다. 모델 장애와 예산 소진은 공개 카탈로그와 recall에 영향을 주지 않는다. 폭주 사용자가 다른 사용자를 굶기지 않는다. 관련 기준은 `QUE-01~14`, `DB-10~11`, `GK-37~38`, `PRIV-03`, `MODEL-05~06`, `SEC-I03`이다.

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
| 로그인, 기여, 본인 dashboard | `WEB-17~19` | `API-32~35`, authenticated propose | FK, nonce, 소유권 | `ACC-WEB-01~02` |
| Agent URL/task 중복·최신성 결정 | `WEB-20~22` 상태와 사유 | `GK-39~48`, `DEDUP-13~18`, `MODEL-01~09` | active pointer와 immutable version | `ACC-SERVER-01~02`, `ACC-MODEL-01` |
| 타인 성공 사용 점수 | `WEB-23~24` | `API-36`, `SCORE-15~24` | receipt uniqueness와 ledger replay | `ACC-DB-01` |
| 기여 memory version별 접근 분석 | `WEB-25` | `API-40` | `memory_usage_aggregates` reconciliation 및 author scope | `ACC-DB-03` |
| 공개 프로필과 rich-text bio | `/settings/profile`, `/leaderboard`, `/u/{handle}` | `GET/PUT /v1/me/profile`, 공개 profile read | handle unique, opt-in/score ledger | `ACC-PROFILE-01`, `FLOW-08` |
| anonymous abuse 차단 | 익명 대시보드 없음 | 익명 recall은 user-level outcome 없음 | score_events 변경 0건 | `ACC-MCP-03`, `ACC-DB-01` |
| URL + 키워드 recall로 작업 시작 | `/guide`의 사용법 | `RCL-01~16`, `MEM-04~05` | `memory_index` 일치 | `ACC-MCP-04` |
| 가입 상태 × Chrome/CLI/Arise 고객 경로 | 공개 guide, 계정 연결 상태 | guest / account-only / linked 상태별 실제 호스트 호출 | receipt 소유권·익명 score 0·작성자 점수 | `ACC-MCP-05`, `CUST-01~06` |
| Muse 큐레이터 도구 루프 | `WEB-20~22` 상태와 사유 | `CUR-01~17`, `MODEL-01~09` | decision·version 이력 | `ACC-SERVER-03` |
| 기여 폭주와 큐 | 기여 상태 표시 | `QUE-01~14` | `gatekeeper_jobs` 상태 | `ACC-SERVER-04` |
| 계정 삭제와 기여 귀속 철회 | device 해지·dashboard 차단 | private write 차단·Gatekeeper 재평가 | pending payload purge 및 attribution 비식별화 | `ACC-PRIV-01`, `FLOW-07` |

## 7. 합격 상태 기록

케이스는 `pass`, `fail`, `blocked`, `not-run` 중 하나로 기록한다. `pass`에는 웹 화면 캡처가 아니라 API trace, 실제 MCP tool trace, 독립 fixture oracle, DB row/digest 중 해당하는 근거를 연결한다. 호스트별 설치 스모크가 되지 않은 항목은 일반 MCP 계약 통과로 대신하지 않는다. 실행 기록의 host version, template version, server build, schema migration, gatekeeper policy version을 함께 남긴다. 현재 문서화 작업만으로는 어떤 제품 동작도 `pass`로 기록할 수 없다.
