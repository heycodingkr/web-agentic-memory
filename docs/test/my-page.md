# MYPAGE: My Page 기능·운영 수용 테스트

2026-11-01 명세와 검증 결과. 기능·화면 계약은 [my-page.md](../spec/my-page.md), 인증/가입 여정은 [email-auth-production.md](email-auth-production.md)와 연결한다. MYPAGE-01~40은 전체 목표 케이스다. 화면은 Chrome 데스크톱/모바일에서 더미 API로 검사했고, owner progress/reflection은 별도 synthetic durable worker 통합 검사 대상이다. 아래 실행 결과와 [캡처](../design/my-page-ui-qa.md)에 범위를 기록했다. 실제 이메일·production worker·production owner 결과는 로컬 검증으로 완료 판정하지 않는다.

## 확인한 기존 근거와 공백

| 근거 | 기존 범위 | 새 명세에서 확인할 차이 |
|---|---|---|
| [dashboard 페이지](../../web/src/routes/dashboard/+page.svelte) | My Page 진입, 요약·탭·검색/필터/페이지/상세, author impact·원장, 접을 수 있는 본인 API 사용량 | 실제 서버의 진행/게시 결과 연결·전체 집계·운영 수용 |
| [memory usage tests](../../crates/wam-server/tests/memory_usage.rs) | owner analytics·집계 계약을 검사하는 코드 | 사용자 화면의 다른 계정/익명/본인 구분과 기간·버전 설명 |
| [application lifecycle](../../crates/wam-server/tests/application_lifecycle.rs) | 검증된 게시 +20, 적격 성공 사용 +2, 검수·귀속 흐름 | receipt→화면의 반영 상태→결과 memory/version→원장까지 연결 |
| [scoring](../../crates/wam-server/tests/scoring.rs) / [PostgreSQL](../../crates/wam-server/tests/postgres_application.rs) | 점수·영속 transaction 관련 코드 | My Page read model의 재시작·동시성·오류/소유권 처리 |
| [dashboard failures](../../web/e2e/dashboard-failures.spec.ts) | 401 정보 제거·이전 요청 무시·429/503에서 정상 패널 유지 | 실제 배포의 장애/계정 전환 여정 |

로컬 durable owner progress API는 synthetic worker/replay 통합으로 검증한다. UI fixture 검증만으로 production 배포 완료를 판정하지 않는다. 실제 이메일·worker 운영 여정은 #157/#159, owner read model과 게시 결과 연결은 #162에서 추적한다.

## 역할·데이터·판정 방법

- A: 메모리 기여자. 새 계정/기여 없는 경우와 점수가 있는 경우를 분리한다.
- B: 다른 인증 계정. A 메모리를 recall/expand하고 유효한 `used_version_ids`로 outcome을 보고한다.
- C: 다른 기여자·IDOR/owner cursor 검사용 계정. A의 원장/진행 상태/개인 분석은 볼 수 없어야 한다.
- Anonymous: 로그인 없이 공개 메모리 호출. 익명 사용자가 A 본인인지 구별할 수 없다.
- Worker/operator: 검증·재현·게시/검토 상태를 만드는 별도 역할. A가 관리자 token을 받아 상태를 직접 승인하는 것으로 테스트하지 않는다.

예비배포에 queued, leased/processing, retry, needs_review, consent_required, rejected, published(new/updated), supported, withdrawn, expired, legacy approved, superseded/retired 결과를 준비한다. API를 통해 상태가 만들어지는 정상 fixture를 우선 사용하고 테스트 준비를 위한 내부 조작은 예비배포에만 제한한다. 운영에서는 소유한 QA 계정·허용된 워크플로만 사용하며 일반 사용자 데이터를 수정하지 않는다.

자동 검사는 공개 API/화면의 관찰 결과와 repository/원장 상태를 비교한다. 표의 `자동/예비`는 단위·통합·브라우저와 동일 artifact의 예비배포 검증을 의미한다. `운영`은 실제 HTTPS origin·메일 인증·실제 worker/consumer 경로다. mock API 화면만으로 운영 수용을 통과시키지 않는다.

## 1. 진입·요약·본인 정보

