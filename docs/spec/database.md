# DB: PostgreSQL 스키마와 영속성 계약

상태: Gatekeeper 자체 PostgreSQL adapter에 더해 wam-server application migration/adapter가 구현됐다. 물리 테이블과 실행법은 [`crates/wam-server/README.md`](../../crates/wam-server/README.md), migration은 `crates/wam-server/migrations/0001_application.sql`이다. Identity/profile/contribution/consent/catalog/version/authorship/receipt/score/work/audit/outbox를 보존하고 복합 상태 변경을 원자적으로 처리한다. 현재 application adapter는 짧은 schema 단위 write lock과 snapshot projection을 사용한다. 대규모 부하·검색 성능 합격은 아직 주장하지 않는다.

로컬 모드는 `.wam-data/application.json` 하나에 관련 상태를 함께 저장하고 OS file lock으로 복수 프로세스 접근을 막는다. 이전 다섯 파일은 검증·원자 저장 뒤 정리한다. PostgreSQL 모드는 관계형 PK/FK/unique, immutable version/score facts, receipt/author 기반 지급 조건과 일일 unique quota를 추가로 강제한다. 이전 file adapter들은 독립 단위 테스트 호환용으로 남는다.

## 저장 원칙

데이터베이스는 계정 소유권, 메모리 버전, gatekeeper 결정, usage receipt와 점수 원장을 일관되게 저장한다. 개인 데이터는 최소로 둔다. 전체 URL과 contribution 원문을 장기 보관하지 않는다. 검색 가능한 canonical URL pattern은 query·fragment·동적 개인 식별 경로를 제거한 안전한 값이어야 한다.

## 주요 테이블

| 테이블 | 주요 필드와 용도 |
|---|---|
| users | 내부 id, oauth_provider, provider_subject, leaderboard_opt_in, created_at, disabled_at |
| creator_profiles | user_id PK/FK, canonical_handle, bio_schema_version, bio_document, service_links, updated_at. 공개 콘텐츠는 opt-in projection으로만 제공 |
| devices | id, user_id FK, host_kind, token_hash, scopes, created_at, last_seen_at, revoked_at |
| contributions | id, contributor_user_id FK, canonical_url_pattern, task_category, trace_schema_version, sanitized_trace, consent_version, nonce, state, received_at, purge_at |
| gatekeeper_jobs | id, contribution_id FK, job_state, attempt_count, next_attempt_at, lease_owner, lease_generation, lease_expires_at, idempotency_key |
| gatekeeper_decisions | id, contribution_id FK, candidate_memory_id nullable, decision, confidence_band, reason_codes, policy_version, evidence_digest, decided_at |
| memory_records | id, canonical_url_pattern, task_category, intent_key, active_version_id, lifecycle_status, created_at, updated_at |
| memory_versions | id, memory_record_id FK, version_no, sanitized_content, fingerprint, evidence_digest, verified_at, created_at, supersedes_version_id |
| memory_version_authors | memory_version_id FK, user_id FK, source_contribution_id FK, attribution_role, attribution_status, created_at |
| usage_receipts | id, memory_version_id FK, consumer_user_id NOT NULL, author_user_id, issued_at, expires_at, outcome, outcome_at, client_nonce. Scoreable receipts are issued only to authenticated users. |
| memory_usage_aggregates | memory_version_id FK, bucket_start, bucket_granularity, authenticated_non_author_recall_matches, authenticated_non_author_detail_expands, distinct_non_author_accounts, anonymous_recall_matches, anonymous_detail_expands, reported_success/partial/failed, refreshed_at. Counts only; no reader identity or receipt ID. |
| account_usage_aggregates | user_id FK, bucket_start, operation, admitted_calls, denied_calls, admitted_cost_units, denied_cost_units, refreshed_at. Minute/day aggregates only; no URL, IP, body, device, request ID, or receipt ID. Retain 90 days under the proposed `usage-analytics.md` policy. |
| anonymous_usage_aggregates | coarse service time bucket, count. No memory/author link, receipt ID, caller/device identifier, or outcome claim; operational total only. |
| score_events | id, beneficiary_user_id FK, memory_record_id FK, memory_version_id FK, consumer_user_id nullable, source_receipt_id nullable, event_type, points_delta, idempotency_key, reversed_by nullable, created_at |
| audit_log | actor type/id, action, target_type/id, decision_digest, created_at. 토큰과 payload 원문은 저장하지 않음 |

user id는 공개되지 않는 내부 식별자다. OAuth 이메일은 로그인에 반드시 필요한 경우에만 provider에서 받고 공개 필드와 분리한다. raw OAuth token과 device secret은 DB에 저장하지 않고 hash 또는 token manager에 보관한다.

## 무결성 제약

