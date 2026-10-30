# 검색·구조화 메모리 성능 및 보안 검증 환경

상태: 검색 회귀용 합성 corpus와 evaluator가 구현됐다. 성능 규모별 DB corpus와 성능 측정, 독립 사람 라벨은 아직 남아 있다. 기능 의미는 [`../spec/core.md`](../spec/core.md), [`../spec/memory.md`](../spec/memory.md), [`../spec/recall.md`](../spec/recall.md)가 정하고, 이 문서는 검색 pipeline을 모듈별로 검증하고 데이터·부하·보안 시험을 재현하는 방법을 정한다.

## 1. 목적과 적용 범위

WAM recall은 URL scope, task, keyword, kind, lifecycle status를 이용해 올바른 L0 색인을 찾고, 결정적인 순서로 최대 3개의 L1 힌트를 돌려준다. 정확성, 구조 유효성, 개인정보 보호, 검색 속도를 하나의 end-to-end 응답만으로 판단하지 않는다. 모듈 경계별 테스트와 같은 입력을 여러 저장·통신 adapter로 통과시키는 parity 테스트를 둔다.

이 검증은 웹 작업을 메모리로 더 빨리 끝내는 사용자 작업 벤치마크와 별도다. browser 조작 시간·작업 성공률은 [`benchmarks.md`](benchmarks.md), 검색 알고리즘·DB/API 지연은 이 문서와 [`metrics.md`](metrics.md)가 다룬다.

## 2. 검색 pipeline과 모듈 테스트 경계

한 정책을 여러 계층이 다시 구현하지 않는다. 모듈 책임의 설계 기준은 [`../design/module-boundaries.md`](../design/module-boundaries.md)다. `wam-core::recall`이 route/task/kind/status 필터와 최종 순위 규칙의 기준이고, DB repository는 인덱스로 후보를 좁히되 유효한 후보를 누락하지 않아야 한다.

| 단계/모듈 | 입력 | 출력 | 격리 검증 |
|---|---|---|---|
| `url_scope` | URL 또는 route pattern | canonical `site_host`와 exact/subtree/host scope | 순수 단위·property test; query, fragment, userinfo 제거, 동적 ID 패턴, 잘못된 scope |
| `keyword` | 키워드·동의어·vocabulary version | NFKC 정규화된 허용 토큰 | 한국어/영어, 별칭, 중복, 순서, 길이, zero-width/기만 문자열 |
| `memory` | versioned L0/L1/L2 | typed record와 L0 index projection | JSON schema, 교차 필드 제약, 크기, kind별 필수 필드, round trip |
| `recall` | canonical URL, task, keyword, kind, 후보 index | 유일한 memory ID 목록과 결정적 rank | golden query, 순위 동점, 교차 host/task 분리, 입력 순열 불변성 |
| repository | DB memory/index row와 canonical query | core가 평가할 완전한 후보 집합 | disposable PostgreSQL, index query plan, active-version join과 reindex 일관성 |
| HTTP/MCP/CLI adapter | 외부 DTO | 같은 core query/result 계약 | schema 오류, auth, transport별 응답 변환, 결과 ID/version/rank parity |

테스트 계층은 core unit/property → in-memory repository 통합 → PostgreSQL 통합 → HTTP/MCP/CLI 계약 → 부하·보안으로 진행한다. 상위 단계가 실패하면 하위 단계의 결과만으로 제품 검색 통과를 선언하지 않는다.

## 3. 구조화 메모리 fixture와 불변조건

테스트 record는 실제 내용 대신 합성 host와 결정적 필드만 사용한다. 예시의 `naver.localhost`는 격리 테스트에서만 쓴다.

```json
{
  "schema_version": 1,
  "l0": {
    "kind": "workflow",
    "site_host": "naver.localhost",
    "url_patterns": ["/editor/post/:id"],
    "task_category": "compose_draft",
    "intent_key": "write_blog_draft",
    "keywords": ["naver", "blog", "write"]
  },
  "l1": {
    "title": "편집기 초안 저장",
    "summary": "편집기에 초안을 작성하고 임시 저장 상태를 확인한다.",
    "key_points": ["입력 뒤 저장 상태를 확인한다."],
    "pitfalls": []
  },
  "l2": {
    "preconditions": ["fixture_editor_open"],
    "steps": [{"action": "click", "note_code": "save_draft"}],
    "structure": {"page_types": ["editor"]},
    "recovery": [],
    "detail": "fixture saved-oracle 통과"
  }
}
```

