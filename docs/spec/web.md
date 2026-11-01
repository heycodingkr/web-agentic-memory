# WEB: 로그인, 기여 큐, 대시보드, 리더보드

상태: SvelteKit 공개 메모리, 프로필, 대시보드, 기여·검수 화면과 Resend 이메일 코드 로그인, 설정된 경우 GitHub OAuth/device flow가 구현되어 있다. Account/profile/contribution/consent/score state는 local `application.json` snapshot 또는 PostgreSQL repository에 저장되고, validated publication worker는 게시·귀속·score +20 및 outbox를 transaction으로 기록한다. 공개 배포에서 이메일 인증 설정이 켜져 있는 것은 확인했지만, 실제 메일 수신과 운영 worker/replay 검수는 별도 수용 기준으로 남아 있다. Per-memory analytics, quota/denial dashboard, remote Streamable HTTP MCP POST도 구현되어 있다. Local isolated web/CLI/MCP smoke와 PostgreSQL integration tests가 통과했다. 표준 OAuth discovery/DCR/PKCE/refresh/revoke, CLI `wam link`/install/doctor, OS secure token storage, remote GET/POST SSE도 구현됐다. macOS Keychain, 격리 Codex/Claude 등록, 합성 Chrome 및 native Codex OAuth와 Arise browser MCP bridge를 검증했다. 가이드의 복사·endpoint·인증 안내·선택형 bridge·모바일 layout은 Playwright로 확인했다. 실제 GitHub OAuth·외부 curator/replay provider·Redis failover·Claude/Arise Hub remote OAuth·다른 OS runtime acceptance는 별도다.

## 공통 경험

웹사이트는 제품 설명, 설치, 계정 로그인, 기여 제출과 결과 확인, 메모리 조회, 점수 대시보드를 한곳에서 제공한다. 공개 방문자는 승인된 메모리를 검색하고 읽을 수 있다. 쓰기, 기여 소유권, 게이트키퍼 큐의 상태 확인, 사용 영수증 조회는 로그인한 계정에만 제공한다.

All user-facing copy defaults to English through the localization layer; do not hard-code Korean interface labels. Dashboard quota/activity and author-memory analytics follow [`usage-analytics.md`](usage-analytics.md). 로그인 후 My Page의 화면 구성·기여 진행/반영 결과·내 메모리·랭킹/포인트를 연결하는 후속 계약과 현재 구현 차이는 [`my-page.md`](my-page.md)를 따른다. The UI distinguishes denied requests, unavailable data, and real zero values, and provides accessible chart/table alternatives.

## 주요 화면

| 경로 | 기능 | 로그인 |
|---|---|---|
| / | 문제, 작동 흐름, 측정 한계, 시작 버튼 | 불필요 |
| /guide | Claude/Codex/기타 MCP 설정 문자열과 OS별 `wam` CLI 설치 명령 복사, 인증 단계, URL+키워드 테스트 호출. Chrome/browser MCP 연결은 별도 안내하고 자동 현재 탭 URL 회수는 선택한 connector의 문서화된 local interface를 쓰는 선택 기능으로 구분 | 공개 안내·복사는 불필요, 기여·영수증 연결은 로그인/host link 필요 |
| /login | 이메일 인증 코드·OAuth 로그인, 로그아웃, 계정 상태 | 진입점 |
| /memories | 메모리 라이브러리: 제품(Docs·Sheets·Gmail 등)·task·출처(내장 예시/기여) 필터와 기기 내 검색, 내장 예시는 `Example` 배지 | 읽기 불필요 |
| /memories/{id} | 핵심 포인트(레벨 1), 단계·사전 조건·복구(레벨 2 JSON을 파싱), URL 범위, task, 버전, 날짜, 복사 가능한 `wam recall` 명령, 비식별 기여 통계 | 읽기 불필요 |
| /dashboard | 현재 전체/공개 순위·점수·원장·최근 기여·30일 API 사용량과 author version 일/주 통계. 목표 My Page의 진행/반영 상세·목록·기간 선택·자동 진입은 [별도 명세](my-page.md) | 필수 |
| /contribute | URL/task 확인, 전송 payload 미리보기, 동의 후 제출 | 필수 |
| /review | local review token으로 기여 상태 집계, claim, approve/reject 상태 기록 | 별도 로컬 검수 토큰 |
| /metrics | 개인을 식별하지 않는 공개 recall/detail/outcome 합계 | 불필요 |
| /leaderboard | 기간별 공개 참여자 순위. handle은 공개 프로필 링크; trend/rank movement uses stored history only | 읽기 불필요, 노출은 사용자 opt-in |
| /settings/profile | handle, rich-text bio, 서비스 링크, 공개 설정 편집 | 필수 |
| /u/{handle} | opt-in 공개 프로필과 외부 서비스 링크 | 공개 프로필만 |
| /settings/devices | 연결 기기 목록, 토큰·기기 해지 | 필수 |
| /admin | 별도 허용 이메일 매직 링크 로그인, 가입자/기여 검토, Muse Gateway 검수·게시 | 전용 관리자 세션. 상세 계약은 [admin spec](admin.md) |

