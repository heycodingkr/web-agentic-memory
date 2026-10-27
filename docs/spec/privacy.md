# PRIV: 데이터 최소화, 기여 동의, 삭제와 공개 범위

상태: 1차 상세 명세. consent storage, purge worker, account deletion은 미구현이다.

## 원칙과 범위

웹 에이전틱 메모리는 재사용 가능한 절차만 공유하고 사이트의 개인 콘텐츠는 수집하지 않는다. contribution은 로그인 사용자가 전송될 데이터와 목적을 확인하고 명시적으로 동의한 뒤에만 보낸다. 인증 식별자는 점수와 소유권에 필요한 metadata로 분리하며 public memory와 leaderboard에는 email, account ID, trace 원문을 노출하지 않는다.

## 동의 모델

Contribution consent는 기본 off다. 사용자가 동의하기 전에는 propose tool이 서버로 contribution을 보내거나 queue job을 만들지 않는다. 사용자는 `consent_version`, 시각, 사용자가 승인한 범위, action category를 서버에서 확인할 수 있다. consent를 철회하면 새 기여를 즉시 막고, 아직 처리 전인 queue item을 취소·삭제한다. 이미 게시된 공유 기록의 삭제는 개별 저자 버튼으로 hard-delete하지 않고, attribution 철회 및 남은 증거를 Gatekeeper Agent가 재평가해 merge 유지·갱신·retire를 결정한다.

Telemetry consent는 contribution consent와 별개다. 하나를 바꿔도 다른 동의가 암묵적으로 바뀌지 않는다. 사용자는 전송 preview와 예시 payload를 보고 아래 정보가 포함되는지 알 수 있어야 한다.

## 허용/금지 데이터

허용되는 최소 구조는 인증된 contributor 내부 ID, 안전한 URL route pattern, 폐쇄형 task category, 표준 action/result category, 동의 버전, idempotency nonce다. contributor ID는 API body에서 받을 수 없고 인증 middleware가 정한다. URL 전체는 처리 전에 정규화하며 query, fragment, userinfo, tracking, account/object ID는 저장하지 않는다.

수집하지 않는 데이터: 원문 prompt, 사용자가 입력한 텍스트, 페이지 본문/HTML, screenshot/video, DOM selector, 폼 내용, credentials, cookie, OAuth token, URL query, 검색어, 문서 제목, 실제 사용자 계정 정보, private browsing history. 고정 enum 외에 자유 텍스트가 필요한 경우 기본 비활성으로 하고 별도 preview/redaction/승인을 요구한다.

## 보관과 삭제

수신 payload는 G0~G3 처리에 필요한 최소 기간만 유지하고 versioned retention policy에 따라 purge한다. 대기/거절 contribution은 처리 상태와 근거 코드를 남기되 sanitized trace도 보관 기한 뒤 삭제한다. 승인된 메모리 version은 개인 콘텐츠가 없는 구조형 절차와 검증 digest만 남긴다. usage score event는 익명 사용 집계와 분리한다.

사용자가 계정을 삭제하거나 attribution을 철회하면 private dashboard 접근을 종료하고 device/session을 해지한다. contribution author link는 법률상 보존 의무를 제외하고 삭제/비식별 처리한다. memory에 다른 기여자의 독립 근거가 없다면 Agent는 memory를 재검증하거나 retire한다. security/legal takedown은 admin kill switch로 공개 recall에서 즉시 숨길 수 있고, 조치 이유를 최소 audit metadata로 남긴다.

## 공개 데이터와 점수

Leaderboard는 opt-in이다. 공개 값은 사용자가 고른 `public_handle`, 기간, 합계 점수로 제한한다. opt-out은 공개 목록에서 즉시 반영한다. profile lookup, contribution ID 추측, receipt ID, device ID, author email, memory 원문, 개별 consumer identity를 공개하면 안 된다.

익명 사용자는 승인된 public memory를 조회할 수 있다. 이 활동은 caller ID와 receipt ID가 없는 운영상 대략적인 집계에만 쓰며 작성자 귀속 receipt, outcome, score event, leaderboard 순위, contribution을 만들지 않는다. 로그인 사용의 성공 결과는 작성자와 다른 account여야 하고 receipt·재전송·quota 검사를 통과해야 한다. 점수 원장 식별자는 비공개이며 계정 소유자에게만 version/date 요약과 함께 노출한다.

## 요구사항과 테스트

| ID | 요구사항 | 합격 기준 | 관련 테스트 |
|---|---|---|---|
| F-PRIV-01 | Contribution defaults off | consent false면 network POST/job/DB row 0건 | PRIV-01~02, MCP-07, FLOW-03 |
| F-PRIV-02 | 명시 consent와 preview | 실제 API payload가 UI preview와 같고 consent version을 기록 | PRIV-02, PRIV-07, WEB-17 |
| F-PRIV-03 | URL/trace 최소화 | 금지 field가 API, DB, logs, catalog, MCP에 0건 | CORE-22~26, DB-03, DB-15, SEC-A |
| F-PRIV-04 | 철회 | pending queue 취소, 새 업로드 차단, in-flight 경계를 명확히 기록 | PRIV-03~04, FLOW-03 |
| F-PRIV-05 | retention | purge date 뒤 contribution payload가 제거되고 합법적인 audit metadata만 남음 | PRIV-05, DB-14 |
| F-PRIV-06 | 동의 분리 | telemetry toggle이 contribution consent를 바꾸지 않음 | PRIV-06 |
| F-PRIV-07 | public handle opt-in | opt-out user와 identity metadata는 leaderboard 응답에서 제외 | SCORE-08, SCORE-24, WEB-24 |
| F-PRIV-08 | anonymous no-score | 익명 조회가 user-linked receipt, success event, point, rank를 만들지 않음 | SCORE-15~16, DB-08, API-30, SEC-C |
| F-PRIV-09 | account deletion/attribution withdrawal | device 해지, private account data 제거, 남은 memory evidence 재판정 | PRIV-09, FLOW-07, DB-13~14, SEC-G |

보관 기간과 공개 집계 최소 그룹 크기는 [`../test/metrics.md`](../test/metrics.md)와 배포 정책에서 합의한다. 적용 법률과 retention 세부 사항이 바뀌면 이 명세와 privacy 화면을 함께 개정한다.
