# WEB: 로그인, 기여 큐, 대시보드, 리더보드

상태: 1차 상세 명세. 실제 웹 애플리케이션은 아직 없다.

## 공통 경험

웹사이트는 제품 설명, 설치, 계정 로그인, 기여 제출과 결과 확인, 메모리 조회, 점수 대시보드를 한곳에서 제공한다. 공개 방문자는 승인된 메모리를 검색하고 읽을 수 있다. 쓰기, 기여 소유권, 게이트키퍼 큐의 상태 확인, 사용 영수증 조회는 로그인한 계정에만 제공한다.

## 주요 화면

| 경로 | 기능 | 로그인 |
|---|---|---|
| / | 문제, 작동 흐름, 측정 한계, 시작 버튼 | 불필요 |
| /guide | Claude/Codex/기타 MCP 설정 문자열과 OS별 `wam` CLI 설치 명령 복사, 인증 단계, URL+키워드 테스트 호출. Chrome/browser MCP 연결 안내는 메모리 MCP와 별도 단계로 표시 | 공개 안내·복사는 불필요, 기여·영수증 연결은 로그인/host link 필요 |
| /login | OAuth 로그인, 로그아웃, 계정 상태 | 진입점 |
| /memories | 공개 승인 메모리 검색 | 읽기 불필요 |
| /memories/{id} | URL 패턴, task, 버전, 검증일, 비식별 기여 통계 | 읽기 불필요 |
| /dashboard | 내 전체 순위와 점수, 공개 참여 중이면 공개 순위, 기여 상태와 버전별 접근·사용 추이 차트 | 필수 |
| /contribute | URL/task 확인, 전송 payload 미리보기, 동의 후 제출 | 필수 |
| /leaderboard | 기간별 공개 참여자 순위. handle은 공개 프로필 링크 | 읽기 불필요, 노출은 사용자 opt-in |
| /settings/profile | handle, rich-text bio, 서비스 링크, 공개 설정 편집 | 필수 |
| /u/{handle} | opt-in 공개 프로필과 외부 서비스 링크 | 공개 프로필만 |
| /settings/devices | 연결 기기 목록, 토큰·기기 해지 | 필수 |
| /admin/gatekeeper | agent decision, 증거, 실패·저신뢰 예외 검토 | 관리자 또는 검토 권한 |

## 로그인과 계정

초기 로그인은 architecture 문서의 GitHub OAuth를 사용한다. 사용자 레코드는 OAuth provider와 provider subject의 조합으로 고정 식별한다. 이메일은 공개하지 않고, 점수 귀속은 로그인 세션에서 파생한 내부 user_id만 쓴다. 클라이언트가 보낸 author_id, consumer_id, score 값을 신뢰하지 않는다.

세션은 Secure, HttpOnly, SameSite 정책을 사용한다. 로그아웃은 웹 세션을 지우며, 사용자가 MCP 기기까지 해지하기를 원하면 별도 화면에서 개별 또는 전체 토큰을 취소한다. 한 계정의 기기마다 마지막 접속 시각, 호스트 종류, 권한 범위만 노출한다.

## 기여 제출과 진행 상태

기여 폼은 canonical URL preview, task category, 자동 수집하는 정규화 action summary, 명시적 공유 동의, 전송될 필드 목록을 보여준다. 원문 페이지 콘텐츠와 브라우저 도구의 argument/result는 보내지 않는다. 사용자가 제출을 누르면 서버는 로그인한 세션을 contributor로 지정하고 nonce를 발급한다. UI에 사용자가 직접 contributor id를 입력하는 필드는 없다.

기여 상태는 received, validating, comparing, replaying, decided, published, rejected, needs-human-review, superseded와 같은 단계로 보인다. 화면은 각 단계의 변경 시각과 결정 사유 코드를 설명하고, 개인 정보나 내부 프롬프트를 그대로 노출하지 않는다. 이미 제출한 기여는 소유자만 조회한다.

## 대시보드와 점수

개인 대시보드는 계정 요약과 본인이 작성자로 귀속된 memory version별 성과를 제공한다. 중복/support로만 제출되어 version author로 등록되지 않은 기여에는 해당 memory의 저자 분석 권한이 생기지 않는다. 대시보드에는 기여 상태, 승인·대기·반려 수, 점수 원장과 함께 각 memory version의 활성/교체/폐기 상태를 표시한다.

memory version마다 기간별 차트를 제공한다. `다른 계정의 recall 결과 전달`은 작성자를 제외한 로그인 계정에 해당 version이 recall 응답으로 전달된 횟수이고, `다른 계정의 상세 열람`은 그 계정에 상세/expand 응답이 전달된 횟수다. 이 값은 사람이 실제 절차를 읽거나 사용했다는 증거가 아니다. 별도로 다른 로그인 계정의 고유 수(기기 수가 아닌 계정 수)와 유효한 outcome 보고의 success/partial/failed 수를 보여준다. 성공 사용 점수는 이 접근 지표와 분리해 점수 원장에 표시한다.

익명 recall과 상세 열람은 `익명 집계`로 따로 표시한다. 익명 집계에는 고유 인원이나 작성자 본인 여부가 없으므로 고유 사용자 수로 표현하지 않는다. 차트는 기본 최근 30일, 7일·90일·전체 기간 선택을 제공하고 일별 또는 주별 값, 기간 합계, 마지막 갱신 시각을 보여준다. 고유 계정 수의 기간 합계는 선택 기간 전체에서 중복 제거하며, bucket별 고유 수를 더한 값으로 표시하지 않는다. 차트와 같은 값을 표로도 확인할 수 있으며, 결과가 없거나 집계가 늦을 때 그 상태를 명확히 표시한다.

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

WEB-16~25와 FLOW-01~06은 로그인·기여·메모리 갱신·기여 version 분석을 검증한다. profile editor, 본인 순위, opt-in leaderboard와 handle 탐색은 [PROFILE-01~10](../test/profile.md) 및 FLOW-08에서 검증한다.