| ID | 우선 | 환경 | 절차 | 합격 기준 |
|---|---|---|---|---|
| MYPAGE-01 | P0 | 자동/예비·운영 | 일반 이메일 신규 가입/기존 로그인 완료, MCP/CLI 연결 중 로그인도 각각 실행 | 일반 인증은 `/dashboard`로 자동 이동. 연결 중 인증은 원래 내부 승인 요청으로 복귀. OTP 완료 화면에서 버튼을 찾아야 하거나 외부 return URL로 나가지 않음 |
| MYPAGE-02 | P0 | 브라우저·운영 | 로그인 전후 상단 메뉴, My Page 직접 URL, 계정 메뉴/로그아웃 확인 | 로그인 후 `My Page` 진입 가능. 프로필/연결 기기/로그아웃 탐색 가능. 이메일 계정을 GitHub라고 표시하지 않으며 production demo token UI 없음 |
| MYPAGE-03 | P0 | 자동/예비·운영 | 기여 없는 신규 A로 첫 진입 | 총점 0·게시 메모리/대기 0, 순위는 아직 없음. “기여 없음”과 첫 제출 CTA. 전체 공개 라이브러리의 내장 예시를 내 메모리로 세지 않음 |
| MYPAGE-04 | P0 | 자동/예비 | A에 여러 상태의 접수 50개와 동일 메모리의 복수 버전을 준비하고 첫 페이지 열기 | 요약은 전체 범위, 표시된 5/20행 합계가 아님. 게시 수는 현재 공개 version의 A author인 고유 memory 수, pending·본인 조치 필요 수의 정의가 일치 |
| MYPAGE-05 | P0 | API·브라우저·운영 | 비로그인/만료 credential/부족한 scope로 My Page API·직접 URL 접근 | private API 401/403, 화면은 재인증/권한 부족 안내, 개인 데이터 미표시. dev token을 production 대체 인증으로 노출하지 않음 |
| MYPAGE-06 | P0 | 자동/예비 | A/C로 contribution ID, memory author 목록, analytics 대상, score cursor를 교차 입력 | 본인 것만 반환. 타 계정과 존재하지 않는 owner ID/cursor는 식별에 유리한 응답 차이를 만들지 않음. 임의 account_id/author_id로 범위 변경 불가 |

## 2. 접수·대기·반영 결과

