# 테스트 전략 (Doc-first)

상태: core/Gatekeeper/server/client/MCP/CLI 자동 테스트와 web 검사·테스트·빌드가 있다. 실제 disposable PostgreSQL에서 application/진단 로그 저장·복구를 검증했고, 격리된 Chrome 스모크가 실제 웹·CLI·stdio MCP와 SQLite 로그를 함께 확인한다. 스모크의 OAuth/model/replay는 합성 fixture다. 실제 외부 provider/OAuth/호스트 수용 시험과 대규모 부하 합격은 별도이며, 기본 테스트에서 외부 서비스용 ignored 항목은 명시적으로 실행해야 한다.

| 문서 | 내용 |
|---|---|
| 이 문서 | 원칙, 계층, 환경, 증거 규약, 기능-테스트 추적표, 마일스톤 게이트 |
| [`environment.md`](environment.md) | 현재 checkout의 실행 가능 범위, 실행 결과, 미확인 환경 |
| [`harness-verification.md`](harness-verification.md) | 테스트 하네스 자체 검증 (HARN-01~12): oracle 정확도, 정리, 통계·스케줄러, 도구 정의 |
| [`functional-tests.md`](functional-tests.md) | 주요 기능별 테스트 케이스 (코어, 게이트키퍼, API, MCP, 점수, 웹) |
| [`profile.md`](profile.md) | handle, rich-text bio, 외부 링크, 개인·공개 순위 테스트 명세 |
| [`failure-matrix.md`](failure-matrix.md) | 인증·네트워크·DB·모델·큐·브라우저·cache·limiter 장애와 복구 계약 (FAIL-01~38) |
| [`local-smoke.md`](local-smoke.md) | 실제 Chrome + 로컬 fixture + CLI/stdio MCP; 메모리 생성/업데이트/삭제 및 DB trace 매칭 |
| [`../spec/diagnostics.md`](../spec/diagnostics.md) | 안전한 진단 스키마, DB 로그, offline spool과 오류·복구 검증 |
| [`cli-link.md`](cli-link.md) | CLI device flow·OS secure storage·stdin·지침 마커 갱신·격리된 Codex/Claude 등록 검증과 남은 범위 |
| [`release-readiness.md`](release-readiness.md) | 공개 client/service 출시 게이트, candidate 증거 계약, 현재 공백과 판정기 (REL-01~14) |
| [`remote-oauth.md`](remote-oauth.md) | MCP discovery/DCR/PKCE·refresh/revoke, Chrome 동의/CORS와 격리 native Codex OAuth 검증 |
| [`host-smoke.md`](host-smoke.md) | Claude Code, Codex CLI, Meta Muse CLI, Arise, Chrome+MCP 스모크 |
| [`../design/browser-host-integration.md`](../design/browser-host-integration.md) | Chrome CDP 경계, Arise MCP Hub 경로, 구현 전 결정 및 HOST-07~12 acceptance |
| [`hosts/`](hosts/README.md) | 호스트별 실행 문서 (확인된 설치/설정 사실만 기록) |
| [`tool-forms/`](tool-forms/README.md) | 도구 형태(원격·로컬 MCP, `wam` CLI, instruction 스니펫)별 설치·사용 시나리오와 형태·호스트·단계 분류 (INST, MCPF, CLI, SNIP) |
| [`user-flows.md`](user-flows.md) | 유저 플로우: 사용자 한 명의 목표 달성 여정 (웹, 서버, MCP 횡단) |
| [`feature-acceptance.md`](feature-acceptance.md) | 제품 여정별 수용 기준과 독립 증거 |
| [`customer-paths.md`](customer-paths.md) | 비회원·회원 × Chrome/CLI/Arise별 설치·URL+키워드 recall·receipt 여정 (CUST-01~06) |
| [`database.md`](database.md) | PostgreSQL 제약·소유권·점수 원장 테스트 |
| [`benchmarks.md`](benchmarks.md) | 실사이트 벤치: Medium/Naver, Google Slides 이미지 생성, Google Flow 영상 제작 (주입 전/후) |
| [`retrieval-benchmark.md`](retrieval-benchmark.md) | URL+키워드 recall 모듈 경계, 구조화 fixture, 규모별 DB/API 성능, 민감값 sink 검증 |
| [`service-abuse-performance.md`](service-abuse-performance.md) | IP/account quota, usage API/UI, CLI/MCP cache, serializer·DB performance, fault injection (`RL`, `USAGE`, `CACHE`, `PERF-21~25`) |
| [`metrics.md`](metrics.md) | 측정 기준 수치화: 게이트/목표값, 통계 방법, 주장 등급 (A/B/C) |
| [`../plan/benchmark-readiness.md`](../plan/benchmark-readiness.md) | 벤치 준비 작업 계획, 의존성, Meta 에이전트 호환 작업, 최소 데모 범위 |
| [`security-scenarios.md`](security-scenarios.md) | 비밀값 유출, 인젝션, 포이즈닝, 인증 우회, 남용 시나리오 |
| [`../spec/README.md`](../spec/README.md) | 기능 명세: 테스트 ID가 추적하는 요구사항의 출처, 작성 순서 |
| [`../../tests/README.md`](../../tests/README.md) | 테스트 코드와 환경의 디렉터리 구조, 문서와 코드의 대응 |

