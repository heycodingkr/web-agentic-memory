# 유저 플로우 테스트 설계 (L3 확장)

상태: FLOW-01~08 1차 초안. 자동화 코드와 실제 호스트 smoke는 아직 없다. 요구사항·기능 테스트 ID를 재사용한다.

목적: 기능 테스트와 호스트 스모크가 각각 못 보는 **한 사용자의 목표 달성 여정**을 끝까지 검증한다. 웹, 서버, MCP, 게이트키퍼를 가로지른다.

고객 계정 상태와 호스트 조합별 설치·recall 매트릭스는 [`customer-paths.md`](customer-paths.md)의 `CUST-01~06`에서 별도로 검증한다. 아래 FLOW 케이스만 통과해도 비회원/회원, Chrome/CLI/Arise 조합이 모두 검증된 것으로 보지 않는다.

## 1. 다른 문서와의 경계

| 문서 | 보는 것 | 예 |
|---|---|---|
| [`functional-tests.md`](functional-tests.md) | 컴포넌트 하나의 입출력 | `LINK-02`: poll이 디바이스 키를 한 번만 반환 |
| [`host-smoke.md`](host-smoke.md) | 실제 호스트에서 설치, 호출, 정리 | `SMK-02`: 호스트가 메모리 도구 5개를 인식 |
| [`customer-paths.md`](customer-paths.md) | 비회원·회원 및 Chrome/CLI/Arise 경로 행렬 | `CUST-01~06`: 복사·설치·연결·recall·권한·점수 |
| 이 문서 | 사용자 한 명이 목표를 이루는 여정 전체 | 링크 연결 → 첫 recall → 기여 → 점수 반영 |

플로우는 하위 테스트 ID를 재사용한다. 이미 있는 검증을 다시 쓰지 않고, 그 검증들이 이어졌을 때만 드러나는 문제(상태 전이, 컴포넌트 간 계약)를 본다.

## 2. 규약

- ID는 `FLOW-<두 자리 번호>`. 한 번 쓴 번호는 재사용하지 않는다.
- 합격은 독립 oracle(서버 상태, DB, 로그)로 판정한다. "화면에 성공이 보임"이나 "에이전트가 성공했다고 말함"은 증거가 아니다 (`docs/test/README.md` 원칙 4).
- 합성 데이터와 loopback 환경에서만 실행한다. 로그인은 테스트용 OAuth 대역을 쓴다 (`WEB-02`).
- 증거는 `target/e2e-logs/user-flows/<run-id>/`에 `docs/test/README.md` 5절 형식으로 남긴다.
- 코드 위치는 `tests/user-flows/`: 웹 UI 단위는 `web/`, 컴포넌트를 가로지르는 여정은 `journeys/`.

## 3. 플로우 기록 형식

```
### FLOW-NN 제목

| 항목 | 내용 |
|---|---|
| 역할 | 누가 (신규 사용자, 기여자, 소비자, 관리자 등) |
| 목표 | 여정이 끝났을 때 참이어야 하는 것 |
| 사전조건 | 시드, 계정, 설정 |
| 단계 | 번호 매긴 사용자 행동과 각 단계의 관측 지점 |
| 합격 기준 | 독립 oracle로 확인할 상태 |
| 관련 ID | WEB-xx, API-xx, SMK-xx, SEC-xx |
| 자동화 | tests/user-flows/... 경로 (없으면 "수동") |
```

## 4. 플로우 목록

| ID | 제목 | 역할 | 상태 |
|---|---|---|---|
| FLOW-01 | MCP 설치부터 첫 recall | 새 MCP 사용자 | 명세 초안 |
| FLOW-02 | 힌트로 브라우저 작업 완료 | 소비자 | 명세 초안 |
| FLOW-03 | 동의 후 기여 제출과 철회 | 기여자 | 명세 초안 |
| FLOW-04 | Gatekeeper 신규 메모리 판정 | 기여자·Gatekeeper | 명세 초안 |
| FLOW-05 | 오래된 메모리 교체와 복구 | 소비자·Gatekeeper | 명세 초안 |
| FLOW-06 | 성공 사용과 공개 점수 | 작성자·소비자 | 명세 초안 |
| FLOW-07 | 계정 삭제와 기여 귀속 철회 | 기여자 | 명세 초안 |
| FLOW-08 | 내 순위 확인, 프로필 작성, 공개 소개 링크 방문 | 기여자 | 명세 초안 |

### FLOW-01 MCP 설치부터 첫 recall

| 항목 | 내용 |
|---|---|
| 역할 | 처음 연결하는 MCP 사용자 |
| 목표 | 브라우저 조작 도구와 메모리 도구를 같이 연결하고 알려진 fixture 사이트 힌트를 조회한다 |
| 사전조건 | 공개 가이드 템플릿, loopback MCP/API, 합성 계정, 브라우저 fixture |
| 단계 | 1) /guide에서 호스트 선택 2) endpoint 설정을 복사·등록 3) OAuth/device flow 승인 4) status와 tools/list 확인 5) 브라우저 MCP도 별도 연결 6) fixture URL과 task로 recall |
| 합격 기준 | 가이드는 복사만 된 상태를 설치 완료로 표시하지 않는다. 두 도구군이 모두 노출되고, 알려진 fixture는 승인 힌트를 반환하며, 설정 문자열·로그에 토큰이 없다. |
| 관련 ID | WEB-09, MCP-01~03, MCP-10, MCP-16~24, SMK-01~14, SEC-A15 |
| 자동화 | tests/user-flows/journeys/FLOW-01 |

