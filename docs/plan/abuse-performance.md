# Abuse, cache, and analytics implementation plan

Updated: 2026-10-27. This tracker records implementation and verification state for [`../spec/rate-limits.md`](../spec/rate-limits.md), [`../spec/usage-analytics.md`](../spec/usage-analytics.md), [`../spec/client-cache.md`](../spec/client-cache.md), and [`../spec/search-performance.md`](../spec/search-performance.md). Code-level tests are not evidence of live Redis/PostgreSQL capacity.

## Priority order

| Priority | Work | State | Remaining evidence/work |
|---|---|---|---|
| P0 | Rate-limit middleware | Implemented: weighted IP/account/shared buckets, Redis atomic script, trusted-proxy resolution, IPv6 /64 grouping, 429/retry headers, fail-closed behavior, authenticated denied counts, per-process in-flight 503 shedding. Disposable local Redis passed shared-quota concurrency, denial accounting, atomic multi-bucket, and account-deletion fence tests across independent connections | Live Redis outage/failover, deployed proxy chain, and production concurrency tuning |
| P0 | Account and memory analytics | Implemented: `/v1/me/usage`, 30-day account activity, authenticated denied-attempt counts, per-version author aggregates, 90-day retention, owner APIs; author analytics and aggregate persistence passed PostgreSQL integration tests; Redis usage/denial aggregation passed a disposable-service test | Redis retention expiry and production retention TTL still need live verification |
| P1 | CLI/MCP local cache | Implemented: shared `wam-client` SQLite cache, anonymous recall opt-in, bounds, HMAC key, stale transport fallback, clear/status commands | Native release builds and multi-process stress across supported OSes |
| P1 | Search/DB profile | Implemented: indexed recall candidate filtering, keyword lookup, direct detail lookup, core-ranking parity integration test; PostgreSQL migrations, application queries, queue/store adapters, and transaction/race tests passed against a disposable local database | `EXPLAIN (ANALYZE, BUFFERS)` on a representative corpus and 1k/10k/100k plus separate 1M benchmark remain; keyword candidate cap needs rank-preserving SQL or a measured quality gate because BM25/RRF depends on the full candidate set |
| P1 | Resilience | Partially implemented: fail-closed Redis, transaction serialization, cache isolation, idempotent write paths | Run broader Redis/DB fault injection and classify retry behavior across live adapters |
| P2 | Usage and leaderboard visual system | Implemented: English-first quota, activity and memory-impact charts; table alternative; no fabricated rank movement | Responsive browser QA and persisted rank history if movement is later requested |
| P2 | Serialization benchmark | Not started; Serde JSON remains canonical | Benchmark representative release payloads before considering local-only formats |

## Technical decisions to verify before code

1. Run Redis in the intended deployment topology and test atomic multi-key admission, failover, latency, and outage policy. Keep an in-memory adapter for deterministic unit/router tests only.
2. Pin and test the actual trusted reverse-proxy chain. Reject spoofed forwarding headers and make direct local loopback behavior explicit.
3. Implement policy in small seams: pure quota/key rules, a storage port, the Redis adapter, and Axum middleware/response mapping. Do not spread quota arithmetic across route handlers or web components.
4. Keep usage counts separate from score and per-memory author impact. Account request aggregates contain no URL/query/IP and are erased on account deletion or expire from Redis; per-memory aggregates contain no reader identity and expire after 90 UTC dates.
5. Benchmark SQLite cache size/build cost on macOS/Linux/Windows, multiple CLI processes, one MCP process, crash/corruption, permission, and eviction scenarios before making it default-on.
6. Replace full-table `AppState` loading on recall with a bounded repository candidate query before profiling scale. Inspect that SQL with `EXPLAIN (ANALYZE, BUFFERS)` and `pg_stat_statements`; add PostgreSQL indexes, Redis cache, vector search, or binary serialization only where the fixed workload demonstrates a bottleneck.
7. Use LayerChart only after Svelte 5 compatibility, accessibility, bundle size, reduced-motion, and license checks. Keep all user-facing product copy English by default and use real server values only.

## Evidence gates

- Specs and IDs first: `RL-*`, `USAGE-*`, `CACHE-*`, `SEC-J*`, `FAIL-33~38`, `PERF-21~24`, `FLOW-09~10`.
- Focused Rust and web tests pass in the local workspace. PostgreSQL integration tests passed in an isolated temporary local database (16 tests); three Redis adapter integration tests passed against a disposable local Redis on port 16379. Redis outage/failover, deployed proxy behavior, and production topology remain unverified.
- Then Playwright user flows for quota warning/429 recovery, usage dashboard, public ranking, local CLI/MCP cache parity, offline cache labeling, and cache clearing.
- Then nightly load at the existing 100k corpus and separate 1M stress, with five repeated runs and p50/p95/p99, errors, throughput, RSS, query plan, Redis availability, and cache-hit split.
- No release claim until output is attached to the environment record and the current shared server implementation has passed the relevant tests.