위 블록은 저장된 JSON envelope의 예다. `schema_version`은 저장 envelope에만 있고, Gatekeeper tool의 `record` 인수는 [`../spec/curator/tools.v1.json`](../spec/curator/tools.v1.json)의 `$defs.record`에 따라 `l0/l1/l2`만 가진다. 두 형식을 각각 검증하고 명시된 변환만 허용한다. example code 값은 fixture 전용 closed-code seed로 제공한다.

정형 검증은 최소 다음 불변조건을 검사한다.

- L0/L1/L2는 같은 `schema_version`과 immutable version에 묶인다. 메타데이터(`id`, `version`, `status`, `verified_at`, `confidence_band`)를 JSON 내용에서 덮어쓰지 않는다.
- `memory_index`는 승인된 version의 L0에서만 만든다. 중복 입력이나 재실행은 같은 index projection을 만들고, 교체·rollback 뒤 이전 active projection은 기본 recall에서 빠진다.
- 정규화한 task/category/kind는 등록 enum만 받는다. 미지의 필드, 잘못된 타입, 유효하지 않은 UTF-8, 중복 JSON key, 빈 scope, 상한 초과는 거부한다.
- keywords는 같은 version 안에서 정규화·dedup하며 공개 vocabulary에 있는 제한 토큰만 유지한다. 자유형 task description과 사용자 prompt는 색인에 들어가지 않는다.
- 서로 다른 host, sibling route, task, kind를 조합해도 scope가 의도치 않게 합쳐지지 않는다. 상위 route는 L0가 하위 범위를 명시한 경우에만 적용한다.
- 동등한 의미의 JSON 필드 순서·배열 중복·공백 차이는 canonical content/fingerprint/index를 바꾸지 않는다. 의미가 다른 route/task/version은 서로 다른 key를 가진다.

관련 component 케이스는 `MEM-01~11`, `CORE-22~26`, `DB-05~06`을 기반으로 하며 보강 케이스 `MEM-12~15`, `DB-20`을 추가한다.

## 4. 검색 회귀 corpus와 정답 라벨

### 4.1 보관 형식

`tests/fixtures/retrieval/`에 manifest, synthetic records, queries, labels를 고정한다. `tests/lib/wamtest/retrieval_corpus.py`가 corpus와 라벨을 만들고, `tests/bench/retrieval.py`가 생성·검증·채점 CLI를 제공한다. 모든 host는 예약된 `example.com`, `example.net`, `example.org` 하위의 합성값이며 실사용 URL·로그·계정·prompt는 포함하지 않는다.

| 산출물 | 각 row에 포함할 것 |
|---|---|
| `manifest.json` | generator version, seed, schema/vocabulary version, record/index row 수, 분포 요약, 파일별 hash와 label provenance |
| `records.jsonl` | structured L0/L1/L2와 version/status fixture |
| `queries.jsonl` | query ID, URL fixture, task, kind, allowlist keywords, warm/cold tag와 train/heldout split |
| `labels.jsonl` | query ID, 관련 memory ID와 relevance grade, 기대 scope/rank 조건 |
| `expected.jsonl` | golden top-3 ID/version, 빈 결과 reason, route/task/keyword 매칭 정보 |

생성·검증:

```sh
python3 -B tests/bench/retrieval.py generate
python3 -B tests/bench/retrieval.py validate
PYTHONPATH=tests/lib python3 -B -m unittest selftest.test_retrieval_corpus -v
```

현재 산출물은 600개 record / 1,500개 index projection / 500개 query이며 100개를 group 단위 held-out으로 둔다. Query는 task/kind/exact route 및 sibling route를 확인하고, negative class는 wrong kind와 없는 host를 나눈다. 다른 host·task·kind·retired/superseded record는 decoy로 포함한다. 라벨은 생성 규칙으로 만든 합성 정답이며 `human_reviewed: false`로 명시한다. 이 데이터만으로 현실적인 BM25 의미 품질이나 독립적인 precision/recall을 주장하지 않는다. 사람 검수와 다양한 route/keyword 경계가 더 필요하다.