## 로그인과 계정

로그인은 [Resend 이메일 인증 코드](email-auth.md)와 설정된 경우 GitHub OAuth를 사용한다. 이메일 인증은 6자리 일회용 코드와 요청 브라우저의 challenge cookie로 검증하고 기존 세션·CSRF·기기 연결을 재사용한다. 계정 식별은 GitHub provider subject 또는 정규화 이메일의 서버 전용 HMAC binding을 provider namespace별로 사용한다. 이메일과 provider 간 계정은 자동 병합하지 않는다. 이메일은 공개하지 않고, 점수 귀속은 로그인 세션에서 파생한 내부 user_id만 쓴다. 클라이언트가 보낸 author_id, consumer_id, score 값을 신뢰하지 않는다.

세션은 Secure, HttpOnly, SameSite 정책을 사용한다. 로그아웃은 웹 세션을 지우며, 사용자가 MCP 기기까지 해지하기를 원하면 별도 화면에서 개별 또는 전체 토큰을 취소한다. 한 계정의 기기마다 마지막 접속 시각, 호스트 종류, 권한 범위만 노출한다.

## 기여 제출과 진행 상태

기여 폼은 canonical URL preview, task category, 자동 수집하는 정규화 action summary, 명시적 공유 동의, 전송될 필드 목록을 보여준다. 원문 페이지 콘텐츠와 브라우저 도구의 argument/result는 보내지 않는다. 사용자가 제출을 누르면 서버는 로그인한 세션을 contributor로 지정하고 nonce를 발급한다. UI에 사용자가 직접 contributor id를 입력하는 필드는 없다.

현재 owner 응답/화면은 queued, in_review, approved, rejected, withdrawn, expired, consent_required의 접수 상태를 제공한다. 실제 durable worker는 검증된 게시와 support, 추가 검토/재시도 등을 구분하며 검증된 신규/개선 게시와 +20을 transaction으로 기록한다. review token은 contributor와 분리하고 generation으로 오래된 승인/claim을 거부한다. approved 문자열만으로 게시/점수 여부를 추정하지 않는다. 대기 이유·처리 단계·실제 memory/version 결과와 owner 목록/상세의 연결은 [My Page](my-page.md)의 목표 계약이며 아직 현재 응답/화면에 충분히 반영되지 않았다.

기여 제출의 확정 여부는 POST가 돌려준 서버 receipt로 판정한다. receipt를 받은 뒤 목록 GET이 실패하면 확인된 제출을 유지하고 목록 새로고침 실패를 별도로 알린다. 사용자가 새로고침을 누를 때는 GET만 재시도하며 POST를 자동 재전송하지 않는다. worker 검수, 게시, memory/version link와 점수는 서버 결과를 받기 전까지 성공 상태로 표시하지 않는다.

## 대시보드와 점수

현재 구현은 총점·개인/공개 순위·최근 기여 상태·점수 원장과 author version별 기간 그래프/표를 제공한다. 원장은 일시·event·memory/version·delta와 cursor를 제공한다. 조회 단위의 다른 계정/익명 집계가 구현돼 있지만 고유 reader 수는 제공하지 않는다. 예시 기초 점수는 원장 내역에서 제외된다. 현재 계정의 401은 개인 데이터를 제거하고 오래된 요청 응답은 무시한다. 다만 초기 API 한 곳의 실패가 정상 패널까지 비우는 구조는 My Page의 패널별 오류 처리로 보완해야 한다.

개인 대시보드는 계정 요약과 본인이 작성자로 귀속된 memory version별 성과를 제공한다. 중복/support로만 제출되어 version author로 등록되지 않은 기여에는 해당 memory의 저자 분석 권한이 생기지 않는다. 목표 My Page에는 전체 제출 상태별 수·실제 반영 링크·현재/과거 버전 공개 상태를 추가하고, 기여 접수 전체와 author 메모리 목록을 구분한다. 이 추가 상태/카운트가 현재 모두 구현됐다고 표시하지 않는다.

memory version마다 기간별 차트를 제공한다. `다른 계정의 recall 결과 전달`은 작성자를 제외한 로그인 계정에 해당 version이 recall 응답으로 전달된 횟수이고, `다른 계정의 상세 열람`은 그 계정에 상세/expand 응답이 전달된 횟수다. 이는 실제 절차 사용/성공이나 고유 사람 수가 아니다. 유효한 receipt의 사용 version에 대한 success/partial/failed 보고는 별도 집계하며, 고유 reader 수는 이번 범위에서 수집·표시하지 않는다. 포인트는 이 접근/보고량과 분리해 서버 점수 정책과 원장으로 표시한다.