## 1. 원칙

1. **합의된 ID 먼저**: 기능 구현은 해당 테스트 ID가 문서에 있어야 시작한다. 구현 중 케이스가 늘면 문서를 먼저 고친다.
2. **합성 비밀값만**: 테스트에 쓰는 키/개인정보는 전부 가짜(센티널)다. 실제 키, 실제 계정, 실제 개인정보를 테스트 데이터로 쓰지 않는다.
3. **격리**: 모든 테스트는 로컬 loopback 서버와 일회용 DB, 일회용 설정 디렉터리에서 돈다. 운영 서버와 사용자의 실제 프로필을 건드리지 않는다.
4. **증거 기반 합격**: "에이전트가 성공했다고 말함"은 증거가 아니다. 서버 상태, DB, 로그, 트레이스, 독립 oracle로 판정한다.
5. **보안은 0 허용**: 센티널 유출은 한 건이라도 실패다. 확률적 허용치를 두지 않는다.
6. **성능 주장은 반복 측정**: 단발 파일럿으로 일반화하지 않는다. 조건당 3~5회 이상, 평균과 편차를 보고한다.
7. **정리**: 테스트가 만든 프로세스, 탭, 파일, DB만 정리한다. 남의 것은 건드리지 않는다.

## 2. 테스트 계층

| 계층 | 범위 | 도구(안) | 실행 시점 |
|---|---|---|---|
| L0 단위 | `wam-core` 규칙, API client, CLI parser, MCP protocol schema | `cargo test --workspace`; property/fuzz는 후속 추가 | 매 커밋 |
| L1 통합 | Axum router + injected in-memory repository, deterministic Gatekeeper tool/curator suites, optional disposable-PostgreSQL/Redis adapter tests | `wam-server/tests/api.rs`, `wam-server/tests/postgres_application.rs`, `wam-gatekeeper/tests/`, `WAM_TEST_DATABASE_URL`, `WAM_TEST_REDIS_URL` runner | in-memory suite runs locally; external adapters require disposable services |
| L2 계약 | OpenAPI 스키마, MCP 프로토콜 준수 | OpenAPI 검증, MCP Inspector/스크립트 클라이언트 | 매 PR |
| L3 웹 E2E | 웹 UI 흐름 (로그인, 연결 승인, 리더보드, 가이드) | Playwright | 매 PR |
| L4 호스트 스모크 | 실제 CLI/브라우저 호스트에서 설치, 호출, 정리 | 스크립트 하네스 (`host-smoke.md`) | 릴리스 전, 야간 |
| L5 보안 시나리오 | 공격 시나리오 재현 (`security-scenarios.md`) | 전용 하네스, 퍼저 | 매 PR(핵심), 릴리스 전(전체) |
| L6 성능 벤치 | tools-only vs 메모리 주입 A/B | 기존 `probe_agent_memory_live.py` 계열 | 주간, 릴리스 전 |

## 3. 환경과 픽스처

| 이름 | 설명 |
|---|---|
| `wam-test-server` | loopback에서 뜨는 서버 + 일회용 Postgres. 시드 카탈로그 주입 가능 |
| `wam-test-limiter` | `WAM_TEST_REDIS_URL`의 일회용 Redis/Valkey namespace, test-only credentials, fake clock/policy로 quota atomicity와 outage 재현. 운영 endpoint 금지 |
| `fixture-sites` | 로컬 정적/동적 테스트 사이트: 리치 에디터, iframe 에디터, 이어쓰기 팝업 사이트, 화면 밖 Save 사이트. 독립 saved-oracle(HTTP) 포함 |
| `sentinels` | 가짜 비밀값 말뭉치 (아래 4절) |
| `inj-corpus` | 인젝션/악성 힌트 말뭉치 (`security-scenarios.md`) |
| `host-profiles` | 호스트별 일회용 설정 디렉터리 (HOME, XDG_CONFIG_HOME, TMPDIR 분리) |
| `fake-llm` | G2 LLM sanitizer 대역 (결정적 응답, 인젝션 시도 포함) |
| `fake-oauth` | GitHub OAuth 대역 (`WEB-02`, `SEC-D09`) |