secret 말뭉치는 retrieval corpus에 복제하지 않는다. `tests/sentinels.json`과 `tests/security/inj-corpus/`는 별도 보안 corpus 범위로 남아 있다.

### 4.2 라벨 corpus 구성

RCL 품질 corpus는 최소 500 query를 고정한다. 현재 PR smoke는 group을 보존한 100개 held-out subset을 사용하고, release/nightly에서는 전체 corpus를 쓴다. 20% held-out은 tuning에 쓰지 않는다. 현재 synthetic set은 범위 필터 회귀 fixture이며, 독립 평가를 위해서는 다음 항목을 사람 검수 라벨로 보강해야 한다.

라벨은 다음 범위를 균형 있게 포함한다.

- 경로: exact route, 안전한 상위 route, host fallback, sibling route, 다른 host, 잘못됐거나 민감한 URL.
- 업무: task 일치, task 없음, 다른 task, unknown enum 거부.
- 키워드: 영어, 한국어, 혼합, 허용 별칭, 단어 순서 변화, vocabulary 밖 토큰, 빈 키워드.
- 상태/종류: active, `needs-reverify`, superseded, retired와 workflow/site_note/gotcha.
- 질의 분포: 고유 host 분산 질의, 한 host에 집중한 hot set, no-match, 동점 및 경계 score.
- URL 개인정보: 동적 ID, query sentinel, fragment, userinfo, Unicode hostname/path 변형.

정답 라벨은 memory ID, acceptable set, relevance grade, 허용 적용 범위, 제외되어야 할 host/task를 포함한다. evaluator는 top-3만 비교하지 않고 전체 scope leakage와 retired/inactive 반환도 검사한다.

## 5. 검색 성능 fixture 규모와 workload

1k/10k/100k/1M 규모의 DB performance corpus generator는 아직 구현되지 않았다. 이를 추가할 때 같은 seed가 같은 record, query, label hash를 생성해야 한다. 현재 600-record 회귀 corpus는 그 성능 규모를 대체하지 않는다.

| 프로필 | memory version 수 | `memory_index` 예상 행 | 용도 | 주기 |
|---|---:|---:|---|---|
| smoke | 1,000 | 약 12,500 | 기능·정답·인덱스 갱신 | PR |
| ci | 10,000 | 약 125,000 | query plan과 기본 latency 회귀 | PR 성능 잡 |
| release | 100,000 | 약 1,250,000 | API/DB 성능 gate | 릴리스 전 |
| stress | 1,000,000 | 약 12,500,000 | 메모리·tail latency·부하 한계 관찰 | 야간 또는 수동 |

한 index row는 하나의 unique `(version_id, site_host, url_pattern, kind, task_category, keyword)` tuple이다. generator는 1~4 route pattern(주기 4)과 2~8 keyword(주기 7)를 독립적으로 순환해 route×keyword projection 평균 약 12.5행/version을 만든다. 표의 행 수는 근사치이며 실제 수와 manifest hash를 매 run에 기록한다. active/needs-reverify/inactive 상태를 섞고, 여러 host를 균일하게 두는 분포와 일부 hot host에 query가 몰리는 분포를 모두 둔다. 성능 보고서에는 memory version 수와 index row 수를 따로 기록한다.

기본 search workload는 exact route 50%, 상위 route 20%, host fallback 10%, no-match/잘못된 범위 20%로 구성한다. keyword 요청은 한/영/혼합을 포함하고 task와 kind가 있는 쿼리와 없는 쿼리를 모두 둔다. API 부하는 기존 `metrics.md`의 loopback 100 RPS × 5분 조건과 동시 실행 1/10/50/100을 사용한다. cache cold와 warm을 별도 run으로 보고한다.

## 6. 재현 가능한 성능 실행 환경