users에는 provider + provider_subject unique constraint를 둔다. contribution의 contributor_user_id는 not null이며 계정 삭제 정책을 따르는 foreign key를 가진다. canonical_url_pattern은 정규화 결과만 허용한다. task_category와 state는 schema enum으로 검증한다. creator_profiles.canonical_handle은 lower-case canonical 값으로 저장하고 case-insensitive unique 제약을 둔다. bio_document와 service_links는 schema version을 함께 저장하며 validator를 통과한 값만 commit한다. profile은 owner만 편집할 수 있고 공개 조회는 leaderboard_opt_in이 true일 때만 허용한다. opt-out/disabled와 공개 projection 무효화는 같은 transaction 또는 outbox event로 보장한다.

같은 contribution nonce는 같은 사용자 범위에서 unique다. 같은 URL pattern + task category + intent key에는 현재 active memory record가 하나만 존재한다. 버전 번호는 record 안에서 단조 증가한다. 이전 memory_versions는 update-in-place하지 않는다. 교체는 새 immutable version을 추가하고 active_version_id만 원자적으로 이동한다. 새 version을 만든 contributor와 source contribution은 `memory_version_authors`에 기록한다. exact duplicate/support만 제공한 사용자를 새 version author로 가장하거나 등록하지 않는다. Usage receipt는 scoreable한 로그인 사용에만 발급하며 `consumer_user_id`가 필수다. 익명 요청은 caller identity/receipt를 저장하지 않고 usage aggregate count에만 반영한다. `memory_usage_aggregates`는 memory version과 bucket 조합당 하나이며 서버 집계 작업만 갱신할 수 있다. `account_usage_aggregates`는 account/bucket/operation별 Redis 절대 counter snapshot을 `GREATEST(existing, incoming)`로 upsert해 flush 재시도를 멱등 처리한다. 익명 IP limiter key는 별도 Redis 계층에서 HMAC+TTL로만 존재하며 PostgreSQL로 복사하지 않는다.

점수 이벤트는 발생 원인 receipt와 type별로 중복되지 않도록 unique idempotency key를 가진다. reversal은 원 이벤트를 삭제·수정하지 않고 역방향 delta 이벤트를 만든다. anonymous usage에는 consumer user id가 null일 수 있지만, 그 receipt는 score_events를 만들 수 없다. 작성자와 소비자가 같으면 score insert를 거부하는 DB transaction invariant를 둔다.

## 동시성 및 queue 처리

wam-server application coordinator는 기여·consent·`wam_work_items`·outbox를 하나의 transaction으로 commit하고, 최종 게시/점수 transaction에서도 계정·consent·lease generation을 다시 검사한다. 이는 아래 Gatekeeper standalone queue adapter와 별도 구현 경로다. Gatekeeper PostgreSQL queue adapter는 queue admission과 contributor nonce/backpressure 확인에 transaction-scoped advisory lock을 사용한다. Worker claim은 per-scope `pg_try_advisory_xact_lock`과 `FOR UPDATE SKIP LOCKED`를 함께 써 같은 scope의 중복 lease를 막고 서로 다른 scope의 claim을 병렬화한다. queue fairness tick은 짧은 singleton row update로 증가한다. 모델 작업은 lease transaction 밖에서 실행된다. lease timeout 후 새 worker가 generation을 올려 재시도할 수 있다. PostgresStore는 queue config가 설정된 DB에서 curator decision transaction 안에 queue job row를 잠그고 generation, owner, 신규 기여 판정에 사용한 contribution의 `attribution_withdrawn` 상태를 확인한다. withdrawal update가 먼저 commit되면 decision transaction은 이를 읽고 memory mutation과 decision journal insert를 거부한다. 이 상태는 현재 Gatekeeper queue row 기준이다. application coordinator는 계정 consent, contribution 상태, immutable 게시, author +20과 성공 사용 +2, reversal 및 withdrawal 재판정을 공통 application transaction에 연결한다. 모델/브라우저 검증은 이 transaction 밖에서 실행한다. 같은 URL/task key를 여러 worker가 동시에 판정하면 store의 active-scope unique constraint와 retry가 active version 중복을 막는다.

agent decision 저장, memory version 생성/교체, contribution 상태 변경, score award 또는 reversal은 단일 transaction 또는 outbox pattern으로 일관되게 완료한다. 외부 LLM과 replay 실행은 transaction을 잡고 기다리지 않는다. 먼저 immutable job input digest를 기록하고, 완료 뒤 정책 버전과 evidence digest를 확인한 후 DB 상태를 전환한다.