### FLOW-02 힌트로 브라우저 작업을 완료

| 항목 | 내용 |
|---|---|
| 역할 | 로그인한 에이전트 사용자 |
| 목표 | 승인 힌트를 참고해 fixture 사이트 초안을 저장하고 사용 영수증 결과를 정확히 기록한다 |
| 사전조건 | published URL/task 메모리, 연결된 test user, 독립 saved-oracle가 있는 로컬 에디터 |
| 단계 | 1) 에이전트에게 fixture 작업 요청 2) trace에서 recall 호출 확인 3) 필요한 경우 상세를 expand 4) 브라우저 도구로 초안 저장 5) 새 페이지 load에서 oracle 확인 6) 사용자 확인 후 receipt outcome 제출 |
| 합격 기준 | 정확한 canonical URL pattern·task의 승인 hint만 사용한다. 새 페이지의 title/body가 oracle과 일치하고 publish 행동은 없다. 서버는 로그인 소비자의 memory version에만 receipt를 묶고 outcome을 한 번만 반영한다. |
| 관련 ID | API-01~07, API-26~29, MCP-03~06, BEH-01~05, SMK-03~06, SCORE-15~24 |
| 자동화 | tests/user-flows/journeys/FLOW-02 |

### FLOW-03 동의 후 제한된 기여 제출과 철회

| 항목 | 내용 |
|---|---|
| 역할 | 새 사이트 경험을 보태려는 기여자 |
| 목표 | 기본 꺼짐 상태를 이해하고, 명시적으로 허용한 메타데이터만 보내며, 대기 자료를 철회할 수 있다 |
| 사전조건 | 합성 로그인 계정, sentinel corpus, fixture에서 관찰한 action trace |
| 단계 | 1) 기여 설정 기본값 확인 2) preview에서 전송 항목 확인 3) 별도 동의 후 propose/submit 4) 본인 dashboard에서 상태 확인 5) 아직 queue에 있는 기여를 철회 6) 서버·클라이언트 sink를 검사 |
| 합격 기준 | 동의 전에는 queue와 POST가 0이다. 동의 후에도 canonical URL pattern·고정 action/result 범주만 기록한다. 철회하면 보류 자료와 신규 전송은 제거되고, 이미 시작된 요청 경계는 기록한다. sentinel·원문 prompt·입력 본문·selector·전체 URL은 어디에도 없다. |
| 관련 ID | PRIV-01~08, MCP-07~09, API-20~25, WEB-15~17, SEC-A01~18 |
| 자동화 | tests/user-flows/journeys/FLOW-03 |

### FLOW-04 Gatekeeper가 새 메모리를 판정

| 항목 | 내용 |
|---|---|
| 역할 | 기여자와 Gatekeeper Agent, 예외 검토 관리자 |
| 목표 | 정상 기여는 안전 게이트와 재생 증거를 거쳐 판정되고, 불확실한 기여는 사람 검토로 보낸다 |
| 사전조건 | 두 번째 URL/task fixture, 기존 승인 version, 결정적 replay harness와 공격 입력 fixture |
| 단계 | 1) 동의한 기여 제출 2) dashboard에서 상태 전이 확인 3) 동일·개선·충돌·저신뢰 사례 각각 처리 4) 확신 부족 사례를 review queue에서 검토 5) 게시 catalog와 audit 기록 확인 |
| 합격 기준 | Gatekeeper의 결정에 policy version·reason code·evidence digest가 연결된다. 서버가 제출 문자열을 실행하지 않는다. approval/replay 전 candidate는 public catalog와 recall에 없으며, 사람이 보는 예외 화면은 최소 공개 정보를 사용한다. |
| 관련 ID | GK-20~48, DEDUP-01~18, API-20~25, WEB-16~24, SEC-B01~09, SEC-C01~10 |
| 자동화 | tests/user-flows/journeys/FLOW-04 |

### FLOW-05 오래된 메모리 교체와 복구

| 항목 | 내용 |
|---|---|
| 역할 | 소비자, Gatekeeper Agent, 관리자 |
| 목표 | UI 변경 뒤 stale hint를 갱신하거나 안전하게 빼고, 이상 변경에서 이전 유효본으로 복구한다 |
| 사전조건 | v1 메모리와 cache가 있는 fixture, 변경된 v2 UI, 고정 replay oracle |
| 단계 | 1) v1 힌트로 변경된 UI 작업 2) 도구 실패와 회복을 trace에 남김 3) 새 제출을 기존 memory와 비교 4) replay 결과로 supersede 또는 needs-review 결정 5) 필요하면 관리자 kill switch/rollback 6) client refresh |
| 합격 기준 | 실패한 힌트는 성공으로 보고하지 않는다. 승인된 새 version만 노출되며 이전 버전은 immutable audit에 남는다. rollback은 새 revision이고 다음 refresh에서 stale entry가 사라진다. |
| 관련 ID | API-09~12, MCP-11~13, DEDUP-13~18, GK-34~48, DB-10~16, BEH-05 |
| 자동화 | tests/user-flows/journeys/FLOW-05 |

