# 미완료 기능 감사와 구현 계획

확인일: 2026-10-27. 코드, HTTP/MCP/CLI 표면, 웹 경로, 명세, 공유 이슈와 현재 테스트 실행을 대조했다. 목표는 로컬 demo slice를 실제 사용자 한 명의 기여가 검수·게시되고, 다른 사용자의 검증 가능한 성공 사용이 점수와 순위로 이어지는 운영 가능한 경로로 완성하는 것이다.

## 현재 검증된 상태

| 영역 | 현재 동작 | 남은 핵심 공백 |
|---|---|---|
| Core | URL/keyword 정규화, recall 순위, 프로필/bio/link 검증, 순위 projection | database-backed memory/index와 version lifecycle |
| Web | GitHub 로그인/로그아웃, 영속 profile/bio·공개 설정, 본인 점수 내역, 리더보드/순위, 기여 제출·철회, review UI, 기기 연결/관리 UI | 실계정 OAuth smoke, PostgreSQL account/profile/catalog, version analytics, consent 삭제·보관 정책 |
| Server | OAuth/device state, file-backed profile/공개 동의, contribution/review, usage receipt/outcome, +2 local score ledger와 본인 내역 API | catalog는 seed, account/profile/identity는 단일 서버 파일, production transaction/outbox 없음 |
| Gatekeeper | 분리된 curator/tool layer, in-memory + PostgreSQL memory/queue adapter, transactional lease/attribution fencing, withdraw/resolve/purge/stats, 계정별 후보 dedup을 포함한 scope-level joint curation, scripted fake, 로컬 Claude/Codex CLI, fixture 검증 HTTP Responses provider, versioned batch envelope, 5개 판정 유형 × 12개 URL/task 맥락의 60-case starter 회귀 코퍼스 | wam-server publish 연결과 consent transaction, live Meta/Muse 판정, 독립 라벨 60+ evaluation set |
| MCP / CLI | stdio·remote Streamable HTTP 공용 도구, propose/list/withdraw/outcome, 개인·공개 rank | remote OAuth discovery, CLI link/unlink/doctor/install과 OS token storage, 실제 host smoke |
| Data / privacy | consent version, URL canonicalization, local owner-bound receipt, aggregate metrics, durable Gatekeeper decision/audit | account/contribution consent persistence, server contribution/version purge worker, per-memory author analytics |
| Test / release | Gatekeeper `eval-60` fake 실행 60/60, Gatekeeper Clippy/fmt 통과, disposable PostgreSQL queue 3개/store 1개 통합 테스트 통과. 전체 workspace test와 fmt는 wam-server의 누락 `durable` 모듈 및 `usage_store.rs` 문법 오류에서 실패 | wam-server 오류 복구 후 전체 workspace 재검증, PostgreSQL load profile, live GitHub/host smoke, CI, 독립 labeled evaluation data |

현재 검증: `cargo run -p wam-gatekeeper --bin wam-curate -- --provider fake --scenario eval-60` 60/60 통과, `cargo clippy -p wam-gatekeeper --all-targets -- -D warnings`, `cargo fmt -p wam-gatekeeper -- --check`, disposable PostgreSQL Gatekeeper queue 3개와 store 1개 통과. `cargo test --workspace --quiet`의 unit/integration suites는 통과했으나 wam-server doctest compile에서 누락된 `durable` 모듈과 `usage_store.rs`의 매개변수 문법 오류로 전체 명령은 실패했다. `cargo fmt --all -- --check`도 wam-server의 누락 모듈에서 실패한다. Svelte check/build는 이전 실행에서 통과했으며 이번 변경은 Rust 범위다. Live GitHub/브라우저 host 경로와 PostgreSQL load profile은 아직 smoke하지 않았다.

공유 이슈 상태: #11~14의 local flow와 #16 프로필/공개 동의 restart persistence, #17 점수 내역 API/UI, #21 catalog 계약 테스트는 resolved. #10 테스트 환경/host adapter와 #15 Gatekeeper/Postgres adapter는 계속 추적한다. 출시 공백은 #18 publish/+20, #22 server DB transactions, #23 privacy 삭제/보관, #24 version analytics, #25 host 설치/계정 연결, #26 실제 사용 version별 점수 귀속으로 등록했다. Arise 자동화 탭/lease 불안정은 #27이다. #10의 실제 모델 host trace는 별도 비용 승인이 준비된 뒤 실행한다.