| ID | 우선 | 환경 | 절차 | 합격 기준 |
|---|---|---|---|---|
| MYPAGE-07 | P0 | API·브라우저·운영 | A가 동의하고 새 기여 제출, 응답 지연/동일 nonce 재시도 포함 | receipt가 My contributions에 나타나고 pending 증가. 제출 완료를 게시/사용 성공으로 표시하지 않고 점수 증가 없음. 재시도로 접수가 중복되지 않음 |
| MYPAGE-08 | P0 | 자동/예비 | worker가 아직 처리하지 않은 queued 접수 조회 | “접수됨·처리 대기”, 접수/마지막 상태 시각과 다음 행동. 처리 근거 없이 검증중 spinner/진행률/queue 순번/완료 ETA를 만들지 않음 |
| MYPAGE-09 | P0 | 자동/예비 | 유효 lease의 processing과 lease 만료/이전 generation 상태를 각각 조회 | 유효 작업에만 “검증 중”. 오래된 worker 응답이 최신 결과를 덮어쓰지 않고, 만료 lease를 계속 처리 중이라고 확정하지 않음 |
| MYPAGE-10 | P0 | 자동/예비 | 일시 지연·다음 재시도 시각 있음/없음·30분 무변화 상태 준비 | 지연/재시도 안내와 확인된 시각, 없으면 미정. 다음 시도를 완료 시간으로 표현하지 않음. 오래 걸림 안내는 실패/거절 판정이 아니며 raw provider/비용/다른 작업 미노출 |
| MYPAGE-11 | P0 | 자동/예비 | 검토가 필요한 접수와 operator가 검토하는 상태 조회 | “추가 검토 대기”, 기다리는 이유. 사용자에게 운영자 승인 버튼/credential을 요구하지 않고 Needs your action을 잘못 증가시키지 않음 |
| MYPAGE-12 | P0 | 자동/예비 | consent_required/동의 철회 상태 조회 후 재제출 경로 실행 | 본인 조치 필요 표시와 검토/재제출 안내. 동의 없이 게시/기여 재개 불가. 과거 payload가 삭제됐으면 없는 내용을 자동 복원하지 않음 |
| MYPAGE-13 | P0 | 자동/예비 | 거절 및 민감한 provider/검수 내부 오류가 있는 접수 조회 | “반영되지 않음”, 허용된 고정 사유·수정 후 제출 안내. 내부 프롬프트/stack/key/민감 문자열 미노출, 접수가 사라지거나 approved로 변하지 않음 |
| MYPAGE-14 | P0 | 통합·예비·운영 | 새 기여를 검증·재현하여 신규 version 게시, A 화면 새로고침 | “반영됨·새 메모리 게시”, 정확한 memory/version 링크·시각. public lookup/author 귀속/원장의 +20과 일치. 조회가 0이어도 게시 상태를 pending으로 바꾸지 않음 |
| MYPAGE-15 | P0 | 통합·예비 | 같은 메모리의 개선 version 게시 후 이전/현재 목록 비교 | “반영됨·개선 버전”, 새 version으로 연결. 과거 version/점수/통계는 올바르게 남고 현재 게시 고유 memory 수를 이중 계산하지 않음 |
| MYPAGE-16 | P0 | 통합·예비 | 동일/보강 기여가 기존 메모리 support로 처리되는 경우 조회 | “반영됨·기존 메모리 보강”, 기존 memory로 연결. 신규 게시/+20/새 author/상대의 private analytics 접근권을 만들어내지 않음 |
| MYPAGE-17 | P0 | 자동/예비 | approved 문자열만 있고 결과 연결이 없는 legacy 접수 조회 | 결과 확인 필요로 표시. 게시/점수 지급/현재 처리 중을 추정하지 않음. 기록이 없는 과거 타임라인 시각을 만들지 않음 |
| MYPAGE-18 | P0 | 통합·예비 | pending 철회와 이미 게시된 기여의 귀속 철회를 각각 수행 | 반영 전 철회는 대기 제거·상태 갱신. 반영 후에는 실제 귀속/공개 유지·비활성화·회수 원장에 맞게 설명. 다른 독립 지원이 남은 memory를 작성자가 임의 삭제했다고 표시하지 않음 |
| MYPAGE-19 | P0 | 통합·예비 | 접수 만료·payload purge 후 목록/상세/새 제출 확인 | 만료 이력과 재제출 경로, 원문/payload 재노출 없음. 만료를 검증 성공·게시·사용 점수로 처리하지 않음 |

## 3. 목록·버전·갱신

| ID | 우선 | 환경 | 절차 | 합격 기준 |
|---|---|---|---|---|
| MYPAGE-20 | P0 | API·브라우저 | 상태/서비스/작업 필터, 정렬, cursor 이동, 새 접수 중 페이지 이동 | 본인 전체 목록에 접근 가능, 누락/중복 없는 안정된 페이지. 필터별 수와 전체 수를 구분하고 owner/filter가 다른 cursor를 재사용하지 않음 |
| MYPAGE-21 | P0 | API·브라우저 | 게시 결과 링크와 이후 superseded/retired/비공개 결과 상세 열기 | 당시 반영 결과와 현재 공개 상태를 함께 표시. 현재 버전과 당시 version을 혼동하지 않고 비활성 public 상세 보호를 우회하지 않음. 설명 없이 깨진 링크만 보이지 않음 |
| MYPAGE-22 | P0 | 브라우저 시계·예비 | pending 화면을 30초 유지, 숨긴 탭/terminal/로그아웃, 429 Retry-After·수동 조회 관찰 | 필요한 진행/요약만 bounded polling, analytics/원장을 매번 호출하지 않음. 숨김/종료/로그아웃에 중단, 429 동안 재요청 없음. 최신 상태가 이전 조회에 덮이지 않음 |

## 4. 순위·포인트·원장

