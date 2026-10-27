# 최소 모듈 경계와 테스트 전략

상태: wam-core 1차 구현. 서버·게이트키퍼·MCP·CLI·웹은 아직 실행 가능한 앱이 아니다.

## 먼저 분리해야 하는 이유

각 부분은 바뀌는 이유와 필요한 테스트 환경이 다르다. 핵심 규칙을 network, database, model SDK, browser, UI에 묶으면 경계 하나의 실패를 좁게 재현하기 어렵다. 현재 구현은 공통 도메인 규칙을 I/O 없는 Rust crate로 먼저 고정한다.

## 현재 구현: wam-core

| 모듈 | 책임 | 독립 테스트 |
|---|---|---|
| url_scope | HTTPS/loopback 정책, URL 개인정보 제거, 동적 세그먼트 정규화, 명시적 exact/subtree/host 적용 범위 | 고정 문자열 fixture. 외부 DNS나 웹 요청 없음 |
| keyword | 버전이 있는 task/keyword allowlist, NFKC 정규화 | 작은 메모리 내 vocabulary fixture |
| memory | immutable memory index의 필수값·host·중복·개수·confidence 검증 | typed 값 생성; DB 불필요 |
| recall | host/scope/task/kind/status 필터, 점수와 결정적 tie-break | index slice + query; 저장소나 시계 불필요 |
| profile | canonical handle, typed rich-text bio, HTTPS 서비스 링크 검증 | AST와 URL fixtures; UI/DB/network 불필요 |
| leaderboard | 본인 전체 순위와 opt-in 공개 cohort rank projection | 합성 account-score slice; DB/시계 불필요 |

모든 외부 문자열은 어댑터에서 wam-core의 타입으로 변환한 뒤 사용한다. 오류에는 원 입력을 담지 않는다. recall은 최대 3개, 명시적 route scope, 승인된 task/keyword vocabulary를 전제로 하는 순수 함수다. MCP와 HTTP는 같은 결과를 DTO로 바꾸며 별도 순위 정책을 구현하지 않는다.

Opaque account path segment는 신뢰한 host adapter가 route 규칙으로 지정한 경우에만 :account로 바꾼다. 기본 파서는 email, 숫자 ID, 표준 UUID를 처리하고, 비밀값처럼 보이거나 path 경계가 모호한 값은 거부한다. 현재 공개 host 검증은 문법과 예약 local suffix에 한정하며 등록 여부나 DNS 해석을 증명하지 않는다. API가 URL을 직접 fetch해서는 안 된다. 나중에 URL fetch가 생기면 별도 SSRF·DNS pinning 경계를 둔다.

## 구현할 어댑터 경계

| 컴포넌트 | 포함할 것 | 테스트 더블·테스트 |
|---|---|---|
| wam-server | HTTP DTO/auth middleware → application use case → Postgres adapter. use case가 MemoryRepository, ContributionQueue, ReceiptRepository, ProfileRepository, Clock 포트를 사용 | use case는 fake repository/clock으로 검증; PostgreSQL 제약·동시성은 disposable DB 통합 테스트 |
| wam-gatekeeper | job orchestration, Meta provider adapter, replay harness adapter, deterministic publish policy | provider와 replay는 trait으로 주입; fake provider는 invalid JSON·timeout·429·적대 출력 재현; commit은 DB lease fencing transaction으로 검증 |
| wam-mcp | MCP schema와 protocol을 공통 application/API 호출로 변환 | in-memory transport에서 initialize/tools/list/call 계약; core 로직 복제 금지 |
| wam-cli | 설치·진단·recall 명령과 host config adapter | 임시 HOME/config directory 및 fake HTTP; 실제 사용자 설정은 사용하지 않음 |
| web | SvelteKit routes/components와 generated/typed API client. profile rich-text editor와 leaderboard/profile route 포함 | editor schema/render tests + loopback Playwright journeys; DB를 직접 호출하지 않음 |

포트는 테스트를 쉽게 하려는 명목으로 모든 함수를 trait으로 만들지 않는다. 외부 상태를 읽거나 쓰고 독립 대체가 필요한 경계(DB, clock, model provider, replay, host config, HTTP transport)에만 둔다.

## 큐와 취소의 필수 DB 불변조건

큐 lease에는 owner, expiry, 단조 증가 generation이 필요하다. 워커는 lease token을 받지만 publish 트랜잭션 안에서 현재 generation을 비교해야 한다. 단순히 Rust 메모리에서 generation을 비교하고 나중에 저장하면 stale worker 경합을 막지 못한다.

동의 철회가 publish보다 먼저 commit되면 새 공개 version과 author point를 만들지 않는다. publish가 먼저 commit되면 별도 withdrawal reevaluation job을 만든다. 이 race는 fake state-machine test만으로 끝내지 말고 disposable PostgreSQL의 동시성 테스트에서 검증한다. 상세 주입 지점은 [failure matrix](../test/failure-matrix.md)와 [DB tests](../test/database.md)에 연결한다.

## 테스트 실행 순서

1. Pure core: cargo test -p wam-core.
2. Application use case: fake ports로 성공·권한·동의·멱등성·오류 흐름을 검증.
3. DB transaction: disposable PostgreSQL, barrier로 lease/consent race 재현.
4. Protocol adapters: HTTP OpenAPI 및 MCP tools/list·tool call contract.
5. Product journeys: SvelteKit + loopback server + fake OAuth + independent browser fixture oracle.
6. Host smoke: 일회용 CLI profile에서 설치/연결/정리. 사용자 profile은 건드리지 않는다.

처음 crate 경계를 넘어 코드를 만들 때도 기능을 모두 한 번에 추가하지 않는다. 공통 contract가 바뀌면 core integration tests를 먼저 고친다.