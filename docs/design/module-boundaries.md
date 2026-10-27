# 최소 모듈 경계와 테스트 전략

상태: 첫 수직 슬라이스 구현 완료 — wam-core, API 계약, Axum 서버, HTTP client, MCP stdio 읽기 도구, CLI, SvelteKit 리더보드·프로필 화면.

## 경계 원칙

각 컴포넌트는 서로 다른 이유로 바뀌고 다른 테스트 환경이 필요하다. 핵심 규칙은 I/O 없는 wam-core에 두며, HTTP·MCP·CLI·웹은 변환과 입출력만 담당한다. 서버 use case는 저장소 port를 통해 데이터에 접근하므로 테스트에서 메모리 저장소를 주입할 수 있다.

모든 외부 문자열은 어댑터에서 typed core 값으로 변환한다. 오류에 원 입력을 담지 않는다. URL은 canonicalize한 뒤 recall에 전달하고, 외부 프로필 링크는 HTTPS 주소 형식만 검증한다. URL preview를 위한 fetch는 없다.

## 현재 crate와 모듈

| 단위 | 책임 | 독립 테스트 |
|---|---|---|
| wam-core::url_scope | URL 개인정보 제거, 동적 세그먼트 정규화, exact/subtree/host 범위 | 문자열 fixtures |
| wam-core::keyword | 버전 있는 task/keyword vocabulary, NFKC 정규화 | 메모리 내 vocabulary |
| wam-core::memory | immutable memory index 제약 | typed 값 생성 |
| wam-core::recall | 범위/task/kind/status 필터, deterministic ranking | index slice + query |
| wam-core::profile | canonical handle, typed bio, HTTPS service link 검증 | AST 및 URL fixtures |
| wam-core::leaderboard | 개인 전체 순위, opt-in 공개 순위 | 합성 score fixture |
| wam-contracts | versioned JSON DTO, 닫힌 입력 schema | serde 계약 테스트 대상 |
| wam-server::application | recall/profile/dashboard/leaderboard use case | 주입된 Repository 구현 |
| wam-server::ports | 메모리·어휘·순위·프로필 조회 및 저장 인터페이스 | fake/in-memory repository |
| wam-server::http | Axum route, auth adapter, HTTP status/cache headers | router + oneshot 요청 |
| wam-server::memory_repository | 시드가 있는 프로세스 내 demo 저장소 | HTTP 통합 테스트 |
| wam-client | MCP·CLI 공용 async HTTP client와 WamApi port | endpoint validation, fake API 주입 |
| wam-mcp | stdin/stdout JSON-RPC MCP, status/recall/expand/leaderboard/my-rank 읽기 도구 | protocol dispatcher 단위 테스트 |
| wam-cli | status/vocab/recall/expand/leaderboard/rank 명령 | parser 및 출력 경계 단위 테스트 |
| web/src/lib/api.ts | 브라우저 typed fetch 및 안전한 외부 URL 검사 | Svelte check/build |
| web/src/lib/components | typed rich-text 편집·렌더링 | 컴포넌트 경계; 서버 검증 통합 테스트 |
| web/src/routes | API 기반 리더보드, 공개 프로필, 개인 순위/프로필 화면 | Svelte check/build |

서버 catalog는 seeded process-local memory이며, `LocalRepository`의 account/profile과 공개 설정은 `.wam-data/profiles.json`에 영속화한다. Contribution/review, identity/device, usage receipt/outcome과 score ledger도 각각 로컬 JSON file adapter가 있다. 본인 원장 내역은 인증 API와 dashboard에서 읽는다. 이 저장소들은 단일 개발 서버용이며 server publish 경로는 별도 wam-gatekeeper worker와 아직 연결되지 않았다. PostgreSQL server transaction/outbox와 host secure token storage는 남아 있다. 이 경계들을 연결할 때 기존 application port를 사용하고, 도메인 규칙을 adapter 안에 복제하지 않는다.

## DB 적용 전 필수 불변조건

Postgres queue lease에는 owner, expiry, 단조 증가 generation이 필요하다. publish 트랜잭션 안에서 현재 generation을 다시 비교해 stale worker를 막는다.

동의 철회가 publish보다 먼저 commit되면 공개 version이나 점수를 만들지 않는다. publish가 먼저 commit되면 withdrawal reevaluation job을 만든다. 이 race는 fake state-machine test로 끝내지 않고 disposable PostgreSQL 동시성 테스트에서 확인한다. 상세 주입 지점은 [failure matrix](../test/failure-matrix.md)와 [DB tests](../test/database.md)를 따른다.

## 확인 명령

1. cargo test --workspace: core, Gatekeeper, server route, MCP/CLI/client adapter.
2. cargo fmt --all -- --check와 cargo clippy --workspace --all-targets -- -D warnings.
3. pnpm --dir web run check 및 pnpm --dir web run build.
4. Postgres가 추가되면 disposable DB lease/consent race를 별도 실행한다.
5. 이후 web+server+fixture 브라우저 사용자 여정과 MCP host smoke를 추가한다.

개발 binary는 catalog만 seed로 시작하고 profile 변경과 opt-out은 파일에서 복원한다. 새 GitHub 계정의 기본 handle 충돌은 다른 고유 handle을 할당해 처리한다. 로컬 파일은 production 다중 서버 저장소나 DB transaction을 의미하지 않는다.