1. 같은 dedicated runner class에서 실행하며 CPU 아키텍처·코어·메모리, OS image, `rustc -Vv`, Cargo.lock hash, PostgreSQL exact version/container digest, 주요 DB 설정, schema migration hash를 기록한다. benchmark 중 다른 load job을 함께 돌리지 않는다.
2. Rust release profile로 빌드한다. PR에서 smoke/10k를, 야간에서 100k와 stress를, 릴리스 전에는 최소 100k를 실행한다. 1M stress가 실패해도 작은 profile 결과와 합치지 말고 별도 보고한다.
3. disposable PostgreSQL을 만들고 같은 migration을 적용한다. generator로 seed하고 `ANALYZE`한 뒤 query plan과 row/index cardinality를 snapshot으로 남긴다. seed에 실제 고객 정보가 섞이면 실행 자체가 무효다.
4. core microbenchmark는 DB/API 없이 `wam-core::recall` 후보 수와 top-3 계산을 측정한다. DB benchmark는 repository query만 측정한다. API benchmark는 loopback HTTP 왕복과 JSON encode/decode를 포함한다. MCP/CLI parity는 동일 query로 ID/version/order가 같은지만 따로 확인한다.
5. cold run은 프로세스 및 application cache를 비우고 첫 요청부터 측정한다. warm run은 고정된 warm-up query를 수행한 뒤 측정한다. 운영체제 page cache를 실제로 비우지 않았다면 “DB cold”라고 쓰지 않는다.
6. 각 profile·cache mode는 warm-up 뒤 최소 5회 반복한다. request order는 corpus seed로 섞고 각 run은 독립 profile/DB snapshot을 쓴다. 평균뿐 아니라 p50/p95/p99, 분산, error rate와 throughput을 남긴다.
7. recall 정확도 gate를 먼저 적용한다. 빠르게 응답해도 expected set, task/host 범위, inactive filtering 중 하나라도 틀리면 성능 합격이 아니다.

저장할 증거는 `target/e2e-logs/retrieval/<profile>/<run-id>/` 아래 manifest, build metadata, latency histogram, query plan, quality summary, error summary로 제한한다. query의 합성 ID만 보존하고 원 URL, keywords가 사용자 프롬프트에서 왔음을 추측할 정보, secret 원문은 기록하지 않는다.

## 7. 성능·품질 지표 연결

기준 숫자는 [`metrics.md`](metrics.md)를 단일 출처로 삼는다. 현재 명세의 기본 게이트는 전체 500-query 라벨 corpus(`precision@3 ≥ 0.90`, `recall@3 ≥ 0.85`), cross-host leakage 0, cached recall p95 ≤50ms, loopback recall p95 ≤500ms, 100 RPS 조건의 오류율 ≤0.1%다. DB-only p95 ≤100ms는 100k version/약 1.25M index row 전용 runner에서 측정하는 목표값이다. `metrics.md`와 이 문서의 workload가 어긋나면 실행 전에 둘을 함께 수정한다.

품질 report는 precision@3, recall@3, exact-route top-1, MRR@3, cross-host/task leak 수, inactive hit 수를 나눈다. latency report는 core-only, DB-only, API end-to-end, MCP/CLI parity를 분리한다. cache hit ratio, DB rows examined/returned, index scan type, RSS와 PostgreSQL buffer hit ratio를 부가 지표로 남긴다.

## 8. 비밀값·PII·악성 입력 처리 계약

“마스킹”의 의미를 경계별로 고정한다.

| 입력 위치 | 처리 규칙 | 허용되는 저장/로그 |
|---|---|---|
| recall URL | local CLI 또는 검증된 host adapter가 전송 전에 query/fragment/userinfo와 식별자를 정규화한다. 모호하면 요청을 보내지 않거나 안전한 빈 결과로 종료한다. | canonical route pattern과 안전 reason code만 |
| keyword/task | 공개 vocabulary·closed enum 밖 값은 local에서 제거 또는 schema error로 거부한다. 자유 prompt는 전송하지 않는다. | allowlist token/enum만 |
| 기여 L0/L1/L2 content | secret, credential, PII, 위험 instruction 검출 시 해당 contribution/candidate 전체를 거부하고 원문을 버린다. 본문에 `[REDACTED]`를 넣고 나머지를 게시하지 않는다. | 고정 오류 `TEXT_REJECTED`와 필요할 때 coarse classifier `secret_or_pii_suspected`만 |
| 서버 log/error | 요청 body, raw URL/query, authorization header를 echo하지 않는다. 필요한 필드는 구조화 allowlist로 기록한다. | correlation ID, 단계, 공개 error code, coarse classifier category |
| cache/index/evidence | 비정규 URL이나 비검증 payload를 저장하지 않는다. | canonical L0 projection/hash 등 schema가 허용한 필드만 |