| ID | 우선 | 환경 | 절차 | 합격 기준 |
|---|---|---|---|---|
| MYPAGE-23 | P0 | 자동/예비·운영 | 0점/양수점수·공개 opt-in/out의 조합으로 조회 | 아직 순위 없음과 비공개를 구분. opt-out이어도 양수 유효 계정의 본인 전체 순위는 확인 가능, public에는 노출하지 않음 |
| MYPAGE-24 | P0 | 자동/예비 | 동점·서로 다른 공개 cohort·과거 rank snapshot 없는 계정 조회 | 전체/공개 순위는 각 cohort의 동점 규칙대로 일치. 현재 점수만으로 과거 순위·상승/하락 화살표를 생성하지 않음 |
| MYPAGE-25 | P0 | 통합·예비·운영 | A 게시 +20 후 다른 B가 유효 receipt/used_version_ids로 성공 보고, 동일 요청 재시도 | 실제 게시에 +20, 적격 첫 성공에 +2, 각 한 번. A의 점수 카드·원장·해당 memory/version 귀속이 같은 결과를 가리킴 |
| MYPAGE-26 | P0 | 통합·예비 | 자기 사용·익명 조회·부분/실패·미사용 version·중복 보고·일일 상한 재현 | 조회수/성공 보고와 포인트를 분리, 부적격 동작에 +2 없음. 원장 수를 성공 보고 수로 가장하지 않고 다음 날 재전송으로 상한을 우회하지 않음 |
| MYPAGE-27 | P0 | API·브라우저 | 게시/타 계정 성공/반전 event를 포함한 원장 조회 | 사유·연결된 memory/version·±delta·시각이 구체적. 지급은 그대로 남고 회수는 별도 음수 event. 모든 +20을 알 수 없는 조정이라고 표시하지 않음 |
| MYPAGE-28 | P0 | API·브라우저 | 기본 20건을 넘는 점수 내역의 더 보기, 타인/없는 cursor 입력 | 멱등·안정된 이력, 본인 cursor만 허용. 총점은 현재 page 합계가 아니며 demo 기초 점수/선택 기간 요청수와 혼동하지 않음 |

## 5. 내 메모리와 다른 이용자의 호출

| ID | 우선 | 환경 | 절차 | 합격 기준 |
|---|---|---|---|---|
| MYPAGE-29 | P0 | 통합·브라우저 | B의 recall 1회, 별도 expand, explicit outcome을 각각 실행하고 A 통계 비교 | recall 결과 전달·상세·보고가 각각 의미대로 증가. recall/expand만으로 실제 절차 사용·성공·포인트를 생성하지 않음. 각 memory/version 귀속에 맞음 |
| MYPAGE-30 | P0 | 통합·예비 | 인증한 A의 자기 조회/성공 보고, 별도 익명 조회 비교 | 인증된 본인 사용은 외부 성과/사용 점수에서 제외. 익명은 익명 요청만 집계하고 확실한 타인의 사용/고유 인원이라고 표현하지 않음 |
| MYPAGE-31 | P0 | API·브라우저 | 같은 B의 반복 요청, 다수 익명 요청, 원문/reader 데이터 검사 | 다른 계정 요청수와 익명 수가 분리. 100회 요청을 100명으로 표시하지 않음. 이메일/계정/device/원본 URL/receipt/개별 reader 내역 없음 |
| MYPAGE-32 | P0 | API·브라우저 | 7/30/90일, UTC 자정/월 경계/월요일 시작 주, 잘못된 날짜/초과 범위 조회 | 일/주 표·그래프·기간 합계 일치, 범위 검증. 90일 데이터를 평생 누계로 표시하지 않고 조회 실패를 0일/0건으로 만들지 않음 |
| MYPAGE-33 | P0 | API·브라우저 | 내 author version 2개·현재 타인 version·support만 한 기여·저자 없는 내장 예시 준비 | author 귀속 범위만 private analytics. activity 0인 본인 memory도 남음. 과거 version 통계를 새 author/current version에 잘못 합치지 않음 |
| MYPAGE-34 | P0 | API·브라우저 | A의 다른 API 호출과 B의 A memory 호출을 독립적으로 증가 | My agent API usage와 Memory impact가 독립. API units/remaining은 포인트/수익/반영 수가 아니며 API 호출만으로 총점 변경 없음 |

## 6. 장애·인증·접근성·운영 여정

