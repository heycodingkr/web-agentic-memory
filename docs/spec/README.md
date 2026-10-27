# 기능 명세 (Feature Spec)

상태: CORE/DEDUP/API/LINK/MCP/WEB/DB/GK/SCORE/PROFILE/PRIV/MODEL 1차 상세 초안. 명세 작성은 코드 동작을 검증했다는 뜻이 아니다.

기능 명세는 "무엇을 만들고, 어떤 상태가 되면 끝인가"의 단일 출처다. 설계([`../design/`](../design/architecture.md))는 "어떻게 만드는가", 테스트([`../test/`](../test/README.md))는 "어떻게 검증하는가"를 다룬다.

## 작업 순서

```
기능 명세        docs/spec/
  → 테스트 스펙   docs/test/functional-tests.md, security-scenarios.md  (케이스 ID, 합격 기준)
  → 스모크 테스트  docs/test/host-smoke.md   →  tests/smoke/
  → 유저 플로우    docs/test/user-flows.md   →  tests/user-flows/
```

[`../test/README.md`](../test/README.md) 원칙 1에 따라 테스트 ID는 구현보다 먼저 합의한다. 명세가 바뀌면 이 문서를 먼저 고치고, 그다음 테스트 문서, 그다음 코드 순서로 따라간다.

## 출발점

이미 구현된 것의 명세는 [`reference/.../web-agentic-memory.md`](../../reference/agent-browser/docs/specs/web-agentic-memory.md)(서버 카탈로그, 기여 동의, 구현 증거)에 있다. 새 명세는 이를 출발점으로 삼는다. 기존 `/web-memory/v1/*` 클라이언트와의 호환 요구(`API-11`)는 `api.md`에 포함한다.

## 규약 (안)

- 영역 이름은 테스트 ID 접두사와 같다 (`CORE`, `GK`, `DEDUP`, `API`, `LINK`, `MCP`, `SCORE`, `PROFILE`, `WEB`, `DB`, `PRIV`, `MODEL`, `MEM`, `RCL`, `QUE`, `CUR`). 영역 하나가 파일 하나다.
- 요구사항 ID는 `F-<영역>-<번호>` (예: `F-LINK-03`). 테스트 ID와 번호를 맞추지 않는다. 요구사항 하나에 테스트 여러 개가 걸릴 수 있다.
- 요구사항마다 "관련 테스트"에 `functional-tests.md`, `host-smoke.md`, `security-scenarios.md`, `user-flows.md`의 ID를 적는다. 테스트가 없는 요구사항은 미완성이다.
- 기능 → 테스트 추적표의 원본은 [`../test/README.md`](../test/README.md) 6절이다. 같은 표를 여기에 다시 쓰지 않는다.
- 작성 템플릿: [`_template.md`](_template.md)

## 명세 현황

| 파일 | 영역 | 상태 |
|---|---|---|
| [`core.md`](core.md) | URL, 정규화, trace 계약 (`CORE`) | 1차 상세 초안 |
| [`dedup.md`](dedup.md) | URL/task 중복·최신성·갱신 (`DEDUP`) | 1차 상세 초안 |
| [`gatekeeper.md`](gatekeeper.md) | 자동 판정, 예외 검토 (`GK`) | 1차 상세 초안 |
| [`api.md`](api.md) | catalog, 기여, 영수증, 인증 (`API`) | 1차 상세 초안 |
| [`link.md`](link.md) | OAuth 로그인·MCP 디바이스 연결 (`LINK`) | 1차 상세 초안 |
| [`mcp.md`](mcp.md) | MCP 설정, 도구 계약, 호스트 연결 (`MCP`) | 1차 상세 초안 |
| [`score.md`](score.md) | 로그인 점수, 사용 영수증 (`SCORE`) | 1차 상세 초안 |
| [`profile.md`](profile.md) | creator handle, 공개 bio/서비스 링크, 개인·공개 순위 (`PROFILE`) | 기능 상세 명세 |
| [`web.md`](web.md) | 로그인, 기여 큐, 공개 화면 (`WEB`) | 1차 상세 초안 |
| [`database.md`](database.md) | 스키마·제약·트랜잭션 (`DB`) | 1차 상세 초안 |
| [`privacy.md`](privacy.md) | 동의·최소 수집·철회·보관 (`PRIV`) | 1차 상세 초안 |
| [`memory.md`](memory.md) | 구조화 메모리 레이어 L0/L1/L2, kind, 색인 (`MEM`) | 1차 상세 초안 |
| [`recall.md`](recall.md) | URL + 키워드 recall, 순위, 호출 시점 (`RCL`) | 1차 상세 초안 |
| [`queue.md`](queue.md) | 기여·큐레이션 큐, 백프레셔, 모델 예산 (`QUE`) | 1차 상세 초안 |
| [`curator.md`](curator.md) | Gatekeeper Agent의 지침과 도구 (`CUR`) | 1차 상세 초안 |
| [`model.md`](model.md) | Meta 모델 기반 Gatekeeper 결정 (`MODEL`) | 1차 상세 초안 |
| [`install.md`](install.md) | Claude Code·Codex 설치와 호스트 등록, 복사 계약 (`INST`, 시나리오 `INST` `MCPF` `SNIP`) | 1차 상세 초안 |
| [`cli.md`](cli.md) | `wam` 명령 표면, 종료 코드, 출력 계약 (`CLI`) | 1차 상세 초안 |