즉, 비밀 텍스트를 찾아 지우고 같은 레코드를 계속 저장하는 방식은 contribution/memory 경로에서 허용하지 않는다. 진단 화면이 local preview에서 마스킹된 형태를 표시할 수는 있으나, masked snippet이 서버 payload·DB·public memory로 이동하면 실패다. 일반 원격 MCP에 모델이 URL을 다듬어 주길 기대하는 것만으로 raw URL의 전송 전 보호를 보장하지 않는다.

보안 corpus는 키/token/JWT/private-key 형식, 이메일·전화·ID, 내부 경로, query token, Unicode confusable/zero-width, base64/URL/hex/double encoding, JSON 필드 간 분할, L0/L1/L2 교차 삽입, 악성 instruction을 포함한다. 정상 client 테스트에서는 outbound request capture에 원본 secret이 없어야 한다. 직접 API 우회 시나리오의 inbound body는 격리 하네스가 만든 악성 입력이므로 raw capture를 증거 파일에 저장하지 않고, 일시 처리 뒤 응답·DB/WAL(application level)·queue·logs·panic/error·cache·process args·benchmark evidence·web source에 남지 않는지 검사한다. 하네스는 sentinel 원문 대신 테스트 case ID와 탐지 여부만 증거로 보존한다.

기존 `CORE-10~19`, `GK-03~08`, `SEC-A01~18`, `SEC-B01~12`, `SEC-E`, `FAIL-01/02/03/32`에 더해 `SEC-A20~22`로 reject-vs-mask 경계와 recall ingress sink를 확인한다. `SEC-A19`는 실사이트 벤치 산출물의 민감 정보 부재를 확인한다. 공격 payload는 loopback과 합성 sentinel에만 쓴다.

## 9. 추가 component·database·performance 케이스