### FLOW-06 성공 사용과 공개 점수

| 항목 | 내용 |
|---|---|
| 역할 | 메모리 작성자와 다른 로그인 소비자, 익명 방문자 |
| 목표 | 재현 가능한 성공 사용만 점수로 귀속하고 공개 동의한 표시명만 순위에 보인다 |
| 사전조건 | 서로 다른 합성 사용자 2명, opt-in/off 사용자, published memory version과 fixture task |
| 단계 | 1) 작성자가 기여해 새 메모리 게시 2) 다른 로그인 소비자가 recall해 receipt 획득 3) fixture task 성공 후 outcome 제출 4) author dashboard에서 version별 recall/상세/outcome 추이 확인 5) 익명 사용과 작성자 본인 사용도 비교 6) opt-in leaderboard 확인 |
| 합격 기준 | 성공 receipt 1건만 정책 점수를 만들고, self-use·익명 호출·실패·중복 재전송은 점수를 만들지 않는다. 대시보드는 접근·상세 응답·다른 계정 수·outcome을 점수와 분리해 보여주며, 익명 수를 고유 방문자로 표현하지 않는다. 공개 순위에는 opt-in 표시명과 합계만 나온다. reversal은 원장에 별도 기록된다. |
| 관련 ID | API-26~40, SCORE-01~24, DB-07~19, WEB-18~25, SEC-C |
| 자동화 | tests/user-flows/journeys/FLOW-06 |

### FLOW-07 계정 삭제와 기여 귀속 철회

| 항목 | 내용 |
|---|---|
| 역할 | 계정과 기여 이력을 정리하려는 기여자 |
| 목표 | 개인 계정 접근과 기기 연결을 종료하고, 공유 메모리는 남은 독립 근거에 따라 안전하게 유지하거나 재검증한다 |
| 사전조건 | 합성 사용자 A의 활성 웹 세션·MCP 기기, 대기 기여 1건, A만 근거인 게시 메모리 1건, A와 B의 독립 근거가 있는 게시 메모리 1건 |
| 단계 | 1) 설정에서 계정 삭제와 attribution 철회를 요청 2) 확인 절차 완료 3) 기존 웹 세션과 MCP 기기로 protected/write 요청 4) 대기 contribution, author link, 기기 상태 확인 5) Gatekeeper가 두 게시 메모리의 남은 근거를 재평가 6) public recall, leaderboard, audit sink 확인 |
| 합격 기준 | 세션·기기는 해지되고 private dashboard 및 새 write 접근이 차단된다. 대기 payload는 정책에 따라 삭제된다. 공유 메모리는 B의 독립 근거가 유효할 때만 유지되고, A만 근거인 메모리는 재검증 또는 retire 전까지 recall에서 숨긴다. 공개 응답과 로그에서 A를 식별할 수 없으며 필요한 최소 감사 근거만 보존 정책에 따라 남는다. |
| 관련 ID | PRIV-09, WEB-22, LINK-07, API-38, DB-13~14, SEC-G |
| 자동화 | tests/user-flows/journeys/FLOW-07 |

### FLOW-08 내 순위 확인, 프로필 작성, 공개 소개 링크 방문

| 항목 | 내용 |
|---|---|
| 역할 | 점수 기여자, 다른 공개 리더보드 방문자 |
| 목표 | 본인 전체 순위를 확인하고 bio/서비스 링크를 작성한 뒤, 동의한 프로필이 handle을 통해 발견된다 |
| 사전조건 | 합성 계정 A/B/C, 순위가 다른 score ledger, A의 opt-in 상태, HTTPS fixture 사이트 |
| 단계 | 1) A가 dashboard에서 전체 순위와 공개 순위를 구분해 확인 2) handle 충돌과 유효 handle을 시험 3) rich-text editor에서 소개, 서식, 서비스 URL을 입력하고 미리보기/저장 4) 공개 leaderboard에서 A의 handle을 클릭 5) 프로필 페이지에서 bio를 확인하고 서비스 링크 클릭 6) B가 A의 프로필 수정을 시도 7) A가 opt-out한 뒤 목록과 프로필 재조회 |
| 합격 기준 | 전체 순위는 본인에게만 보이고 공개 순위는 opt-in cohort 기준이다. 동점은 같은 순위다. 게시 handle은 `/u/{handle}`로 이동하며 bio와 HTTPS 링크가 보인다. 다른 계정 편집은 거부된다. opt-out 즉시 공개 행/프로필은 404·목록 제외되고 내부 ID/email은 노출되지 않는다. |
| 관련 ID | PROFILE-01~10, ACC-PROFILE-01, SCORE-15~24 |
| 자동화 | tests/user-flows/journeys/FLOW-08 (웹 구현 후) |