익명 recall/상세는 익명 요청 집계이며 본인의 로그아웃 요청일 수도 있다. 현재 카드 합계와 표의 분해 값을 My Page에서는 더 명확히 구분한다. 현재 차트는 기본 최근 30일과 일/주 grouping을 제공한다. 목표 UI는 API가 허용하는 7/30/90 UTC 날짜 선택을 추가하고, 최대 90일 데이터를 전체 기간/평생 누계로 표시하지 않는다. 차트와 같은 값을 표로 제공하고, 마지막 조회 시각·조회 실패·진짜 0건을 구분한다.

사용 영수증, consumer 이메일/계정, device ID, 원본 URL은 기여자 화면에 노출하지 않는다. 수치 분석은 해당 계정이 작성자로 귀속된 version만 반환하고, 공개 leaderboard에는 개인별 memory 접근 통계를 노출하지 않는다. 익명 사용과 작성자 본인의 사용은 점수 이벤트를 만들지 않는다. 다른 로그인 계정의 성공 보고가 점수 정책을 통과한 경우에만 저자에게 사용 점수를 지급한다. 개인 dashboard에는 본인만 볼 수 있는 전체 순위를 제공한다. 공개 leaderboard는 opt-in 사용자만으로 별도 순위를 매기며 handle과 점수 합계만 보여준다. 실명·이메일·기여 원문은 보여주지 않는다. 공개 프로필 기준은 [profile spec](profile.md)에 있다.

## 공개 프로필과 bio 편집

기여자는 `/settings/profile`에서 고유 handle, bio, 최대 5개의 서비스 링크를 편집한다. rich-text editor는 문단, H2/H3, 굵게/기울임, 순서/비순서 목록, HTTPS 링크를 제공한다. 임의 HTML과 embed는 받지 않으며 붙여넣기와 미리보기도 같은 schema 검증을 거친다. 편집기 입력은 키보드와 화면 낭독기로 조작할 수 있고 글자 수와 저장 상태를 보여준다.

리더보드에서 공개 handle을 누르면 `/u/{handle}`이 열린다. 공개 프로필은 opt-in 상태일 때만 조회되고 opt-out 즉시 404와 목록 제외가 적용된다. bio의 링크는 명시된 HTTPS 목적지로 이동하며 서버가 URL을 fetch하지 않는다. 새 탭 링크는 `noopener noreferrer`를 적용한다. 기능·보안 기준은 [PROFILE tests](../test/profile.md)다.

## 최신성, 버전, 삭제

공개 메모리 상세 화면은 현재 활성 버전과 마지막 재현 검증 날짜를 보여준다. 사용자가 과거 버전을 직접 선택하지 않는 한 superseded/revoked 버전은 기본 검색에 노출하지 않는다. 사이트 UI 변경이나 점수 이상 징후가 있으면 needs-reverify 표시와 함께 새 기여를 기다릴 수 있다.

고객이 자기 미게시 기여를 철회하면 대기 큐에서 제거한다. 이미 게시된 공유 메모리를 고객이 단독으로 직접 지우지는 못한다. 기여자가 삭제 요청을 하면 gatekeeper가 해당 author attribution을 숨기거나 비식별 처리할 수 있고, 메모리 내용은 다른 독립 기여자 증거가 남아 있는지 확인한 후 유지·갱신·폐기한다. 보안·법률상 즉시 제거가 필요한 경우에는 관리자 kill switch가 동작하며 이유를 감사 로그에 남긴다.

## UI 상태와 접근성

로그인 전, 로딩, 결과 없음, 재인증 필요, 연결 실패, contribution pending, gatekeeper review, superseded, revoked 상태 각각에 명확한 설명과 다음 행동을 제공한다. 폼은 키보드로 전부 조작 가능하고 URL 패턴과 task 선택 입력에 label을 제공한다. 점수 차감이나 메모리 폐기는 되돌릴 수 없는 사용자 행동처럼 보이지 않도록 과거 버전·감사 내역을 설명한다.

## 수용 테스트

My Page의 새 요구사항은 [MYPAGE-01~40](../test/my-page.md)에서 별도 명세하며 아직 실행 결과가 아니다. 기존 WEB-16~31과 FLOW-01~06/09는 로그인·기여·메모리 갱신·기여 version 분석·quota/usage dashboard·접수 후 목록 refresh 복구를 검증한다. profile editor, 본인 순위, opt-in leaderboard와 handle 탐색은 [PROFILE-01~10](../test/profile.md) 및 FLOW-08/WEB-29에서 검증한다.