| ID | 시나리오 | 합격 oracle |
|---|---|---|
| MEM-12 | JSON schema와 serialize/deserialize 왕복, 필드 순서·공백 변화 | 같은 typed record와 canonical projection/fingerprint; unknown field/type은 거부 |
| MEM-13 | immutable version에서 L0 `memory_index` projection 재생성 | 같은 seed/version은 같은 projection이며 재실행 중복 0 |
| MEM-14 | L0 keyword/task/url pattern과 L1/L2 필수 교차 필드 누락·불일치 | 저장 전 거부, 부분 record/index 0 |
| MEM-15 | L0/L1/L2 각각 및 필드 간 분할 sentinel | 전체 candidate 거부, raw sink 0, public memory 0 |
| RCL-17 | 동일 query를 core exhaustive scan과 DB candidate repository로 실행 | 후보 ID 집합이 같고 core가 계산한 rank/top-3와 완전히 일치 |
| RCL-18 | 저장·입력 순서, duplicate index row, page size를 바꿔 반복 | 동일 memory는 한 번만 반환되고 ID/version/rank가 결정적으로 동일 |
| RCL-19 | 생성한 host/path/task 조합에 property test 적용 | 다른 host/task/sibling route 결과 leakage 0; upper-scope는 명시된 경우만 일치 |
| RCL-20 | vocabulary/schema version 전환과 synonym lookup | 같은 버전 내 한/영 alias 일치; version mismatch는 명시적 거부 또는 호환 변환 |
| RCL-21 | publish/supersede/retire/rollback 직후 cache와 index 조회 | 새 active version만 top-3에 반영; stale inactive hit 0 |
| RCL-22 | 500-query fixed/held-out corpus와 normalized result rows | precision/recall, empty-result, scope/cross-host/task/kind/inactive gate 통과. 현재 기계 생성 라벨은 harness contract 확인용이며 독립 제품 품질 gate가 아니다 |
| RCL-23 | 동일 URL scope 안에서 제목·intent·summary의 어휘 검색 점수 비교 | title/intent 가중 BM25와 RRF가 관련 메모리를 상위에 두며 다른 host는 결과에 포함하지 않는다 |
| RCL-24 | 메모리 fixture 입력 배열을 여러 순서로 섞어 같은 query 반복 | memory ID/version/order와 정수 `score`가 매번 동일하다 |
| PERF-13 | fixed seed generator를 같은 toolchain에서 두 번 실행 | retrieval fixture의 manifest/file hash와 record/index/query/label 수가 동일; 대규모 DB profile generator는 미구현 |
| PERF-14 | core-only search에서 1k/10k/100k fixture 실행 | latency histogram과 RSS 기록; 정확도 변동 0; `metrics.md` core 목표와 비교 |
| PERF-15 | 100k version/약 1.25M DB index rows에서 exact/prefix/keyword lookup과 `EXPLAIN` 수집 | scope와 keyword index를 이용하는 plan; full scan 발생 시 사유를 설명하고 regression으로 기록 |
| PERF-16 | loopback API 100 RPS × 5분, 1/10/50/100 concurrency | recall 순위/응답 schema 일치, 기존 p95/error-rate gate 충족 |
| PERF-17 | app cache cold/warm 및 ETag 304에서 동일 query 실행 | warm/cold 결과 동일; hit/miss/p95 분리 기록; stale version 반환 0 |
| PERF-18 | exact/prefix/no-match/hot-host 혼합 부하 | 각 query class의 p50/p95/p99, error, host/task leakage 분리 보고 |
| PERF-19 | recall 부하 중 version publish/reindex/rollback 주입 | transaction 경계가 보이고 중복·부분 index 0; 결과는 active version contract를 따른다 |
| PERF-20 | 동일 build·seed 반복 benchmark | 5회 결과와 변동계수 기록; CI guard 초과는 원인 분석 전 무시하지 않는다 |

DB 저장/인덱스 통합은 [`database.md`](database.md)의 DB-01~24, multi-layer fault cases는 [`failure-matrix.md`](failure-matrix.md)를 함께 적용한다.

## 10. 실행 게이트와 연결 문서

- PR 필수: CORE/MEM/RCL unit·property, fixed 100-query smoke, 10k dataset correctness와 latency regression, secret sink scan.
- Nightly: 전체 500-query labels, 100k version/약 1.25M index row dataset, load/concurrency, fuzz corpus, PostgreSQL plan snapshot.
- Release 전: 100k version/약 1.25M index row dataset, 100 RPS × 5분, cold/warm, host adapter parity, 전 SEC sentinel sink sweep. 1M stress는 별도 결과로 기록한다.
- 성능 기준이 맞더라도 검색 quality, secret zero-leak, cross-host isolation, deterministic rank 중 하나라도 깨지면 출시 불가다.

Normalized adapter 결과는 query마다 정확히 한 행인 `{"query_id":"rcl-000-compose","memory_ids":["mem-000-compose"]}` JSONL로 저장한다. 메모리 ID는 corpus record에 있어야 한다. 다음 명령은 precision@3, recall@3, MRR@3, top-1/exact-top-3, 빈 결과 정확도와 scope/cross-host/task/kind/inactive hit 수를 따로 평가한다.

```sh
python3 -B tests/bench/retrieval.py score --results target/e2e-logs/retrieval/results.jsonl
```

이 scorer와 합성 fixture는 검색 adapter 계약을 확인한다. DB/API 지연, 대규모 scale gate, 사람 검수 품질은 별도 실행이어야 한다.

기능 요구사항은 [`../spec/memory.md`](../spec/memory.md), [`../spec/recall.md`](../spec/recall.md), [`../spec/core.md`](../spec/core.md), [`../spec/database.md`](../spec/database.md)를 따른다. 실행 결과와 환경 갱신은 [`environment.md`](environment.md)에 별도 기록한다.
