# Notion — Arise live workflow benchmark

최종 갱신: 2026-10-31 · 이슈 #103 / #126 / #128 · **R8: 두 단축 후보 모두 효율 개선 없음; 기본 seed에 반영하지 않음**

## R8 — 짧은 힌트도 실제 개선을 확인한 뒤 반영

첫 후보 M1은 L1을 1,793자에서 943자로 줄이고 L2를 작업자에게 주지 않았다. 두 번째 M2는 일반적인 순서와 요청에 이미 있는 지시를 더 걷어내 조건부 주의사항 699자만 제공했다. 두 후보 모두 전체 작업은 성공했지만 더 느렸고 개별 브라우저 호출도 늘었다.

| 조건 | 전체 작업·독립 확인 | 실행 시간 | 외부 도구 경계 | 개별 브라우저 호출 |
|---|---|---:|---:|---:|
| T — 작업만 | 통과 | 301.862초 | 125 | 124 |
| M1 — 첫 압축 후보 | 통과 | 375.414초 | 112 | 129 |
| M2 — 조건부 주의사항 | 통과 | 406.530초 | 126 | 182 |

M1은 T보다 73.552초(24.37%), M2는 104.668초(34.67%) 느렸다. M1의 외부 경계 감소는 실제 개별 호출 감소가 아니었다. AST 소스 감사에서 세 로그의 직접 호출을 감싼 조건·반복·동적 간접 호출과 스크립트 실행 오류는 0이었다. M2에는 T의 snapshot 9회 대비 42회, read 0회 대비 28회가 기록됐다. 이는 관찰된 추가 확인이며, 그 원인이 힌트 자체라고 분리해 입증한 것은 아니다. M2의 도구 거부 표식은 pending JavaScript dialog·stale snapshot·editable focus 각 1회였다.

M1의 저장된 화면에서는 사이드바가 고정되어 보기 탭이 선택 메뉴로 접히고 보드 일부가 잘려 있었다. 독립 검증자가 사이드바를 닫자 보기 탭과 보드가 드러나는 것을 실제 화면으로 확인했다. M2는 단순 표와 DB 구분, 속성만 요청한 경우의 인라인 편집, 상태 그룹과 값 구분, 열·보기 탭이 잘릴 때만 사이드바 닫기를 안내했으나 더 빨라지지 않았다.

**두 최적화 후보는 기본 seed에 반영하지 않고 원래 Notion 본문으로 복원했다.** 현재·레거시 호스트의 database-entry에서 본문을 요청한 경우에만 작성하도록 하는 수정만 남겼다. 이 수정은 기존 L2가 요청과 무관하게 본문 작성을 요구하던 회귀를 수정 전 실패→수정 후 통과 테스트로 확인한 것이다. 최종 Notion 본문의 속도 개선을 검증했다는 뜻은 아니다. 조건별 힌트 제공과 별도 과제·반복 비교는 #128로 남겼다.

세 조건은 같은 Arise 0.1.60, 실제 실행기 `gpt-6-luna/max`, 새 대화와 900초 한도다. 격리 WAM catalog에서 실제 recall한 L1만 각 M에 제공했고, 고정 버전 L2는 본문 대조용으로 조회했다. 공통 작업·허용 도구·시작 주소와 현재 페이지 wrapper를 대조했고 자동 과거 기억 두 주입이 없었다. 입력창 copyback과 실행기 전체 원문도 일치했다. 시작 주소는 같은 기존 QA 휴지통 페이지였고 복원·수정하지 않았다. 입력 첫 줄에는 이전 R7 표기가 남았지만 실제 생성 제목과 원문에는 고유한 R8 T/M/M2 표식이 있다. 검증기는 이 고유 표식과 전체 원문으로 실행을 구분한다.

세 조건 모두 별도 탭에서 표의 두 항목·날짜·상태, Due / Sort old → new, Board / Group Status, Status is not Complete와 Done 포함 완료 그룹 제외, Alpha만 표시됨과 Private 위치를 독립 확인했다. 각 실행이 만든 DB와 그 하위 두 항목만 Trash 확인창을 거쳐 이동했고, 정확한 QA URL을 새 탭에서 열어 휴지통 배너와 Restore page를 확인했다. 영구 삭제는 하지 않았다. 시험 후 실제 앱의 자동 기억 두 스위치를 원래 true/true로 복원하고 설정 파일에서도 확인했다.

