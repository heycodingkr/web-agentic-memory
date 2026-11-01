# 기능 명세 (Feature Spec)

상태: 기능별 contract와 test ID가 정리되어 있다. 명세 작성이나 working-tree 구현은 외부 서비스/production 검증 완료를 뜻하지 않는다.

기능 명세는 "무엇을 만들고, 어떤 상태가 되면 끝인가"의 단일 출처다. 설계([`../design/`](../design/architecture.md))는 "어떻게 만드는가", 테스트([`../test/`](../test/README.md))는 "어떻게 검증하는가"를 다룬다.

Chrome과 Arise를 연결하는 사전 결정, 데이터 경계, HOST acceptance는 [`../design/browser-host-integration.md`](../design/browser-host-integration.md)에 기록한다. 세부 요구사항과 테스트 연결은 아래 MCP·설치·recall 명세를 따른다.

## 작업 순서

```
기능 명세        docs/spec/
  → 테스트 스펙   docs/test/functional-tests.md, security-scenarios.md  (케이스 ID, 합격 기준)
  → 스모크 테스트  docs/test/host-smoke.md   →  tests/smoke/
  → 유저 플로우    docs/test/user-flows.md   →  tests/user-flows/
```

[`../test/README.md`](../test/README.md) 원칙 1에 따라 테스트 ID는 구현보다 먼저 합의한다. 명세가 바뀌면 이 문서를 먼저 고치고, 그다음 테스트 문서, 그다음 코드 순서로 따라간다.

## 출발점

현재 구현의 canonical source는 루트 Cargo workspace와 `web/`이며 이 디렉터리의 기능 계약을 따른다. [`reference/.../web-agentic-memory.md`](../../reference/agent-browser/docs/specs/web-agentic-memory.md)는 서버 카탈로그·동의에 관한 과거 참조 사본이며 현재 제품의 구현 상태표가 아니다. 기존 `/web-memory/v1/*` 클라이언트와의 호환 요구(`API-11`)는 `api.md`에 포함한다.

## 규약 (안)

