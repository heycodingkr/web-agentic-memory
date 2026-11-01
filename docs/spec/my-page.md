# MYPAGE: 로그인 후 내 기여·메모리·성과 대시보드

2026-11-01 기능 명세와 구현 현황. 로그인 후 My Page에서 본인 기여의 대기/처리/반영 결과, 내 메모리, 랭킹·포인트와 다른 이용자의 호출 통계를 확인한다. durable owner API는 worker 상태·outbox 시각·귀속된 memory/version으로 진행/반영 DTO를 구성한다. 실제 HTTPS 이메일 계정, 운영 worker, 운영 게시/호출 전체 여정은 별도 수용 단계다. 테스트 계약은 [my-page.md](../test/my-page.md), 실제 화면과 검증 결과는 [UI 검증 기록](../design/my-page-ui-qa.md).

집계 정의는 [usage-analytics.md](usage-analytics.md), 점수 지급은 [score.md](score.md), 개인/공개 순위는 [profile.md](profile.md), 인증은 [email-auth.md](email-auth.md)를 따른다. 이 명세는 이들을 로그인 이후 사용자 경험으로 연결한다.

## 현재 코드에서 확인한 범위

| 항목 | 현재 구현 | 보완할 부분 |
|---|---|---|
| 로그인 후 진입 | 일반 인증 성공 시 `/dashboard` 자동 이동, 상단 `My Page`·계정 메뉴·로그아웃 | 실제 운영 이메일/HTTPS 여정 |
| 내 기여 | durable owner 응답을 받아 검색·상태 필터·6행 UI 페이지·owner 상세를 제공 | 서버 cursor/filter, 대량 계정 조회 측정 |
| 기여 상태 | owner DTO가 queue/work state, 다음 재시도 시각, outbox 최신 시각을 읽는다. worker/provider 원문은 노출하지 않고 고정 reason code를 사용한다 | 운영 로그의 실제 상태 변화·시계 일치 수용 |
| 실제 반영 | attribution과 immutable version을 확인해 new/updated/supported, memory/version ID와 현재 공개 상태를 owner-only 응답에 연결한다. legacy approved는 `unknown`으로 남긴다 | HTTPS 배포 뒤 결과 반영과 공개 memory 화면 대조 |
| 내 메모리 성과 | author analytics 기반 My memories/Memory impact 탭, 7/30/90일·일/주·표 대안 | 전용 owner 목록, 현재 공개 상태와 과거 버전 구분·제출 건 연결 |
| 랭킹/점수 | 총점·전체 순위·공개 순위 문맥, 별도 원장 탭·더 보기, 게시/성공/회수 사유 표시 | 서버 요약 집계와 실제 게시→지급 운영 여정 |
| 다른 사람의 호출 | 다른 계정 recall·상세·보고 성공, 익명 요청을 분리한 카드·차트·표. 요청수와 고유 인원/포인트 구분 | 실제 A/B 계정 호출·outcome과 운영 집계 대조 |
| 내 API 사용량 | 하단 접기/펼치기 영역의 quota·30일 요청/거부 합계 | 운영 데이터·한도 대조 |
| 오류/인증 만료 | 패널별 로딩/오류·정상 값 유지, 429 재시도 제한, 현재 401 개인 정보 제거·이전 요청 무시 | 운영 장애·계정 전환 여정 |

확인 소스: [dashboard](../../web/src/routes/dashboard/+page.svelte), [contribute](../../web/src/routes/contribute/+page.svelte), [owner DTO](../../web/src/lib/api.ts), [worker 결과](../../crates/wam-server/src/durable/worker.rs), [analytics](../../crates/wam-server/src/application/analytics.rs). 로컬 더미 API를 실제 브라우저로 렌더링·검사했다. 실제 로그인 계정의 운영 화면 검증은 별도다.

## 진입과 정보 구조

- 기본 계정 홈 이름은 **My Page**, URL은 기존 `/dashboard`를 유지한다. 신규 가입과 기존 로그인 모두 인증 성공 후 바로 이동한다. MCP 승인/CLI 기기 연결 중 시작한 인증은 해당 내부 요청으로 먼저 복귀한다.
- 상단에서 로그인 전에는 `Log in`, 로그인 후에는 `My Page`와 계정 메뉴를 제공한다. 계정 메뉴에 프로필 설정·연결 기기·로그아웃을 둔다. 이메일로 로그인해도 GitHub 세션이라고 표시하지 않는다. production에 demo token 입력 UI를 노출하지 않는다.
- 공개 `/memories`는 모두의 라이브러리다. 로그인한 사람의 비공개 제출 현황을 공개 라이브러리 필터로 대체하지 않는다.