## 현재 데이터 흐름과 끊긴 경계

```text
Web contribution
  → contributions.json / local review state
  → in-memory JobQueue
  → [wam-gatekeeper worker와 연결되지 않음]
  → seeded InMemoryRepository [memory publish 없음]

Authenticated recall
  → usage.json receipt (memory/version/author attribution)
  → outcome=success
  → scores.json (+2 local event)
  → seeded points + local event 합산 순위
```

따라서 현재 점수는 seed memory author의 성공 사용만 반영한다. 수동 review의 `approved`는 게시 결정이 아니며 신규/개선 version +20도 발생하지 않는다.

## 우선순위와 순서

### P0 — 기여에서 게시까지의 신뢰 가능한 데이터 경로

**완료한 local profile 안전 보완 (#16):** 단일 프로세스 개발 서버에서 handle/bio/service links/opt-in과 새 GitHub 계정 profile을 원자적 파일 저장소에 기록한다. 저장 실패 시 memory 상태를 변경하지 않고, restart 및 opt-out 복원 회귀 테스트를 통과시킨다. 이는 PostgreSQL adapter 전까지 공개 범위가 restart로 초기화되는 위험을 막는 임시 local slice다.

1. **정책과 API 계약을 고정한다.**
   - Gatekeeper의 replace에 replay가 필수인지 독립 근거로 충분한지, create에 필요한 독립 기여자 수, 저신뢰 판단 시 review 기준을 결정한다. 관련 미결 결정은 `docs/spec/curator.md`와 이슈 #15에 기록한다.
   - 현재 한 receipt가 recall 결과 여러 memory/version을 담고 outcome 성공 시 각 알려진 author에게 점수를 시도한다. 실제로 어떤 version이 쓰였는지 확인할 수 있도록 version별 outcome 또는 receipt 내 사용 memory 목록을 계약에 추가하고, receipt membership을 서버에서 검증한다.
   - approve/reject가 review 상태인지 Gatekeeper publish decision인지 이름과 상태 전이를 분리한다.
   - **완료 기준:** version별 attribution, 중복, 철회, replace, reversal에 대한 contracts와 실패 테스트가 먼저 합의된다.

2. **Disposable PostgreSQL과 migration을 만든다.**
   - Docker/Colima daemon을 시작할 수 있는 개발 환경/CI job을 준비하고 migration runner 및 깨끗한 DB fixture를 추가한다.
   - users, profiles, devices, contributions/consent, gatekeeper jobs/decisions, immutable memory records/versions/authors, receipts/aggregates, score events, audit를 schema 명세대로 저장한다.
   - Gatekeeper memory/version/decision/audit 및 queue migration은 구현되었고, 나머지 application table migration은 아직 없다.
   - **완료 기준:** 새 DB migration/replay, unique/FK 제약, restart 지속성, 민감값 비저장 테스트가 동작한다. 운영 DB는 테스트 대상이 아니다.

3. **PostgreSQL repository/queue adapter를 구현한다.**
   - 서버의 Repository, IdentityStore, ContributionStore, UsageStore, ScoreStore와 Gatekeeper Store/Queue port를 어댑터로 구현한다.
   - contribution 저장+job enqueue, lease claim/commit, decision+publish, outcome+score/reversal은 transaction 또는 outbox로 묶는다. `SKIP LOCKED`, scope lock, lease generation, commit 시 consent/withdrawal 재확인을 적용한다.
   - Gatekeeper `PostgresStore`/`PostgresQueue`와 commit-time lease fence는 구현·테스트했다. wam-server application store와 contribution/queue transaction은 남아 있다.
   - **완료 기준:** 같은 nonce, 같은 scope 동시 worker, lease 만료 뒤 stale commit, server restart, worker 재시도에서 duplicate/lost publish나 score가 없다. 이 단계는 이슈 #15의 PostgreSQL adapter 작업과 맞춘다.

4. **Gatekeeper worker를 wam-server의 실제 publish 경로에 연결한다 (#18).** 이슈 #15의 engine/queue adapters와 별개인 server application integration이다.
   - contribution은 접수 후 sanitized job으로 변환하고, 현재 분리된 tool layer/worker에 전달한다. final schema, secret, scope, consent, replay 검사를 통과시킨다. 저신뢰/상충 결과는 사람 검수로 보낸다.
   - create/replace/retire는 immutable version 및 memory index를 만들고, decision, contribution 상태, author/source attribution과 active pointer를 원자적으로 갱신한다. 철회된 contribution이나 generation이 바뀐 worker 결과는 commit하지 않는다.
   - 새/검증 개선 version 승인 +20은 승인된 version author만 받으며 duplicate/support에는 지급하지 않는다.
   - **완료 기준:** submit → decision → publish/other result → catalog/recall 노출이 integration test로 검증되고, stale version·withdrawal·worker retry가 catalog를 오염시키지 않는다.

5. **Production receipt/outcome/score를 게시 version에 연결한다.**
   - receipt에는 발급 시점의 immutable version/author를 고정한다. 성공한 사용 version만 +2를 기록하며 현재 local 중복 방지 및 consumer/version·consumer/author 일일 제한을 DB 제약/transaction으로 옮긴다.
   - 점수 합계는 production에서 score event 원장만 source of truth로 사용한다. seed baseline은 test fixture로 제한한다. rollback/정책 위반 회수는 원 event를 삭제하지 않고 reversal event로 남긴다.
   - access/recall 통계와 score events는 분리하고, per-memory analytics에는 reader/receipt 식별자를 복제하지 않는다.
   - **완료 기준:** 다른 계정의 success만 +2, 자가 사용·익명·partial/failed·중복·만료·철회는 0점이며, restart/retry/동시 보고 뒤 원장 replay 결과가 동일하다.

### P1 — 실제 계정과 사용자 데이터 수명주기

6. **OAuth/device 경로를 production 계정 저장소에 연결한다.**
   - GitHub 실계정 OAuth smoke, session/CSRF/logout/revocation, device 승인/rotation/revoke를 확인한다. 현재 상태 머신과 UI는 로컬 JSON 개발 경로다.
   - identity/profile/account lifecycle을 PostgreSQL에 저장하고, secrets는 배포 secret manager에서 주입한다.
   - **완료 기준:** 테스트 계정 두 개로 owner isolation, opt-out, credential revoke, server restart를 검증한다.

7. **기여자 dashboard와 privacy lifecycle을 완성한다.**
   - 본인 점수 event history API/UI (#17)와 version별 recall/detail/outcome analytics를 연결한다. event 응답은 본인 beneficiary 기록만 반환하고 다른 소비자, receipt ID, 내부 식별자를 숨긴다. 본인 지급 근거·일시·version과 pagination은 구현하고 Arise에서 검증했다. memory-level analytics는 아직 없다.
   - consent withdrawal, pending payload purge, account deletion/deactivation, author attribution 재검토, retention job과 audit을 구현한다.
   - **완료 기준:** 계정 A/B 격리, 삭제/철회 후 read/write 차단, 공개 projection 즉시 제거, 보존기한 이후 payload 삭제를 integration test로 확인한다.

### P2 — CLI/MCP 설치와 host 연결

8. **CLI account linking과 운영 도구를 추가한다.**
   - `wam link/unlink`, `doctor`, `install`을 구현하고 device flow를 연결한다. access token은 OS secure storage에 두며 argv/config/log에 노출하지 않는다.
   - **완료 기준:** 임시 HOME/config와 OS test store에서 install/rotation/revoke/doctor를 검증한다.

9. **Remote MCP 인증과 host 지원을 마무리한다.**
   - 현재 remote Streamable HTTP는 stateless POST JSON과 scope 검증을 제공한다. 대상 host가 요구하는 OAuth discovery/GET-SSE 지원을 결정하고, 구현할 경우 spec-conformance 및 cancellation/resume 동작을 테스트한다.
   - Claude Code/Codex의 실제 MCP 등록 및 tool-call trace, Arise MCP Hub 경로, Chrome browser tool과 WAM MCP 공존을 확인한다. WAM browser-control transport와 브라우저 MCP를 혼합하지 않는다.
   - **완료 기준:** 각 host에서 anonymous read, linked private rank, authorized propose/outcome, revoked credential의 독립 smoke evidence가 있다.

### P3 — 평가와 출시 준비

10. **Gatekeeper/retrieval evaluation set을 확보한다.**
    - Gatekeeper CUR-15는 현재 template-derived 60-case starter를 독립 사람이 검토한 60+ labeled job으로 확장하고 live Meta/Muse scoring을 한다. Retrieval은 계획된 labeled query/held-out split, injection/leak corpus와 UI drift fixtures를 준비한다.
    - batch format 변경 뒤 CUR-14 re-evaluation을 수행하고, HTTP provider는 provider-specific quota/Retry-After 오류를 fake server로 검증했다. 실제 Meta/Muse 연결은 배포 credential과 승인된 live run이 필요하다.
    - **완료 기준:** benchmark가 seed, provider/model, policy/tool hash, invalid-run 사유를 기록하고 결과를 재현한다.

11. **CI와 end-to-end release gate를 둔다.**
    - CI에서 Rust test/Clippy/fmt, Svelte check/build, disposable PostgreSQL migration/integration을 실행한다.
    - Playwright 사용자 흐름과 MCP Inspector/host smoke를 별도 job으로 분리하고, 실계정·모델 호출 job은 수동 실행 및 비용 표시를 적용한다.
    - **완료 기준:** 아래 대표 여정의 모든 경계를 독립 oracle로 확인한다.

## 끝에서 끝까지의 출시 기준

웹 계정 A가 동의하고 contribution을 제출한다 → Gatekeeper가 판단하고 새 immutable version을 attribution과 함께 게시한다 → 다른 계정 B가 해당 version을 recall하고 선택 사용을 보고한다 → B에게는 점수가 발생하지 않고 version author A에게 정책상 점수가 한 번만 발생한다 → dashboard/leaderboard가 같은 원장 합계를 보인다. 익명 사용, A의 자기 사용, 실패/부분 결과, 중복 report, 철회/폐기된 version은 점수를 만들지 않는다. 전체 경로가 DB restart·재시도·동시 호출 뒤에도 같은 결과를 낸다.

## 이번 보완의 검증 증거

- Rust workspace: 177 pass / 0 fail / 5 ignored. 제외 5개는 실제 Claude/Codex 호출 2개와 disposable PostgreSQL adapter 테스트 3개다. 이 수치는 공유 트리의 해당 실행 시점 결과다.
- Strict Clippy와 rustfmt 검사를 수행했고, Svelte check는 오류/경고 0, production build는 성공했다.
- Arise GUI+MCP: 합성 데이터 대시보드 20→25개 페이지 이동, 마지막 버튼 제거, reader/receipt 비노출, 잘못된 인증 재조회 후 이전 데이터 제거, handle/opt-out 저장·새로고침·API 재시작 복원, 공개 404와 leaderboard 제외를 확인했다.
- rich bio/service links의 restart round-trip과 손상 파일/저장 실패/권한 검사는 Rust HTTP/File 회귀 테스트로 검증했다. Arise editor 전체 여정·모바일·실계정 OAuth 및 WAM MCP Hub 연결 acceptance는 이번 UI 검증에 포함되지 않는다. 추가 UI 자동화 중 target tab이 about:blank로 바뀐 문제는 #27로 분리했다.

## 명세 정리 항목

일부 `docs/spec/*` 상단 상태 문구는 현재 코드보다 오래된 설명을 남긴다. 특히 CORE/RECALL/MEMORY/INSTALL/DEDUP/GATEKEEPER 상태와 API/MCP/WEB 구현 범위를 하나의 capability matrix와 맞춰 정리한다. 각 요구사항은 **미구현 / local slice / production-ready / live verified** 중 하나로 표시하고, 테스트 증거 링크를 둔다.