구현 위치는 [`tests/env/`](../../tests/env/README.md)다. `sentinels`는 `tests/sentinels.json`, `inj-corpus`는 `tests/security/inj-corpus/`에 둔다.

## 4. 센티널 말뭉치 규약

센티널은 **실제 서비스에서 유효하지 않은** 가짜 값이며 실제 포맷과 형태만 같다. 한 파일(`tests/sentinels.json`)에서만 정의하고, 테스트는 이를 import한다.

| 보안 테스트 대상 유형 | 예 (형태) |
|---|---|
| 클라우드/LLM API 키 형태 | 접두사 + 고엔트로피 문자열 (가짜) |
| JWT 형태 | 헤더.페이로드.서명 (가짜 서명) |
| Bearer/Authorization 헤더 | `Bearer <가짜>` |
| 비공개 키 블록 | `-----BEGIN ... PRIVATE KEY-----` 가짜 본문 |
| 이메일, 전화번호, 주민/신분증 형식, 카드번호(Luhn 유효 가짜) | 한국/해외 형식 모두 |
| IP, 내부 호스트명, 로컬 경로 | `/Users/<가짜>/...`, `C:\Users\...` |
| 쿠키/세션 ID, 쿼리스트링 토큰 | 가짜 |
| 검색 변형 | literal, URL percent encoding, Base64/Base64URL, 16진수, 동일 artifact 안에 나뉜 조각 |

현재 생성된 500개 corpus는 API-key 형태, JWT, Bearer, private-key test block, 예약 이메일 도메인, 무효 전화번호, 합성 로컬 경로, session cookie, URL query, opaque token의 10 family다. 주민/신분증, 결제 카드, IP, 내부 호스트명은 이 corpus에 들어 있지 않다. 이 형식들을 테스트할 때는 전용 합성 fixture를 추가해야 한다.

**판정 방법**: `tests/security/scan_sentinels.py`가 명시한 로컬 파일/디렉터리를 재귀 검색하고 값 자체는 출력하지 않는다. JSON/nested-JSON escape, NFKD·invisible 문자 제거·명시한 common-confusable map도 검사한다. 모든 Unicode 및 대소문자 변형 전체를 포괄하지는 않는다. `pnpm --dir web run test:security`는 실제 loopback web/CLI/MCP/API와 8종 로컬 sink를 수집하고 `check_sink_inventory.py`로 누락·무결성·누출·500개 양성 대조군을 검사한다. 실제 PostgreSQL/Redis/WAL·host/provider/staging sink 수집은 별도다. [범위와 실행법](../../tests/security/README.md)을 따른다. (4.1 참조)

### 4.1 유출 검색 대상 (sink 목록)

- 서버 DB 전 테이블 덤프, 작업 큐, 감사 로그
- 카탈로그 API 응답, `/memories`, `/expand`, 리더보드, `/me`
- MCP 응답(도구 결과, 에러 메시지, 서버 instructions)
- 서버/워커/클라이언트 로그, `tracing` 출력, 패닉 메시지
- 클라이언트 로컬 캐시, 큐 파일, 설정 파일
- 프로세스 인자와 환경 변수 (`ps` 출력)
- HTTP 에러 바디와 응답 헤더
- 웹 HTML 소스, 클라이언트 번들, 소스맵

현재 스캐너는 위 sink를 자동 수집하지 않는다. 각 sink를 로컬 evidence 파일로 내보낸 후 경로를 전달한다. 예: `python3 -B tests/security/scan_sentinels.py target/e2e-logs/<suite>/<run-id>/sinks --json-out target/e2e-logs/<suite>/<run-id>/sentinel-scan.json`. 결과는 clean일 때 0, leak 발견 시 1, 읽기 실패/파일 크기 초과 시 2를 반환한다. 상세 범위와 제한은 [`tests/security/README.md`](../../tests/security/README.md)에 있다.

## 5. 증거 규약

각 실행은 `target/e2e-logs/<suite>/<run-id>/`에 남긴다. (git 무시)

| 파일 | 내용 |
|---|---|
| `meta.json` | 커밋, 호스트 종류/버전, OS, 모델/effort(해당 시), 시작/종료 시각 |
| `cases.jsonl` | 케이스 ID, 입력 요약, 기대, 실제, 판정, 근거 경로 |
| `trace.jsonl` | 구조화 이벤트 (요청, 도구 호출, 상태 전이). 완료는 명시적 Done/Failed 이벤트로 판정 (로그 침묵은 완료 증거 아님) |
| `sinks/` | 유출 검색에 쓴 덤프 스냅샷 |
| `shots/` | 웹/브라우저 스크린샷 (필요 시) |
| `summary.md` | 사람이 읽는 요약: 통과/실패/건너뜀, 실패 원인 |

