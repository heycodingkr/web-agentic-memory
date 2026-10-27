# SCORE: 로그인 기반 기여 점수와 어뷰징 방지

상태: 성공 사용 점수의 로컬 vertical slice는 구현됐다. `wam-server`는 다른 계정의 성공 결과에 memory author +2를 append-only score event로 기록하고, leaderboard와 개인 dashboard에 합산한다. PostgreSQL 트랜잭션, 승인 기여의 게시 및 +20, reversal, production abuse controls는 아직 구현되지 않았다.

## 현재 구현 범위

- 인증된 recall 결과의 owner-bound 영수증에 memory/version/author 귀속을 보존한다. 익명 recall에는 영수증이나 점수 event가 없다.
- outcome=success 중 `used_version_ids`로 실제 사용했다고 명시한 receipt 소속 버전에만 점수를 시도한다. 생략하거나 빈 목록이면 결과만 기록하고 점수는 없다. 중복 ID나 receipt 밖의 버전은 400이며 영수증을 소비하지 않는다.
- 결과와 사용 버전 집합은 최초 보고 때 함께 고정한다. 같은 집합의 순서가 바뀐 재전송은 멱등 처리하고, 결과나 집합을 바꾸면 409다. 본인 사용을 제외하고 consumer-version과 consumer-author 일일 상한을 최초 보고일(UTC) 기준으로 적용한다. 다음 날 재전송해도 quota가 새로 생기지 않는다.
- 개발 서버는 `.wam-data/scores.json`에 원장 event를 원자적으로 저장한다. 저장된 event 합계를 seed 점수에 더해 리더보드와 dashboard 순위를 계산한다.
- 이 구현은 알고 있는 seed memory author에만 적용된다. 수동 contribution 승인은 memory version을 게시하지 않으므로 author +20을 지급하지 않는다. 승인 publish 연결, PostgreSQL transaction/outbox, reversal, version별 author analytics는 다음 단계다. 본인 event 조회와 pagination은 `/v1/me/score-events` 및 dashboard에서 제공한다.

## 목적

점수는 유용한 웹 워크플로를 만든 사람에게 귀속되어야 한다. MCP를 많이 호출하거나 익명으로 반복 조회하는 행동에 점수를 주지 않는다. memory author가 받는 점수는 로그인한 다른 사용자의 성공 사용에 근거한다.

## 점수 대상과 발행 원천

모든 점수 수령인은 웹에 로그인된 사용자 계정이다. contribution 생성 때 서버가 contributor user_id를 넣고, request body의 contributor_id는 받지 않거나 무시한다. memory record와 version은 이 작성자 attribution 및 gatekeeper approval에 연결된다.

기본 정책 초안은 신규 또는 검증된 개선 memory version 승인에 version author +20, 다른 로그인 계정이 그 version을 성공 사용했을 때 해당 author에게 +2를 준다. duplicate/support 기여는 신규 승인 점수를 받지 않는다. 실패/부분 결과, 단순 recall, 미인증 사용, 작성자 본인의 사용은 사용 점수를 만들지 않는다. 수치는 versioned score policy로 관리하고 UI에서 현재 규칙을 공개한다.

## 성공 사용의 정의

점수 지급에는 다음 모두가 필요하다.

1. 메모리는 recall 당시 published 상태여야 한다.
2. 서버가 인증된 consumer 계정과 memory version에 바인딩한 receipt를 발행했어야 한다.
3. 브라우저 에이전트가 작업 완료 후 outcome=success와 실제 사용한 `used_version_ids`를 보고해야 한다. 사용 목록은 영수증에 든 버전의 중복 없는 부분집합(최대 3개)이다.
4. receipt는 해당 소비자 계정이 아직 사용하지 않은 유효 receipt이고, 만료되지 않았어야 한다.
5. 소비자 계정이 작성자와 다르고, 이 사용이 하루 quota 및 abuse check를 통과해야 한다.
6. 점수 지급 transaction에서 같은 receipt/event가 처리된 적이 없어야 한다.

로그인하지 않은 소비자는 published memory를 읽을 수 있지만, 서버는 score receipt를 발행하지 않는다. 익명 recall 수는 보안상 필요한 집계 지표에 그치며 leaderboard, author points, caller points에 절대 반영하지 않는다. 로그인했더라도 task가 끝나지 않았거나 성공 보고가 없으면 점수는 0이다.

## 본인 원장 조회