- 영역 이름은 테스트 ID 접두사와 같다 (`CORE`, `GK`, `DEDUP`, `API`, `LINK`, `MCP`, `SCORE`, `PROFILE`, `WEB`, `DB`, `PRIV`, `MODEL`, `MEM`, `RCL`, `QUE`, `CUR`, `RL`, `USAGE`, `CACHE`, `PERF`). 영역 하나가 파일 하나다.
- 요구사항 ID는 `F-<영역>-<번호>` (예: `F-LINK-03`). 테스트 ID와 번호를 맞추지 않는다. 요구사항 하나에 테스트 여러 개가 걸릴 수 있다.
- 요구사항마다 "관련 테스트"에 `functional-tests.md`, `service-abuse-performance.md`, `host-smoke.md`, `security-scenarios.md`, `user-flows.md`의 ID를 적는다. 테스트가 없는 요구사항은 미완성이다.
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
| [`provider-outage.md`](provider-outage.md) | 모델 provider 일시 장애 대기: 분류, 시도 없는 대기, 한도·순환, 운영 (`OUTAGE`) | 명세(구현 전, #158) |
| [`curator.md`](curator.md) | Gatekeeper Agent의 지침과 도구 (`CUR`) | 1차 상세 초안 |
| [`model.md`](model.md) | Meta 모델 기반 Gatekeeper 결정 (`MODEL`) | 1차 상세 초안 |
| [`install.md`](install.md) | Claude Code·Codex 설치와 호스트 등록, 복사 계약 (`INST`, 시나리오 `INST` `MCPF` `SNIP`) | 1차 상세 초안 |
| [`release.md`](release.md) | candidate·증거 판정·플랫폼·출시 게이트 (`REL`) | 첫 계약·로컬 판정기 구현 |
| [`cli.md`](cli.md) | `wam` 명령 표면, 종료 코드, 출력 계약 (`CLI`) | 1차 상세 초안 |
| [`rate-limits.md`](rate-limits.md) | IP·계정별 요청 제한, 429, 어뷰징 제어 (`RL`) | 서버·웹 구현, 외부 Redis/ingress 검증 대기 |
| [`usage-analytics.md`](usage-analytics.md) | 계정 사용량, 메모리 영향, 차트 계약 (`USAGE`) | API·대시보드 구현, live DB/Redis 검증 대기 |
| [`client-cache.md`](client-cache.md) | CLI/MCP 공용 로컬 캐시와 서버 공개 캐시 (`CACHE`) | 익명 recall 로컬 cache 구현; vocab/detail/shared HTTP cache는 미구현 |
| [`search-performance.md`](search-performance.md) | 검색·DB·직렬화·복원력 측정 계약 (`PERF`) | 인덱스 경로 구현, query-plan/scale benchmark 대기 |

사용자 작업의 성능 이득(`PERF-01~12`)은 [`../test/benchmarks.md`](../test/benchmarks.md)에서, 검색 pipeline과 DB/API 성능(`PERF-13~20`)은 [`../test/retrieval-benchmark.md`](../test/retrieval-benchmark.md)에서, rate limit·usage·cache·serializer·fault tests는 [`../test/service-abuse-performance.md`](../test/service-abuse-performance.md)에서 다룬다. 공통 수치 기준은 [`../test/metrics.md`](../test/metrics.md)를 따른다.

## 기존 문서 반영 여부 점검

아래는 작업 전 문서 상태를 기준으로 한 점검이다. 현재 독립 실행형 로컬 제품 코드가 있다. 아래 표는 작업 전 문서 공백을 설명하며 “명세 있음”은 “기능 구현 완료”를 뜻하지 않는다. 현재 구현·실행 결과와 남은 출시 요건은 [`implementation-priorities.md`](../plan/implementation-priorities.md)를 따른다.

| 요구사항 | 작업 전 문서 상태 | 보강 후 기준 |
|---|---|---|
| 웹에서 Claude/Codex용 MCP 설정 문자열 복사 | 기존에는 일반 스니펫 복사만 정의. 설치 확인과 브라우저 도구 공존 테스트가 빠짐 | mcp.md, MCP-16~24, HOST-01~12 |
| 웹 로그인과 본인 기여 분석 대시보드 | 기존에는 로그인과 화면만 정의. 기여 소유권·세부 상태·익명 점수 제외·기여 version별 사용 차트가 약함 | web.md, api.md, link.md, usage-analytics.md, WEB-16~29, API-30~46 |
| 기존 URL 메모리 중복·최신성 판단 | 기존 설계는 hostname + 작업 category 중심. 경로별 URL 메모리 식별이 없음 | core.md, dedup.md, gatekeeper.md, CORE-22~26, DEDUP-13~18 |
| gatekeeper 에이전트가 업데이트 여부 결정 | 기존 설계는 일상 게시에도 관리자 승인을 요구해 요청과 충돌 | gatekeeper.md와 설계 변경: 에이전트가 정상 케이스 최종 판정, 예외만 사람 검토 |
| 로그인 소비자의 성공 사용으로 기여자 점수 지급 | 기존 점수 초안은 있으나 익명 호출의 무점수 처리와 접근 분석 통계의 점수 분리가 명시되지 않음 | score.md, privacy.md, database.md, SCORE-15~24, DB-07~19 |
| Claude Code·Codex CLI에서 MCP 또는 CLI 형태로 쉽게 설치 | 기존에는 웹 복사 문자열과 `wam link` 언급뿐이었다. `wam` CLI 명세, 설치 채널, 호스트 한도 확인, 형태별 시나리오가 없음 | install.md, cli.md, INST-01~48, MCPF-01~27, CLI-01~33, SNIP-01~10 |
| 비회원·회원과 실제 host 연결 상태 구분 | 익명 read, 웹 로그인, device link 테스트가 각각 있었으나 웹 계정만 있는 경우와 연결 전/후 Chrome·CLI·Arise 전체 경로 교차 검증이 없음 | mcp.md, link.md, recall.md, `../test/customer-paths.md` CUST-01~06, `../test/feature-acceptance.md` ACC-MCP-05 |

## 문서 범위와 검증 상태

`docs/test/functional-tests.md`는 단위·통합 계약을, `docs/test/feature-acceptance.md`는 위 요구사항을 처음부터 끝까지 검증하는 수용 기준을, `docs/test/database.md`는 저장 제약과 동시성 검사를 정의한다. 기준 정의와 실행 결과를 구분한다. 구현·로컬 검증이 있는 항목과 외부 수용 검증이 남은 항목은 각 명세의 상태와 [출시 수용 기준](../test/release-readiness.md)에 적는다. 운영 설정·프록시·worker·배포 후 확인은 [운영 배포 절차](../operations/deployment.md)를 따른다.