규칙: 통과한 대표 이미지는 사람이 열어서 확인한다. 건너뛴 케이스는 이유를 기록한다 (조용히 건너뛰기 금지).

## 6. 기능 → 테스트 추적표

| 기능 | 기능 테스트 | 호스트 스모크 | 보안 시나리오 |
|---|---|---|---|
| 스키마 검증, URL 정규화·경로 범위 | CORE-01~08, CORE-20~26 | | SEC-A, SEC-E |
| Redaction (G1) | CORE-10~19, GK-01~08 | SMK-07 | SEC-A 전체 |
| URL/task별 신규성·중복·최신성 (G3) | DEDUP-01~18 | | SEC-C |
| 재현 검증 (G4) | GK-20~26 | | SEC-C |
| Agent 판정, 게시/롤백 (G5/G6) | GK-30~48 | | SEC-B, SEC-C, SEC-F |
| Meta 모델 provider와 Gatekeeper AI 판정 | MODEL-01~14 (`../spec/model.md`) | HOST-01~04 | SEC-A, SEC-B, SEC-E |
| 구조화 메모리 레이어 (L0/L1/L2) | MEM-01~15 (`../spec/memory.md`) | | SEC-A, SEC-B |
| URL + 키워드 recall | RCL-01~22 (`../spec/recall.md`, `retrieval-benchmark.md`) | SMK-03~06, BEH-01, BEH-08~09 | SEC-A20~22, SEC-D |
| 검색 데이터·DB/API 성능과 재현성 | PERF-13~20, DB-20~21 (`retrieval-benchmark.md`) | retrieval smoke/load | SEC-A20~22 |
| rate limit·IP/account abuse controls | RL-01~12, API-44, SEC-J01~06, FAIL-33~36 | quota/429 flow | SEC-D, SEC-E, SEC-J |
| 개인 API usage와 dashboard | USAGE-01~06, API-40, API-45~46, WEB-25~29, FLOW-09 | | SEC-D, SEC-G, SEC-J |
| CLI/MCP local cache | CACHE-01~11, FLOW-10, FAIL-34, PERF-24 | host cache smoke | SEC-A22, SEC-F, SEC-H, SEC-J |
| search scale, serializer, fault tolerance | DB-21/27, PERF-21~25, FAIL-33~38 | retrieval/load | SEC-A, SEC-E, SEC-J |
| 기여·큐레이션 큐 | QUE-01~24 (`../spec/queue.md`) | | SEC-C, SEC-E, SEC-I |
| 큐레이터 지침과 도구 | CUR-01~21 (`../spec/curator.md`) | HOST-01~04 | SEC-B, SEC-I |
| 카탈로그/조회 API | API-01~12, API-30~31, API-37, API-39 | SMK-03~05 | SEC-D, SEC-E |
| 로그인 기여·receipt·author analytics API | API-20~29, API-32~40 | SMK-07, SMK-09 | SEC-A, SEC-C, SEC-E |
| 디바이스 연결 | LINK-01~11 | SMK-08 | SEC-D |
| MCP 웹 설치·도구·인증 | MCP-01~24, MCPF-25~27 | SMK-01~14 | SEC-B, SEC-F, SEC-H |
| 고객 설치·인증·recall 여정 | CUST-01~06 (`customer-paths.md`) | HOST-01~12, SMK-01~14 | SEC-A, SEC-C, SEC-D, SEC-H |
| Claude Code·Codex 설치, `wam` CLI, 스니펫 | INST-01~48, MCPF-01~27, CLI-01~33, SNIP-01~10 (`tool-forms/`) | HOST-01~12, SMK-01~14 | SEC-A, SEC-H |
| 영수증/점수/리더보드 | SCORE-01~24, PROFILE-05~08 (`profile.md`) | SMK-09 | SEC-C |
| 웹 UI와 contribution analytics dashboard | WEB-01~29, PROFILE-06~10 (`profile.md`) | | SEC-D, SEC-A |
| 프로필, rich-text bio, handle ranking | PROFILE-01~10 (`profile.md`, `../spec/profile.md`) | FLOW-08 | SEC-A, SEC-C, SEC-D |
| DB 무결성 | DB-01~27 | | SEC-A, SEC-C, SEC-D, SEC-J |
| 교차 계층 실패·복구 | FAIL-01~38 (`failure-matrix.md`) | HOST/SMK fail states | SEC-A, SEC-D, SEC-E, SEC-F, SEC-I, SEC-J |
| 동의/철회/보관/계정 삭제 | PRIV-01~09 | SMK-07 | SEC-G |
| 성능 이득 | PERF-01~12 (`benchmarks.md`) | PERF 스모크 | SEC-A19, SEC-H09 |