아래는 목표 화면 구성이다. 현재 구현의 실제 캡처와 남은 차이는 [UI 검증 기록](../design/my-page-ui-qa.md)을 따른다.

```text
My Page                       [Submit a contribution] [Refresh] [Account ▾]
[Total points] [Overall rank] [Published memories] [Pending] [Needs your action]

[My contributions] [My memories] [Memory impact] [Point history]

My contributions
[All] [Pending] [Published] [Supported] [Needs action] [Not accepted]
제출 항목 / 서비스·작업 / 현재 상태·기다리는 이유 / 상태 시각 / 다음 행동

My memories
메모리 제목 / 현재·과거 버전 / 공개 상태 / 기간 내 조회·성공 보고 / 점수 / 상세

Memory impact
[Last 7 / 30 / 90 days] [Day / Week] [UTC 범위] [마지막 조회 시각]
다른 계정의 recall 결과 전달·상세 조회 / 익명 집계 / 성공·부분·실패 보고

Point history
지급·회수 사유 / 관련 메모리·버전 / ±포인트 / 일시

[접기/펼치기: My agent API usage — 본인 호출량과 API 한도]
```

요약 카드는 전체 계정 범위에서 서버가 집계한다. 현재 페이지나 최근 5개만 더해 총수를 만들지 않는다. `Published memories`는 **본인이 현재 공개 버전의 author인 고유 memory 수**이며 버전 개수/모든 제출 수와 다르다. Pending은 아직 결과가 확정되지 않은 제출, Needs your action은 동의 재검토 등 실제 본인 조치가 필요한 제출이다. 단순 운영자 검토 대기를 사용자가 승인해야 하는 상태로 표시하지 않는다.

## 기여와 반영 상태

하나의 제출에는 접수/처리 상태와 반영 결과가 있다. 공개 memory의 현재 활성 여부는 별도의 축이다. 제출 상태 `approved`만 보고 공개 완료나 +20을 추정하지 않는다.

| 목표 상태 | 사용자 표시 | 근거·의미 | 다음 행동 |
|---|---|---|---|
| queued | 접수됨 · 처리 대기 | 접수 transaction 완료, 아직 처리 시작 근거 없음 | 상세/새로고침, 정책상 가능하면 철회 |
| processing | 검증 중 | 현재 generation의 유효한 작업 lease 등 실제 처리 근거 | 현재 단계 확인, 기다리기 |
| retry_scheduled | 처리 지연 · 재시도 예정 | 서버가 일시 지연/재시도를 기록 | 확인된 다음 시도 시각 표시, 없으면 예정 미정 |
| needs_review | 추가 검토 대기 | 자동 결과를 확정하지 못해 검토 필요 | 대기 설명. 사용자에게 운영자 승인 버튼을 제공하지 않음 |
| consent_required | 공유 동의 확인 필요 | 현재 payload/동의로 진행할 수 없음 | 내용을 확인해 동의·재제출. 동의 없이 자동 진행 금지 |
| published | 반영됨 · 새 메모리/개선 버전 게시 | 검증된 게시 transaction과 해당 contribution의 author 귀속/버전 연결 확인 | 실제 memory/version 열기, 기간 통계/점수 내역 |
| supported | 반영됨 · 기존 메모리 보강 | 기존 메모리에 support로 반영된 결과 확인 | 해당 메모리 열기. 새 author/+20이라고 표시하지 않음 |
| rejected | 반영되지 않음 | 최종 거절 결과와 공개 가능한 사유 | 수정해 새 기여 제출. 원 접수 이력은 유지 |
| withdrawn | 철회됨 | 본인의 철회 확정 | 귀속/공개/점수에 미치는 실제 결과 안내 |
| expired | 접수 만료 | 보관/처리 정책상 payload 사용 불가 | 필요하면 다시 제출 |
| unknown / legacy approved | 결과 확인 필요 | 과거 approved만 있고 게시/support 연결을 확인할 근거 없음 | 결과 미확인 표시. 처리중/반영됨으로 꾸미지 않음 |

이 표는 owner API가 화면으로 투영하는 상태 계약이다. 현재 구현은 durable work 상태·outbox 시각과 contribution의 attribution 및 immutable memory version을 조합한다. queued/processing/예산 대기·재시도와 근거가 연결된 published/supported/rejected 결과는 서버 기록에서 파생하고, provider 원문은 반환하지 않는다. `approved`만 남은 legacy 접수는 결과를 `unknown`으로 둔다. 아직 durable 근거가 없거나 운영 경로에서 확인되지 않은 상태는 클라이언트가 추정하지 않는다. 구현과 운영 검증 범위는 아래 상태표와 [테스트 실행 기록](../test/my-page.md)에 구분한다.

