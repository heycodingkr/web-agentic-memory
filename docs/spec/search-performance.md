# Search, database, serialization, and fault tolerance

Status: design contract plus unbenchmarked implementation. Current retrieval uses deterministic URL/task/kind candidate filtering, fielded BM25 and RRF ranking. PostgreSQL and `memory_index` are the database path; no WAM benchmark currently justifies adding a vector database or replacing the public JSON API.

## Scale and query contract

Keep the repository query as a narrow candidate selector for the shared `wam-core` ranking policy. Index immutable active memory projections by canonical host/route/task/kind and host/keyword, then join the active-version pointer before returning candidates. Bound candidate count, response size, keyword count, and pagination. Use keyset pagination for growing public lists. Keep aggregate dashboards on precomputed buckets rather than running unbounded `COUNT(DISTINCT)` against raw request history.

The existing benchmark profiles already define 1k, 10k, 100k versions and a 1M stress profile. At 100k versions (about 1.25M index rows), retain the existing DB-only p95 goal of 100 ms on the dedicated runner. Report 1M separately; no constant-latency claim is made. Compare the same query mix at 10k and 100k, inspect `EXPLAIN (ANALYZE, BUFFERS)`, and track rows visited/returned, index plan, buffer hit rate, and p50/p95/p99. PostgreSQL documents `EXPLAIN` as the way to inspect actual plan behavior; `pg_stat_statements` supplies aggregate planning/execution statistics ([EXPLAIN](https://www.postgresql.org/docs/current/using-explain.html), [pg_stat_statements](https://www.postgresql.org/docs/current/pgstatstatements.html)). Indexes must be justified by measured query plans because they add write/storage overhead ([PostgreSQL indexes](https://www.postgresql.org/docs/current/indexes.html)).

Use `pg_stat_statements` in production after checking its preload/restart and retention implications. Slow-query logs and traces contain operation names and query fingerprints only, never URL patterns, keywords, tokens, user input, or request bodies. Test realistic hot-host skew and no-match queries, not only uniform synthetic rows.

## Current working-tree query finding

The PostgreSQL recall path now queries `wam_memory_index` by host, canonical route, task, and kind, joins only active versions, then runs the shared `wam-core` ranking policy over those candidates. Vocabulary reads use the index, and memory detail uses an ID lookup. This removes the prior whole-catalog snapshot from recall and expand reads. The SQL route selector still has no explicit candidate cap; hot hosts with many matching records may return a large candidate set. Applying an arbitrary SQL `LIMIT` before ranking would change BM25 corpus statistics and reciprocal-rank fusion order, so a cap needs a rank-aware implementation or a measured approximate-search quality gate. The generic transaction path for profiles, contributions, identity, and score state still loads an `AppState` snapshot. PostgreSQL remains serialized through one connection thread and repository mutex.

The application migration now adds daily per-memory usage aggregates with a `(version_id, utc_date)` primary key and a retention index. Recall/detail writes update only the matching aggregate row; analytics reads are owner-version and date-range filtered. `wam-core` remains the ranking authority. The parity test compares indexed results against the exhaustive core oracle.

PostgreSQL integration tests compile but are ignored unless a disposable `WAM_TEST_DATABASE_URL` is configured. This environment has no such database, so migrations, query plans, concurrent writes, and p95 behavior have not been live-verified. Before a scale claim, run the indexed route test plus `EXPLAIN (ANALYZE, BUFFERS)` and the fixed 1k/10k/100k profiles. In particular, decide whether measured hot-host candidate sets need a safe bound or an additional rank-aware index. Keep auth/profile/score semantics isolated while their existing state transitions remain snapshot-based.

Add a new index only when a query plan and benchmark show a real scan/bottleneck. Partition time-bucket usage aggregates only after measured size or retention/purge costs justify it. Keep PostgreSQL source of truth and do not add Qdrant, graph DB, Tantivy, or an in-process cache as an unmeasured first response. If fielded keyword retrieval becomes the bottleneck, benchmark PostgreSQL text indexes against Tantivy and hybrid/RRF alternatives using the held-out query set before migration.

## Serialization

Keep Serde JSON as the public HTTP/MCP contract and current cache envelope until profiles show serialization is a meaningful portion of request CPU or payload cost. Benchmark representative recall, catalog, dashboard, and analytics payloads on release builds. Compare time, allocations, encoded bytes, peak RSS, and compatibility for JSON against candidates such as Postcard or rkyv; test actual schema attributes and versioning before adopting one. Serde JSON upstream reports high throughput on its own benchmarks, while Postcard/rkyv trade compatibility and validation constraints for compact or zero-copy formats ([serde_json](https://github.com/serde-rs/json), [Postcard](https://github.com/jamesmunns/postcard), [rkyv](https://github.com/rkyv/rkyv), [Rust serialization benchmark](https://github.com/djkoloski/rust_serialization_benchmark)). Those headline rates are not WAM measurements.

Do not serialize public API contracts in a binary-only format. If a binary cache representation wins the WAM benchmark, version it behind the local cache and retain JSON network compatibility. Keep size/time limits and schema validation before accepting any serialized cache entry.

## Fault tolerance

- Apply explicit connect, request, database statement, and cache-operation timeouts. Bound concurrency and body sizes.
- Retry only transient, idempotent reads a small fixed number of times with exponential backoff and jitter. Never blind-retry writes; use existing idempotency keys and verify the prior result after an ambiguous commit.
- A transaction retry restarts the entire transaction only for classified serialization/deadlock conflicts. Do not retry validation, auth, not-found, or quota errors.
- A Redis limiter outage does not become a local per-process bypass. Return controlled 503 on routes that require admission. A public cache miss falls through to PostgreSQL only when the DB is available.
- Cache failures are isolated: discard/rebuild only derived cache state, keep authoritative DB state. Publish/rollback invalidation is generation-based and tested for stale results.
- Model/provider retry and queue lease rules remain in `model.md`/`queue.md`; no external model call runs while holding a DB transaction.

## Requirements

| ID | Requirement | Tests |
|---|---|---|
| F-PERF-01 | Search candidate SQL uses a measured index plan at scale | DB-21, DB-27, PERF-15, PERF-21 |
| F-PERF-02 | Same query results and quality hold across cache, DB, HTTP, MCP, and CLI | RCL-18~24, PERF-17~20 |
| F-PERF-03 | Dashboard counts use bounded aggregates and owner-scoped queries | DB-26, USAGE-03 |
| F-PERF-04 | JSON remains canonical unless representative benchmarks justify a versioned local alternative | PERF-22 |
| F-PERF-05 | Retry, timeout, and outage behavior preserves idempotency and authoritative state | FAIL-33~38 |

## References from adjacent projects

- [`tantivy`](https://github.com/quickwit-oss/tantivy): a Rust full-text search engine; consider only if measured PostgreSQL text search is insufficient.
- [`qdrant/skills`](https://github.com/qdrant/skills): hybrid retrieval and reciprocal-rank fusion evaluation patterns; reuse the evaluation method before adopting a vector store.
- [`getzep/graphiti`](https://github.com/getzep/graphiti) and [`mem0`](https://github.com/mem0ai/mem0): temporal/graph and entity-linking retrieval patterns for future multi-hop memory needs, not a current schema requirement.
- [`LongMemEval`](https://github.com/xiaowu0162/LongMemEval) and [`MemEval`](https://github.com/ProsusAI/MemEval): evaluation dimensions to adapt into WAM's synthetic, held-out site/task query set.