이 실험은 **T→M1→M2의 고정 순서이며 M1 결과를 보고 M2를 수정한 개발 관찰**이다. 같은 로그인·캐시·사이트 UI 상태를 공유하며 별도 미사용 과제나 반복 통계는 없다. 일반화·인과 효과·비용 절감·자동 Arise–WAM 통합을 주장하지 않는다. 준비의 조회·독립 확인·정리는 시간과 호출에서 제외한다. 두 후보의 부정 결과도 보존한다.

새 catalog의 version 번호는 1부터 시작해 기존 endpoint와 opaque version_id가 같을 수 있으므로 실제 L1/L2 본문을 source와 대조했다. 후보와 최종 seed의 격리 catalog를 구분한다. 최종 seed는 별도 catalog에서 현재·레거시 Notion을 포함한 12개 레코드의 실제 L1과 고정 버전 L2가 일치하는지 확인했다. 기존 공유 서버는 변경하지 않았다. 기존 ID의 기본 seed가 자동 갱신되지 않는 문제는 #127로 추적한다.

로컬 증거: `target/e2e-logs/bench/productivity-live126-r8/`의 조건별 결과·입력 검증·독립 확인·정리 JSON, `notion-retrieval.json`, `notion-M2-retrieval.json`, `candidate1-productivity-seed.json`, `candidate2-productivity-seed.json`, `final-seed-source-proof.json`, `source-call-audit.json`, `settings-restored.json`, `result.json`. 계정 원본 화면과 전체 실행 로그는 공개 저장소에 넣지 않는다.

이전 R7 판정: T·M 전체 작업·독립 확인 통과, 당시 실행에서 시간·호출 절감 없음.

## R7 실제 실행

`30bc7b3` 포커스 수정을 포함한 `3fe67c6` / `v0.1.60`을 세 운영체제에 배포하고 Mac GUI·bundled MCP를 함께 교체했다. 계정 프로필은 복사하지 않았다. 두 조건 모두 새 대화, `gpt-6-luna/max`, 900초 한도이며 실제 앱의 자동 기억 두 스위치를 끄고 실행기 user 기록에서 자동 과거 기억이 없음을 확인한다. 입력창 copyback과 실행기 원문 일치도 검사한다(T 1,189자, M 7,453자).

| 조건 | 결과 | 외부 도구 경계 | 실행 시간 | 관찰 오류 |
|---|---|---:|---:|---|
| T | 전체 작업 및 독립 재확인 통과 | 107 | 293.419초 | editable focus 1; renderer/stale 표식 0 |
| M | 전체 작업 및 독립 재확인 통과 | 117 | 327.862초 | editable focus 1; renderer/stale 표식 0 |

두 조건 모두 명시적 memory 호출 0, 탐지한 eval 변경 패턴 0이다. 별도 탭에서 실행이 만든 정확한 QA 페이지 URL을 열어 표의 Alpha QA — In progress — November 2, 2026, Beta QA — Done — November 9, 2026을 확인했다. Sort 메뉴는 Due / Sort old → new였다. Active QA tasks는 Board / Group Status였고 Alpha만 보였다. 필터는 Status is not Complete로 Complete 그룹과 Done 항목이 선택돼 있어 완료 상태를 제외했다. 페이지는 Private 위치였다.

M도 별도 탭에서 두 행의 날짜·상태, Due / Sort old → new, Board / Group Status, Status: Not Complete와 Alpha만 표시됨을 확인했다. M의 완료 보고는 is not Done이라고 표현했지만 실제 UI 요약은 T와 같은 Complete 그룹 제외였다. Done이 그 그룹에 포함돼 요구한 완료 제외 동작을 만족했다.

검증 후 두 실행이 각각 만든 QA 데이터베이스와 그 안의 두 항목만 Move to Trash로 이동했고, 정확한 QA URL 재접속에서 각각 휴지통 배너를 확인했다. 영구 삭제는 하지 않았다. 독립 정리 단계의 페이지 로딩 중 renderer timeout 1회 뒤 재시도는 통과했으며 이 오류는 T 실행의 107회에 포함하지 않는다. 별도 이슈 #27에 기록했다.

완료 시각 이전의 마지막 backend token_count / total_token_usage 보고값은 다음과 같다.