- 한 행에 제목 또는 정규화 서비스·작업, 접수 ID, 접수 시각, 최근 상태 변경 시각, 현재 상태, 기다리는 이유, 가능한 다음 행동을 표시한다. 상세에는 실제로 기록한 단계 이력만 보여준다. 과거 시각이 없으면 미기록으로 표시한다.
- `반영됨`은 공유 memory catalog에 반영됐다는 뜻이다. 모든 에이전트가 읽었거나 작업에 적용했다는 뜻은 아니다. 게시 후 조회 0건이어도 게시 완료 상태는 유지한다.
- `processing` spinner는 실제 처리 상태에만 사용한다. 모든 pending에 무한 spinner·진행률·queue 순번·완료 예상 시간을 만들어 넣지 않는다. 다음 재시도 시각은 완료 예정 시간이 아니다.
- 대기/지연에는 경과 시간과 마지막 상태 시각을 보여준다. 30분 이상 상태 변화가 없으면 `Taking longer than usual` 안내를 추가하되 거절/실패로 단정하지 않는다. 이 30분은 UX 안내 기준이며 처리 SLA가 아니다.
- 모델 예산·외부 서비스·재현 검증 등 내부 원인을 사용자에게 필요한 수준의 고정 사유로 정리한다. provider key, raw 오류/프롬프트, lease holder, 다른 이용자 작업, 운영자 credential은 노출하지 않는다.
- 반영 이후 버전이 교체/비활성화되면 `당시 반영 결과`와 `현재 공개 상태`를 함께 보여준다. 과거 게시를 다시 pending으로 바꾸거나 사라진 링크만 남기지 않는다.
- 신규/개선은 해당 immutable version으로 연결한다. 지원되는 공개 상세가 현재 버전만 보여주면 “현재 버전 열기”와 해당 버전 ID를 구분한다. 비활성 버전은 본인용 안전한 이력에서 확인하며 공개 조회 통제를 우회하지 않는다.

## 내 메모리·호출 통계

내 제출은 모든 접수 건이고, 내 메모리는 내가 version author로 귀속된 결과다. pending·반려·support 기여를 내 author memory로 잘못 추가하지 않는다. 기존 메모리 보강 결과 링크는 내 제출에서 계속 볼 수 있지만, 그 이유만으로 다른 author의 비공개 분석 권한을 받지 않는다.

| 지표 | 정의 | 표시하지 않을 해석 |
|---|---|---|
| Other-account recall hits | 본인을 제외한 로그인 계정에 내 version이 recall 응답으로 전달된 횟수 | 고유 이용자 수, 실제 절차 적용/성공 |
| Other-account detail views | 다른 로그인 계정에 내 version의 상세/expand가 반환된 횟수 | 실제 작업 성공 |
| Anonymous recall / detail | 신원을 구분하지 않는 익명 요청 집계 | 서로 다른 사람 수, 확실한 타인 수. 로그아웃한 본인의 요청일 수도 있음 |
| Reported success / partial / failure | 해당 receipt의 실제 사용 목록에 있는 version에 대한 유효한 outcome 보고 | 자동 독립 검증된 모든 실제 작업의 성공률, 포인트 지급 횟수 |
| My agent API usage | 내 계정이 API를 호출한 요청/units/거부량 | 남이 내 메모리를 호출한 수, 내 포인트 |

API가 reader ID를 저장하지 않으므로 **고유 이용자 수는 이 범위에서 제공하지 않는다.** 다른 계정+익명 값을 합산하면 반드시 `전체 요청 집계`라고 표시하고, 타 계정/익명 분해 값도 확인할 수 있어야 한다. 알 수 없는 “다른 사람 100명”, 근거 없는 성공률이나 성장률은 표시하지 않는다.

기본 기간은 최근 30 UTC 날짜, 선택은 7/30/90일과 day/week다. 현재 analytics API의 90일 상한/보관 범위에 맞춘다. `전체 기간`이라는 이름으로 90일 데이터를 평생 누계처럼 표시하지 않는다. 날짜 경계·월요일 시작 주간 집계와 합계는 [기존 analytics 계약](usage-analytics.md)을 따른다. 차트/표/기간 합계는 같은 데이터이고, 활동이 없는 내 메모리도 0건으로 목록에 남는다. 단, API가 실패하면 0건이라고 표시하지 않는다.