`GET /v1/me/score-events`는 `read:dashboard` 권한으로 본인이 수령한 event만 반환한다. 최근 append 순으로 `limit` 1~100개(기본 20)를 읽으며 `next_cursor`를 `before`로 보내 이전 내역을 이어 읽는다. cursor는 본인 event여야 하며 다른 계정 event와 없는 event는 동일한 404다. 각 event에는 ID, 종류, 점수 delta, memory/version, 시각만 담고 소비자 계정·receipt·idempotency key는 보내지 않는다. 이 API의 event 합계에는 demo seed 기초 점수가 없으므로 UI는 기초 점수와 실제 원장 지급을 혼동하지 않게 설명한다. 응답은 `Cache-Control: no-store`다.

## 차감과 재계산

published memory가 gatekeeper 재검증에서 폐기되거나 abuse로 취소되면 기존 지급을 원장 수정으로 지우지 않는다. 원래 event를 가리키는 reversal event를 기록한다. 원장 합계는 event delta 합으로 계산하며, 임의로 dashboard total을 바꿀 수 없다.

## 어뷰징 제어

- server identity: author와 consumer는 OAuth session 또는 device token에서 가져온다. MCP argument의 user id를 사용하지 않는다.
- self-use 방지: 같은 user 또는 같은 account-linked device 집합의 recall은 author credit을 주지 않는다.
- 반복 보고 방지: receipt는 random opaque id, 만료, 단회 outcome, DB unique idempotency key를 가진다.
- 호출 팜 방지: recall 횟수만으로 점수를 만들지 않는다. distinct authenticated consumer와 completed outcome만 본다.
- sybil 제한: 신규 계정의 점수 가중치를 낮추거나 보류하고, 계정·기기·시간·반복 상호호출 패턴을 rate limit/anomaly review에 사용한다. 비공개 식별 신호는 public leaderboard에 노출하지 않는다.
- 일일 상한: consumer-memory-version pair 및 consumer-account pair에 고정 상한을 적용한다. 같은 계정이 여러 기기를 써도 한 번으로 계산한다.
- 경쟁자 비방 방지: failed report만으로 메모리를 즉시 삭제하거나 점수를 차감하지 않는다. 독립 실패 신호와 gatekeeper replay로 재검증한다.
- 공개 프로필: leaderboard opt-in은 handle, 점수 합계와 `/u/{handle}` 프로필을 공개하는 동의다. opt-out이면 공개 목록과 profile endpoint 모두에서 숨긴다. 세부 규칙은 [`profile.md`](profile.md)에 있다.

## Dashboard와 공개 순위

로그인 사용자는 총점, 승인된 memory 수, 타 계정의 성공 사용 수, 최근 원장 event, 보류·회수 점수를 본다. 별도 기여 분석 화면에서는 본인이 author인 memory version별 recall 결과 전달 수, 상세 응답 수, 다른 로그인 계정의 고유 수, success/partial/failed 보고 수, 익명 집계와 기간별 추이를 볼 수 있다. 이 지표는 접근·보고량을 설명하는 통계이며 점수가 아니다. recall 결과 전달이 실제 절차 사용이나 성공을 뜻하지 않고, 익명 집계는 고유 인원이나 타인 여부를 뜻하지 않는다. 작성자 본인의 사용은 다른 계정의 성과 수치와 점수에서 제외한다.

공개 leaderboard는 기간별 rank, opt-in handle, 합계만 반환한다. 공개 rank는 opt-in 사용자 cohort 안에서 계산하며 동점은 같은 경쟁 순위를 쓴다. 개인 dashboard는 서버 순위 자격이 유효한 전체 양수 점수 계정 기준의 본인 순위를 반환하고 이 값은 공개하지 않는다. 공개 응답에 개별 memory 통계, reader identity, 비공개 계정 정보는 넣지 않는다. 세부 테스트는 PROFILE-05~08이다.

## 검증 지표

점수 엔진은 신규 기여와 성공 사용을 재현할 수 있어야 한다. 임의의 client-reported balance, 요청당 점수, 단순 read count로 계산하는 구조는 불합격이다. 기여자 한 명이 자기 계정을 여러 device로 연결해 자기 memory를 반복 조회해도 점수는 변하지 않아야 한다.

SCORE-15~24와 DB-07~17이 로그인 조건, anonymous no-score, self-use, receipt replay, quotas, version author attribution, reversal, opt-in visibility, event replay를 검증한다.