성능 이득(`PERF`)은 [`../test/benchmarks.md`](../test/benchmarks.md)와 [`../test/metrics.md`](../test/metrics.md)가 이미 명세 역할을 한다. 별도 파일을 만들지 않는다.

## 기존 문서 반영 여부 점검

아래는 작업 전 문서 상태를 기준으로 한 점검이다. 이 저장소에는 아직 독립 실행형 제품 코드가 없으므로, “명세 있음”은 “기능 구현 완료”를 뜻하지 않는다.

| 요구사항 | 작업 전 문서 상태 | 보강 후 기준 |
|---|---|---|
| 웹에서 Claude/Codex용 MCP 설정 문자열 복사 | 기존에는 일반 스니펫 복사만 정의. 설치 확인과 브라우저 도구 공존 테스트가 빠짐 | mcp.md, MCP-16~24, HOST-01~06 |
| 웹 로그인과 본인 기여 분석 대시보드 | 기존에는 로그인과 화면만 정의. 기여 소유권·세부 상태·익명 점수 제외·기여 version별 사용 차트가 약함 | web.md, api.md, link.md, WEB-16~25, API-30~40 |
| 기존 URL 메모리 중복·최신성 판단 | 기존 설계는 hostname + 작업 category 중심. 경로별 URL 메모리 식별이 없음 | core.md, dedup.md, gatekeeper.md, CORE-22~26, DEDUP-13~18 |
| gatekeeper 에이전트가 업데이트 여부 결정 | 기존 설계는 일상 게시에도 관리자 승인을 요구해 요청과 충돌 | gatekeeper.md와 설계 변경: 에이전트가 정상 케이스 최종 판정, 예외만 사람 검토 |
| 로그인 소비자의 성공 사용으로 기여자 점수 지급 | 기존 점수 초안은 있으나 익명 호출의 무점수 처리와 접근 분석 통계의 점수 분리가 명시되지 않음 | score.md, privacy.md, database.md, SCORE-15~24, DB-07~19 |
| Claude Code·Codex CLI에서 MCP 또는 CLI 형태로 쉽게 설치 | 기존에는 웹 복사 문자열과 `wam link` 언급뿐이었다. `wam` CLI 명세, 설치 채널, 호스트 한도 확인, 형태별 시나리오가 없음 | install.md, cli.md, INST-01~48, MCPF-01~24, CLI-01~30, SNIP-01~10 |
| 비회원·회원과 실제 host 연결 상태 구분 | 익명 read, 웹 로그인, device link 테스트가 각각 있었으나 웹 계정만 있는 경우와 연결 전/후 Chrome·CLI·Arise 전체 경로 교차 검증이 없음 | mcp.md, link.md, recall.md, `../test/customer-paths.md` CUST-01~06, `../test/feature-acceptance.md` ACC-MCP-05 |

## 문서 범위와 검증 상태

`docs/test/functional-tests.md`는 단위·통합 계약을, `docs/test/feature-acceptance.md`는 위 요구사항을 처음부터 끝까지 검증하는 수용 기준을, `docs/test/database.md`는 저장 제약과 동시성 검사를 정의한다. 이 기준들은 아직 구현 테스트 결과가 아니라 개발 전 합의할 명세다.