## 순위·포인트

- 총점은 서버 원장 기준의 계정 전체 점수다. 선택한 통계 기간의 요청수나 현재 ledger page 합계로 대체하지 않는다. production은 demo 기초 점수를 사용하지 않는다.
- 전체 순위는 현재 순위 자격이 있는 양수 점수 계정 기준, 공개 순위는 opt-in cohort 기준이다. 공개 opt-out과 점수 0으로 아직 순위가 없는 상태를 구분한다. 현재 순위만으로 지난주 대비 상승 화살표를 만들지 않는다.
- 새/개선 version의 검증된 게시에 +20, 다른 인증 계정의 적격 성공 보고에 +2를 기존 정책/멱등성과 일일 상한에 따라 지급한다. queued, 단순 approved, support, 조회, 익명 사용, 본인 사용만으로 점수를 약속하지 않는다.
- 원장에는 `Memory published +20`, `Successful use by another account +2`, `Publication/use credit reversed -N`처럼 지급/회수 사유와 연결된 memory/version·시각을 표시한다. 알 수 없는 event만 중립적인 설명으로 남긴다. 모든 비성공-use event를 같은 `Point adjustment`로 처리하지 않는다.
- 재전송·재시도·재조회는 점수를 중복 지급하지 않는다. 회수는 원 지급을 지우지 않고 reversal을 보여준다. 성공 보고가 많아도 정책상 상한으로 일부만 포인트가 될 수 있음을 설명한다.

## 필요한 데이터/API 변경

아래는 후속 구현 계약이다. 이미 이 필드나 신규 endpoint가 있다고 보고하지 않는다. 기존 필드/권한은 유지하고 versioned/additive 방식으로 확장한다.

| 대상 | 현재 | 필요한 변경 |
|---|---|---|
| `GET /v1/me/dashboard` | total_points/overall_rank/public_rank | 전체 범위의 현재 author memory 수·pending·needs-action 수, 명확한 순위 상태, 조회 시각. 같은 요약은 일관된 repository snapshot에서 계산 |
| `GET /v1/me/contributions` / `{id}` | durable adapter가 반환하던 접수 ID·URL/task/actions·status·submitted_at | additive owner-only `progress`(stage·reason code·updated_at·next_attempt_at), 실제 `attributions` 기반 `reflection`(new/updated/supported·memory/version·reflected_at·availability). reason code는 `model_budget`, `temporary_processing_failure`, `additional_review`, `not_accepted`만 사용자 문구로 해석. 안정적인 filter/sort/cursor는 별도 후속 |
| `GET /v1/me/memories` | 전용 목록 없음 | 신규 owner 목록. author 귀속 기반으로 current/history를 구분하고 공개 상태·관련 접수·버전·선택 기간 집계 연결. 소유권/전체 수를 usage가 있는 행만으로 추정하지 않음 |
| `GET /v1/me/memories/analytics` | 최근 30일, 최대 90일 day/week 집계 | 정의 재사용, UI에서 from/to와 7/30/90일 선택 연결. 현재 version/author의 권한과 기간을 지킴 |
| `GET /v1/me/score-events` | owner-only cursor 원장 | 의미 있는 event 표시와 접수/메모리 문맥 연결. 모든 조회량이 포인트가 된다는 계산 금지 |

progress는 durable worker 상태와 contribution ID를 대상으로 기록된 outbox 이벤트에서 읽는다. `processing` 진입은 worker claim 이벤트로 시각을 남기며 다음 재시도 시각은 `not_before`에서 제공한다. reflection은 같은 owner/source contribution의 attribution과 immutable version이 있을 때만 반환한다. 과거 기록, 수동 approve, 삭제/철회 후 소유권이 끊긴 attribution에는 게시 결과를 추측하지 않고 `unknown`과 nullable 시각을 허용한다. version availability는 `current`, `superseded`, `retired`, `not_current`로 구분한다. owner API에 관리자 review token을 요구하거나 공개 worker/admin 상태 전체를 재사용하지 않는다. 일반 기기 token의 scope를 넓혀 account 관리 쓰기를 허용하지 않는다.

## 갱신·오류·빈 상태