| ID | 우선 | 환경 | 절차 | 합격 기준 |
|---|---|---|---|---|
| MYPAGE-35 | P0 | 브라우저·예비 | analytics 한 곳 503/429, 다른 요약·기여·원장은 정상 응답 | 정상 패널은 표시, 실패 패널만 error/retry/as-of. 실패를 조회 0·점수 0·기여 없음으로 표시하지 않음. 전체 Promise.all로 정상 정보까지 버리지 않음 |
| MYPAGE-36 | P0 | 브라우저·예비 | 현재 계정의 401, A→C 계정 교체 중 A의 늦은 성공/401 응답 | 현재 인증 무효는 모든 개인 정보 제거·polling 중단. 이전 계정 응답/401이 새 계정 데이터나 세션을 덮지 않음. 순위/기여/원장/차트 모두 포함 |
| MYPAGE-37 | P0 | 브라우저·예비 | offline·잘못된 응답·집계 지연·빈 값·실제 0·오래된 last-known 값 비교 | 각각 구별된 상태/마지막 조회 시각. 무한 busy/불필요한 자동 요청 없음. 서버가 제공하지 않은 집계 시각/완료 시각을 만들지 않음 |
| MYPAGE-38 | P0 | 브라우저·운영 대표 조작 | 좁은 화면, 키보드로 메뉴/탭/필터/페이지/재시도, 표 대안·상태 알림 확인 | 상태는 색만으로 표현하지 않음. 차트/표 수치 일치, label/초점 유지, 가로 넘침·가려진 CTA 없음. 자동 갱신이 초점을 빼앗거나 알림을 반복 낭독하지 않음 |
| MYPAGE-39 | P0 | disposable PostgreSQL·예비 | worker/API 재시작·동시 lease/재시도·게시 transaction 실패·복원 후 owner 조회 | 진행/반영 귀속·version·원장 정합성 유지, 한 번만 게시/지급. rollback/실패 후 가짜 반영됨 없음. 원문/credential을 백업 증거나 owner 결과로 노출하지 않음 |
| MYPAGE-40 | P0 | 실제 운영 | 아래 A/B 전체 여정 실행 | 실메일 인증부터 기여 대기→실제 검증·게시→다른 계정 호출/보고→통계·원장·새로고침까지 독립 증거로 일치. fake worker/고정 성공 replay/mocked UI로 대체하지 않음 |

## 실제 운영 전체 여정 — MYPAGE-40

1. 배포 commit/artifact·origin·시각을 고정하고 A/B를 실제 이메일 코드로 로그인한다. A의 초기 본인 점수/순위·기여·메모리 기준값을 기록한다. 신규 0점 A면 최종 예상 점수는 첫 적격 흐름 기준 22다.
2. A가 허용된 공개 워크플로를 검토·동의하고 제출한다. My Page에 receipt와 “처리 대기”가 나타나는 것을 확인한다. 이때 새 게시/+20이 없어야 한다.
3. 실제 worker가 실제 model 및 독립 재현 검증을 수행한다. 검토/재시도면 실제 이유를 보여주며 운영자가 UI 상태만 반영됨으로 바꿔 통과시키지 않는다.
4. 신규/개선 version 게시 transaction을 확인한다. A의 항목이 정확한 memory/version의 “반영됨”으로 바뀌고 새 게시에 +20 원장이 한 번 생긴다. 현재 public 조회로 해당 결과가 노출되는지도 확인한다.
5. B가 실제 API 또는 연결된 MCP에서 A의 version을 recall하고 expand한다. A의 다른 계정 요청/상세 집계를 확인한다. 단순 조회 시 추가 포인트는 없다.
6. B가 유효 receipt에 실제 선택한 version을 명시해 성공 outcome을 한 번 보고한다. A에 성공 보고가 보이고 정책상 적격이면 +2 원장/총점이 반영된다. 똑같은 재보고는 중복 지급하지 않는다. 독립 oracle 없이 사용자 보고만으로 실제 작업 품질이 검증됐다고 확대하지 않는다.
7. A가 새로고침/재로그인해도 동일 기여·version·통계·점수가 유지되고, 로그아웃한 다른 브라우저가 A의 개인 현황을 읽지 못하는지 확인한다.

기존 공개 메모리 보강으로 판정되면 MYPAGE-16 흐름으로 기록하고 +20 신규 게시 검사를 통과한 것으로 대체하지 않는다. 실제 provider/replay/소유 테스트 계정이 준비되지 않으면 차단으로 기록한다.

## 구현·실행 순서와 결과 양식