## 7. 마일스톤 게이트

| 마일스톤 | 통과해야 하는 것 |
|---|---|
| M1 (`wam-core`) | CORE 전부, GK-01~08, SEC-A 전부 (센티널 0 유출), 퍼저 30분 크래시 0 |
| M2 (중복 판정) | DEDUP 전부, 합성 라벨 정확도 기준 (아래), SEC-C 중 중복 스팸 |
| M3 (서버+MCP+가이드) | API, LINK, MCP, MEM, RCL 전부, DB-01~27, CUST-01~04, RL/CACHE core, Claude/Codex를 포함해 최소 2개 호스트 통과, SEC-B/D/E/F/H/J 핵심 |
| M4 (웹+리더보드) | WEB, SCORE, PROFILE, USAGE 전부, rich-text/editor·handle 소유권·opt-in 공개·rank 분리 통과, owner quota/usage chart·author analytics·익명 no-score·self-use·receipt replay 통과, SEC-C/J abuse, SEC-D 웹 인증 |
| M5 (Gatekeeper 전체) | GK-20~48, QUE-01~24, CUR-01~21, MODEL-01~14 (실제 Meta provider 연결 포함), CUST-05~06, PERF-01~25 반복 측정, 전 호스트 스모크, 보안 시나리오 전체 (SEC-I/J 포함) |

### 수치 기준 (초안, 구현 전 합의 필요)

| 항목 | 기준 |
|---|---|
| 센티널 유출 | 0건 |
| 정상 입력 오탐 폐기율 | 합성 정상 세트에서 ≤ 5% (측정 후 조정) |
| 중복 판정 정확도 | 라벨 세트에서 ≥ 90% (동일/유사/신규/충돌 4분류) |
| `recall` p95 지연 | 로컬 캐시 적중 시 ≤ 50ms, 캐시 미적중 서버 왕복 ≤ 500ms (loopback 기준) |
| 힌트 크기 | 도메인당 최대 3개, 각 ≤ 400 estimated tokens |
| 성능 이득 | 반복 측정 평균에서 메모리 주입이 시간 또는 호출 수 개선, 성공률 비하락 |

## 8. CI 구성(안)

| 잡 | 내용 | 실패 시 |
|---|---|---|
| `unit` | L0 + clippy + fmt | 머지 차단 |
| `integration` | L1 + L2 | 머지 차단 |
| `database` | disposable PostgreSQL migrations와 DB-01~27 | 머지 차단 |
| `rate-limit` | disposable Redis, RL-01~11, SEC-J01~05, cross-instance boundary | 머지 차단 |
| `retrieval-smoke` | 고정 100-query subset, 10k memory version fixture, projection·parity·latency sanity | 머지 차단 |
| `retrieval-nightly` | 500-query corpus와 100k version/약 1.25M index row profile, query plan·load | 리포트; quality/security gate 미달은 릴리스 차단 |
| `fault-injection` | loopback 환경에서 FAIL-01~38의 오류 주입·복구·상태 일관성 | 머지 차단 |
| `model-contract` | MODEL-01~08, MODEL-10~14 (deterministic provider fixture/config checks) | 머지 차단 |
| `web-e2e` | L3 | 머지 차단 |
| `customer-paths` | loopback fixture와 일회용 계정·host profile에서 CUST-01~06 실행; Arise/Claude/Codex 실제 호스트 기록은 release smoke | 릴리스 차단 |
| `sec-core` | SEC-A, SEC-B, SEC-D, SEC-E, SEC-J 핵심 세트 (SEC-A20~22 포함) | 머지 차단 |
| `fuzz-nightly` | redaction/스키마 퍼저 | 이슈 자동 생성 |
| `host-smoke` | L4 (자체 러너, 호스트 CLI 설치 필요) | 릴리스 차단 |
| `bench-weekly` | L6 | 리포트 |

## 9. 열린 질문

1. 호스트 스모크를 CI에서 자동으로 돌릴 수 있는 호스트와 수동 체크리스트로 남길 호스트의 구분. (CLI 인증/구독 필요 여부)
2. 수치 기준(오탐률, 정확도, 지연)의 최종값.
3. 퍼저 시간 예산과 corpus 보관 방식.