- 제출 성공 시 receipt를 내 기여 목록에 넣고 요약을 재조회한다. 요청 중 timeout이면 같은 nonce/멱등 계약으로 확인하여 이중 제출을 막는다.
- 해당 화면이 보이고 pending이 있을 때 진행/요약만 30초 간격으로 갱신한다. 전체 analytics/원장을 매번 polling하지 않는다. terminal·숨겨진 탭·로그아웃에는 polling을 멈춘다. 수동 새로고침도 제공한다.
- 429는 Retry-After를 지키고 연속 요청을 중단한다. 503/네트워크 실패는 실패 패널에서 재시도를 안내한다. last-known 값이 있으면 갱신 실패/조회 시각을 붙이고 0으로 덮어쓰지 않는다.
- 요약·기여·메모리·통계·원장은 독립적으로 로딩/오류를 처리한다. analytics 하나가 실패해도 정상 기여 상태와 점수는 볼 수 있다. 단, **현재 계정의 401/계정 전환은 모든 개인 데이터를 제거**한다. 이전 요청의 늦은 응답은 새 계정/최신 조회를 덮어쓰지 않는다.
- 새 계정은 “아직 기여가 없습니다”와 첫 제출 CTA를 본다. 기여가 pending이면 “검토 중인 기여가 있습니다”, 게시 후 조회가 없으면 “아직 조회가 없습니다”로 구분한다. 조회 실패를 빈 계정으로 안내하지 않는다.
- 상태는 색뿐 아니라 텍스트/아이콘으로 구분하고, 수동 조회와 자동 변경은 접근성 상태 알림으로 제공한다. 차트 표 대안·모바일 리스트·키보드 필터·초점 유지를 지원한다.

## 요구사항과 테스트 추적

| ID | 기능 | 테스트 |
|---|---|---|
| F-MYPAGE-01 | 로그인 후 My Page 기본 진입·상단 메뉴·내부 승인 복귀 | MYPAGE-01, MYPAGE-02 |
| F-MYPAGE-02 | 정확한 전체 요약·신규 계정 빈 상태 | MYPAGE-03, MYPAGE-04 |
| F-MYPAGE-03 | owner 인증/권한·타 계정 격리 | MYPAGE-05, MYPAGE-06 |
| F-MYPAGE-04 | 접수/처리/지연/검토/동의/거절의 실제 진행 상태 | MYPAGE-07~13 |
| F-MYPAGE-05 | 신규·개선 게시와 support 반영 구분 | MYPAGE-14~17 |
| F-MYPAGE-06 | 철회·만료·버전 현재 상태와 이력 | MYPAGE-18, MYPAGE-19, MYPAGE-21 |
| F-MYPAGE-07 | 목록 필터/페이지·전체 수·pending 갱신 | MYPAGE-20, MYPAGE-22 |
| F-MYPAGE-08 | 전체/공개 순위·미순위·공개 선택의 구분 | MYPAGE-23, MYPAGE-24 |
| F-MYPAGE-09 | +20/+2·중복/상한/회수 및 원장 사유 | MYPAGE-25~28 |
| F-MYPAGE-10 | 다른 계정/익명/본인 조회와 outcome을 의미대로 분리 | MYPAGE-29~31 |
| F-MYPAGE-11 | 기간·버전·author 귀속과 과거 버전 통계 | MYPAGE-32, MYPAGE-33 |
| F-MYPAGE-12 | 본인 API 사용량과 저자 성과 분리 | MYPAGE-34 |
| F-MYPAGE-13 | 부분 장애/인증 만료/응답 교체·집계 지연 | MYPAGE-35~37 |
| F-MYPAGE-14 | 모바일·접근성·표 대안 | MYPAGE-38 |
| F-MYPAGE-15 | PostgreSQL/worker 재시작·동시성·실제 배포 전체 여정 | MYPAGE-39, MYPAGE-40 |

## 구현 순서와 현재 판정

P0는 기본 My Page 진입, 본인 제출의 실제 진행/반영 연결, 요약·기존 순위/점수/성과의 명확한 표시, owner 격리와 오류 처리다. 메모리 목록/결과 projection → 기여 목록/상세 → 카드·내비게이션 → 통계/원장 연결 → 예비배포/실운영 여정 순서로 구현한다. 미래의 고유 reader 추적·랭킹 이력·메일/push 알림은 이번 범위에 추가하지 않는다.

진행/게시 결과의 owner 조회 공백은 #162, UI 개선·시각 검증은 #164, 부분 장애 수정은 #163으로 추적한다. UI는 기존 API와 optional 결과 필드로 동작하고, 상세 진행 상태는 opt-in 로컬 더미 데이터에서 검증했다. 서버가 요약 집계를 보내지 않으면 author version 수로 이름을 바꾸고, 완전한 반환 목록일 때만 pending/action 수를 계산한다. 목표 기능 전체와 운영 MYPAGE-40은 완료 판정하지 않는다.