1. 완료: durable owner progress/reflection read model과 권한·원장 lifecycle 검사 (local synthetic path).
2. 완료: 목록/상세·반영 링크·필터 및 My Page 요약/인증 진입의 UI·회귀 브라우저 검사.
3. 완료: 통계/점수 원장과 부분 장애·계정 만료 경로의 합성 브라우저 검사.
4. 남음: disposable PostgreSQL에서 새 owner projection을 worker 재시작/상태 전이와 교차 검증하고 예비배포 전체 상태 행렬을 실행한다.
5. 남음: 실제 운영 MYPAGE-01/02/03(승인된 QA 계정)/05/07/14/23/25/38/40 여정과 결과를 기록한다.

서버 lifecycle/owner projection 검사는 `application_lifecycle.rs`에 합성 worker 상태를 연결해 구현했다. `cargo test --locked --workspace`에서 이 suite를 포함해 통과했다. `web/e2e/my-page.spec.ts`는 데스크톱/모바일 14/14 통과했고, `pnpm --dir web run test:smoke`는 synthetic email OTP→제출→fixture worker 게시→버전 교체→점수→철회→삭제까지 1/1 통과했다. OTP는 격리 fixture의 fake sender에서만 읽으며 실제 이메일을 보내지 않는다. 장애/인증 회귀는 `playwright.my-page-regression.config.ts`, 이메일 로그인/로그아웃은 `playwright.email-auth.config.ts`로 실행했다. 이 결과는 운영 이메일이나 배포 worker 검증이 아니다.

| 케이스 | 환경·브라우저·시각 | 결과 | 관찰·증거 | 이슈 |
|---|---|---|---|---|
| MYPAGE-01/02 | 로컬 Chrome PC/모바일 및 4090 공개 Arise, 2026-11-01 | 지정 경로 통과 | mock 내부 승인 복귀 검사와 별도로, 실제 메일 가입/기존 로그인 후 My Page 자동 이동·새로고침 세션 유지·계정 메뉴 로그아웃을 확인. [직접 배포 증거](deployment-4090.md) 참조. 운영 CLI/MCP 승인 전체는 별도 | #157 |
| MYPAGE-03/17/20/27/28/32/34 | 로컬 더미 API, PC/모바일 | UI 범위 통과 | 빈 계정, legacy 결과 미확인·접수 시각 구분, 검색/필터/6행 페이지/상세, 기간 선택·원장 더 보기·별도 API usage | #164 |
| MYPAGE-08~16, 25 일부 | 로컬 합성 durable worker/replay | 서버 통합 검증 통과 | queued·model-budget wait/retry timestamp·published new+version·supported·rejected without reflection·owner isolation 및 적격 다른 계정 success-use +2를 dashboard/ledger와 대조. production worker/email 경로 미실행 | #162; 배포 #157/#159 |
| MYPAGE-08/11/14/16 | 로컬 Chrome desktop/mobile, preview fixture, 14/14 | 브라우저 검증 | 게시 memory link/version, model-budget wait explanation·next retry가 desktop/mobile에 표시. fixture screenshot은 local sample data | #162 |
| MYPAGE-35/36 | 로컬 mock API, PC/모바일 | 장애 시나리오 통과 | analytics 503에서도 246점/기여 유지. usage 429/503 정상 패널 유지, 현재 401 전체 제거, 늦은 401 무시, 재시도 제한 | #163 |
| MYPAGE-38 | Chrome 1440×1080/390×844 및 Pixel 7 | 자동·시각 검사 통과 | 네 탭 axe A/AA 위반 0, 가로 넘침 없음·키보드 탭 전환·실제 스크린샷 확인. 실제 보조기기 수용 별도 | #164 |
| MYPAGE-04~07/18/19/21~24/26/29~31/33/37/39/40 | server/database staging·운영 | 전체 수용 미실행 | 페이지 전체 범위 집계, cursor/대량 데이터, 전체 상태 행렬, hosted owner read model·실제 A/B 호출/보고·worker restart/운영 여정이 남음. 로컬 owner projection 통과는 대체 증거가 아님 | #162, #157/#159 |

실행 시 한 케이스 한 행으로 `통과/실패/차단/미실행`을 기록하고 API/web artifact, 실제 worker/provider/replay 구성 여부, QA 역할을 남긴다. 실제 사용자 이메일·인증 코드·token·다른 독자의 신원은 증거에 넣지 않는다. 소스상 필드/화면이 있다는 사실, 기존 fixture 통과, 실제 운영 통과를 구분한다.