[`memory.md`](memory.md)와 [`queue.md`](queue.md)에 따라 다음을 확장한다. `memory_records`의 active 유일 제약은 `kind`를 포함하고, 검색용 `memory_index`(version_id, site_host, url_pattern, kind, task_category, keyword)는 승인된 immutable version의 L0에서만 만든다. 행 단위는 유일한 `(version_id, site_host, url_pattern, kind, task_category, keyword)` tuple이다. 같은 tuple은 중복 저장할 수 없으며 색인은 재생성 가능해야 한다. 조회용 복합 인덱스는 최소 site/route/task/kind scope와 site/keyword 검색을 지원해야 하고, lifecycle status는 active version join으로 필터링한다. 구체 인덱스 순서는 `EXPLAIN` 측정으로 확정하되 full candidate scan과 잘못된 active version 노출을 테스트한다. Gatekeeper 구현 테이블 `wam_gatekeeper_jobs`에는 job kind, priority, scope, contributor/device, nonce, contribution reference, state, attempts, retry 시각, lease owner/expiry/generation이 저장된다. queue config singleton은 모든 프로세스가 같은 정책을 사용하도록 비교한다. adapter는 worker lease, withdraw, resolve/fence, terminal purge, queue stats를 지원한다. server application의 실제 intake는 `wam_work_items`를 사용해 atomic enqueue와 consent/decision 연결, 게시 뒤 withdrawal 재판정을 처리한다. Gatekeeper standalone의 batch/fairness adapter와 동일한 queue라고 간주하지 않는다.

## 점수 원장

점수 합계는 score_events 원장에서 계산하거나 materialized summary로 제공하되, 원장과 합계가 재계산 시 일치해야 한다. 이벤트는 기여 승인, 타인 계정의 성공 사용, 롤백·정책 위반 회수로 구분한다. user ID, client body, dashboard 합계를 source of truth로 삼지 않는다. 모든 지급은 서버의 gatekeeper decision이나 사용 receipt에서 파생한다.

성공 사용 event의 beneficiary는 receipt 발급 시점의 `memory_version_authors`와 Gatekeeper 승인으로 정한다. 클라이언트가 작성자 ID를 전달하거나 변경해도 점수 귀속에는 영향을 주지 않는다. supersede된 과거 version의 receipt는 당시 version author에 귀속되고, 신규 version 사용 receipt는 신규 version author에 귀속된다.

익명 접근은 서비스 전체 운영용 coarse count와 memory별 dashboard aggregate로 집계할 수 있지만, per-call receipt나 개별 브라우저 식별값을 저장하지 않는다. memory별 집계는 익명 recall/detail count만 제공하고 고유 인원이나 outcome을 추론하지 않는다. 인증된 사용 통계도 dashboard aggregate에는 count만 기록하고 reader ID/receipt ID를 복제하지 않는다. 이 값은 작성자 점수나 leaderboard에 사용하지 않으며, 점수는 로그인 계정 기반 원장 이벤트만 소비한다.

## 보관과 인덱스

대기·거절 contribution의 sanitized trace는 문서화된 짧은 보관 기간 뒤 삭제한다. 게시된 memory version과 감사 decision은 PII가 없는 구조화 데이터로 유지한다. query parameter, cookie, 원본 URL을 log table에 복제하지 않는다.

조회 인덱스는 `memory_index`의 `(site_host, url_pattern, task_category, kind, version_id)` scope 검색과 `(site_host, keyword, version_id)` keyword 검색, active version join, memory fingerprint, contributor_user_id + received_at, score beneficiary_user_id + created_at, receipt consumer_user_id + issued_at, memory_version_authors(user_id, memory_version_id), memory_usage_aggregates(memory_version_id, bucket_granularity, bucket_start), account_usage_aggregates(user_id, bucket_start, operation)를 고려한다. author 분석 쿼리는 현재 로그인 사용자의 authored version으로만 제한한다. leaderboard 조회는 opt-in handle과 점수 합계만 읽고, 전체 개인 순위는 authenticated user_id에 대해서만 계산한다. public profile lookup은 canonical handle과 opt-in 상태를 함께 확인한다. 인덱스 순서는 [`search-performance.md`](search-performance.md)의 고정 프로파일과 `EXPLAIN`으로 확정한다.

## 삭제와 롤백

agent가 기존 정보 폐기를 결정해도 memory record를 hard delete하지 않는다. lifecycle_status를 superseded 또는 retired로 바꾸고 새 version/decision을 연결한다. 보안상 데이터 삭제가 필요한 경우에는 sanitized content의 파기와 감사 이력을 분리한다. rollback은 이전의 검증된 payload를 새 immutable version으로 추가해 active로 전환하고, 거절된 version의 점수 reversal을 별도 원장 이벤트로 기록한다.

## DB 테스트

DB-01~27은 계획된 application DB 제약 외에 Gatekeeper memory/queue adapter, nonce와 lease restart, worker fencing, stale decision commit 거부, withdrawal/decision row-lock race, resolve/purge/stats, usage aggregate idempotency/retention, production recall query pushdown/index use를 disposable PostgreSQL에서 검증한다. 운영 데이터베이스를 테스트 대상으로 사용하지 않는다. 성능 프로파일과 재현 환경은 [`../test/retrieval-benchmark.md`](../test/retrieval-benchmark.md)와 [`../test/service-abuse-performance.md`](../test/service-abuse-performance.md)에 둔다.