| 조건 | 누적 input | 그중 cached input | output | total |
|---|---:|---:|---:|---:|
| T | 12,807,265 | 12,604,928 | 21,576 | 12,828,841 |
| M | 11,368,159 | 11,196,288 | 23,117 | 11,391,276 |

반복 컨텍스트를 포함한 누적 보고값이다. M의 input은 적었지만 output은 늘었고 캐시 상태도 다르므로 청구 비용이나 인과적인 절감으로 해석하지 않는다.

관찰된 M은 T보다 호출과 시간이 늘었다. 실제 힌트를 불러와 두 조건을 실행했지만 시간·호출 절감은 확인하지 못했다. 조건당 한 번, T→M 고정 순서와 공유 로그인 상태이므로 인과 효과나 일반 성공률을 주장하지 않는다. 자동 Arise–WAM 통합 시험도 아니다.

로컬 증거: `target/e2e-logs/bench/productivity-live103-r7/`의 `notion-{T,M}-result.json`, 조건별 입력 검증, `notion-{T,M}-independent-*`(M 표의 최종 확인은 `table-final`), `notion-{T,M}-cleanup.json`, `notion-retrieval.json`. 원본 계정 화면과 전체 실행 로그는 공개 저장소에 넣지 않는다.

시험 종료 후 실제 XDG 경로와 기본 설정 경로의 자동 기억 두 스위치를 원래 true/true로 복원했고 앱 화면·파일 readback을 확인했다. R7 `settings-restored.json`에 기록했다.

## 실행 조건과 성공 기준

실제 로그인 상태의 Arise 새 대화에서 `gpt-6-luna/max`로 조건당 900초 동안 작업한다. 두 조건 모두 실제 앱 UI에서 자동 personal/WAM 기억을 끄고 실행기 user 입력에 자동 과거 요약이 없는지 확인한다. 명시적 memory 도구도 금지한다. T에는 작업만 제공하고 M에는 실제 로컬 WAM recall 및 version-pinned L2 힌트 세 개를 추가한다. 자동 Arise–WAM 통합 검증은 아니다.

호스트 Unicode 입력 누락이 다른 서비스의 R4에서 확인되어, R7는 네이티브 복사·붙여넣기 후 입력창 copyback과 실행기 user 기록을 원문과 대조한다. 정확한 입력이 확인된 실행만 비교한다.

1. 기존 비공개 워크스페이스 안에 새 QA 페이지와 작업 데이터베이스를 만든다. 기존 페이지는 수정하지 않는다.
2. `Name`, `Status`, `Due` 속성을 준비한다. Alpha QA는 In progress·2026-11-02, Beta QA는 Done·2026-11-09로 입력한다.
3. All QA tasks 표 보기에 두 항목을 마감일 오름차순으로 표시한다.
4. Active QA tasks 보드 보기를 상태별로 묶고 Done 제외 필터로 Alpha QA만 표시한다.
5. 새로고침 후 항목·속성·보기·필터·정렬을 다시 확인한다. 독립 검증자가 재확인하고 새 QA 페이지만 정리한다. 공유나 멤버 초대는 하지 않는다.

## 메모리 조회

이번 실행의 실제 WAM API에서 다음 세 항목과 각각의 고정 버전 L2를 조회했다.

- `web:example-notion-task-database`
- `web:example-notion-database-entry`
- `web:example-notion-database-views`

현재 앱 호스트 `app.notion.com`과 레거시 `www.notion.so`를 구분한다. 조회 성공은 데이터베이스 편집 성공의 증거가 아니다. 조건당 한 번·T→M 고정 순서·공유 로그인 상태의 파일럿이므로 인과적인 메모리 효과나 일반 성공률을 주장하지 않는다.

## 이전 기록과 증거

R3(0.1.57)에서는 로그인 화면까지만 확인해 편집 성능 측정에서 제외했다. 이후 기존 로그인 세션을 확인했으며, R7에서 실제 데이터베이스 시험을 완료했다. 이전 로그인 질문의 시간 초과를 로그인 완료로 해석하지 않았다.

이전 준비 조회 증거: `target/e2e-logs/bench/productivity-live103-r6/notion-retrieval.json`. 이후 전체 작업은 R7에서 확인했고, 최신 후보 비교와 독립 확인·정리는 위 R8 기록을 따른다. 계정·워크스페이스 원본 화면과 전체 에이전트 로그는 공개 저장소에 포함하지 않는다.
