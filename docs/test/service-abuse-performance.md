# Rate-limit, usage, cache, and resilience test specification

Status: test specification with a locally tested subset. Rust unit/router tests cover weighted buckets, IPv6 prefix grouping, API 429 behavior, owner usage, local cache bounds/privacy, and local author-analytics aggregates. PostgreSQL application/analytics/queue/store tests passed in a disposable local DB. Redis 8.10.3 tests passed for shared admission across two store connections, concurrent quota boundaries, denial accounting, atomic multi-bucket behavior, and deletion fencing. Redis outage/failover and scale cases remain pending. Requirements are defined in [`../spec/rate-limits.md`](../spec/rate-limits.md), [`../spec/usage-analytics.md`](../spec/usage-analytics.md), [`../spec/client-cache.md`](../spec/client-cache.md), and [`../spec/search-performance.md`](../spec/search-performance.md). Use synthetic accounts, synthetic IPs, fixed clocks, disposable Redis/PostgreSQL, and loopback HTTP only.

## 1. Rate-limit contract tests

| ID | Setup / action | Pass oracle |
|---|---|---|
| RL-01 | Anonymous client sends `limit + 1` recall operations from one IPv4 address using an injected small policy | First `limit` admissions succeed; next returns 429; denied request invokes neither recall repository nor usage receipt writer |
| RL-02 | Authenticated account sends requests past guest allowance, then past account allowance; a second account shares the same IP | Account allowance is higher as configured, but shared-IP ceiling still blocks both accounts; account A cannot consume or inspect B's account bucket |
| RL-03 | Send a concurrent boundary burst through two independent server instances backed by the same limiter store | Accepted count never exceeds the atomic configured quota; no per-process split-brain window |
| RL-04 | Direct peer sends forged `Forwarded`, `X-Forwarded-For`, and `X-Real-IP`; trusted proxy sends a multi-hop chain | Direct caller is charged to socket peer; only configured proxy hops are trusted; malformed chain is rejected or falls back safely |
| RL-05 | Exercise IPv4, IPv6 addresses within/outside /64, account IDs, and rotating HMAC secret | IPv4 exact address and IPv6 /64 grouping match policy; persisted key has no raw IP; expired/old-key buckets are removed without affecting account identity |
| RL-06 | Exhaust each endpoint class and inspect response plus browser CORS fetch | HTTP 429, current `rate_limited` code, `Retry-After`, `RateLimit-Limit/Remaining/Reset`, exposed headers; no IP, key, account, or route data in response |
| RL-07 | Web API returns low remaining quota, then 429 with `Retry-After` | Warning and wait state use server values; UI does not auto-loop/retry early; user can retry after the wait |
| RL-08 | Race `N + k` admissions across tasks sharing one key and different keys | Exactly at most N admissions per key; independent keys proceed; denied calls do not execute expensive handler work |
| RL-09 | Stop Redis or inject command timeout during a costly read, contribution, challenge, and public cache hit | No local-counter bypass; affected uncached/expensive/write paths return retryable 503; safe cache path is clearly marked and bounded |
| RL-10 | Repeated device-code attempts, OAuth poll, contribution retry with the same idempotency key, and distinct contribution submissions | Challenge cap applies; identical write retry creates one logical contribution; fresh over-limit writes are denied before queue/storage work |
| RL-11 | Send valid, malformed, 401, 404, and 5xx requests around quota boundary | Policy counts match the documented admission point; errors do not leak a different caller's remaining quota; 429 attempts are separate from admitted usage |
| RL-12 | Fill the configured per-process in-flight cap while polling `/healthz` | Saturated API/MCP work gets `503 service_busy` and `Retry-After`; health checks still answer; rejected work does not enter route handlers |
| RL-12 | Drive synthetic distributed traffic from many IPs/accounts until configured DB/Redis pool or latency guard trips, then recover | Global load shed returns a distinct 503 without granting a per-client bypass or reporting `RATE_LIMITED`; health/readiness remains available; safe public cache behavior is bounded; guard clears after load recovers |

| RL-13 | One anonymous client initializes MCP, lists tools, checks status, recalls and expands the returned version without sleeping | All succeed within the initial read budgets; discovery costs 1, recall 3, expand 2, and status has its independent low-cost bucket. Authenticated MCP recall/detail are counted in the same owner usage classes as HTTP |
| RL-14 | Exhaust the action bucket using MCP writes, unknown tools, malformed envelopes and oversized bodies; then request status | Write/unknown/invalid requests retain cost 5 and reach 429 with Retry-After; status remains available in its separate bounded bucket. The 64 KiB body limit, original transport/auth errors and no-store/CORS boundaries remain enforced |

## 2. Personal usage and web tests

| ID | Setup / action | Pass oracle |
|---|---|---|
| USAGE-01 | Account A performs recall, expand, contribution, plus one denied call; query `/v1/me/usage` | Admitted calls, weighted cost units, denied request count, remaining bucket capacity/refill wait, and 30 UTC dates match independent server counters; denied attempts are separate from admitted calls |
| USAGE-06 | Exhaust account action quota and fetch `/v1/me/usage` from that account | Status/usage endpoint remains available within its separate low-cost account/IP bucket and reports action quota state without exposing shared-IP counters |
| USAGE-02 | Flush the same aggregate batch twice and across worker restart | PostgreSQL bucket totals are unchanged after replay; freshness target and late-bucket behavior are recorded |
| USAGE-03 | Account A requests `/v1/me/memories/analytics`; account B requests the same endpoint | Each account sees only its authored versions; there is no caller-selectable foreign memory ID; response has no IP, URL, request body, token, reader, or receipt ID |
| USAGE-04 | Create anonymous recall/detail traffic and query public and author analytics | Anonymous service totals remain coarse publicly; author impact may count anonymous hits by authored version but has no stable visitor, IP, reader identity, rank, or per-IP history |
| USAGE-05 | Advance fixed clock past 90-day retention and delete an account | Expired/account-linked rows are purged; unlinked coarse service totals follow the privacy policy; no score, memory author, or public profile is changed by usage purge |
| WEB-26 | Load dashboard at near-limit and fresh quota states, then at mobile and keyboard-only viewport | English labels explain operation unit/refill; account meter never reveals shared-IP remaining count; progress is not color-only; full-refill time and loading/empty/error states are accessible |
| WEB-27 | Trigger 429 and limiter 503 in dashboard and public routes | 429 displays the advertised wait time; 503 is an availability state, never a quota/zero-data state; no automatic retry storm |
| WEB-28 | Render 30-day account activity and daily/weekly authored-memory impact from a fixed API fixture | Series match API totals, label UTC range and counts, separate account calls from author impact, and provide an accessible table alternative |
| WEB-29 | Open leaderboard with opt-in/out accounts and no historical rank snapshot | Current private/public rank and points match API; opt-out profile is absent; no fabricated rank delta or empty-as-zero chart |

## 3. Local cache tests (CLI and MCP parity)

| ID | Setup / action | Pass oracle |
|---|---|---|
| CACHE-01 | Use a temporary platform cache directory with CLI and MCP adapters pointing at the same `wam-client` cache | Same cache entry is reusable by both; result schema and hit status match |
| CACHE-02 | Send anonymous recall with `X-WAM-Client-Cache` opt-in while HTTP response remains `Cache-Control: no-store` | The WAM app client caches a successful response under the canonical request HMAC; browsers/proxies do not cache it; raw route, keyword and body are absent from SQLite rows/diagnostics |
| CACHE-03 | Send authenticated recall with receipt, `no-store`, or mismatched scope/endpoint | No private response, receipt, credential, or cross-origin entry is written or read |
| CACHE-04 | Return 401, 403, 429, 5xx, timeout, malformed JSON, and an oversized body | Errors are not cached; last valid entry remains intact; malformed/oversized cache data is rejected within limits |
| CACHE-05 | Fill beyond entry and byte cap from two client processes | LRU/size eviction remains under both caps; writes are transactionally readable; no torn record |
| CACHE-06 | Kill writer during commit; corrupt SQLite DB/WAL; reopen cache | Recovery discards/rebuilds cache only; server data, credentials, settings, and unrelated files remain untouched |
| CACHE-07 | Set fake clock at fresh, expired, and stale-if-network-error boundary | Fresh cache can satisfy eligible anonymous request; stale fallback only occurs on network failure, within 15 minutes, with visible age; online revalidation updates it |
| CACHE-08 | Change API origin, schema version, canonical URL, task, and keyword order | Origin/schema/scope changes miss; equivalent canonical requests with keyword order normalized hit; raw values are not persisted |
| CACHE-09 | Run the same query over remote MCP, stdio MCP, and CLI with cold/warm/offline network | Online server result IDs/rank are equal; eligible anonymous cache result is clearly marked and never creates a receipt/outcome/score |
| CACHE-10 | Revoke/retire a memory and request online, then simulate transport failure | Online response replaces cache; stale fallback is limited to the documented transport-failure window. A server kill-switch/generation signal is future work and is not claimed by the current protocol |
| CACHE-11 | Run `wam cache status`, `wam cache clear`, and MCP status | Only size/count/age state is shown; clear removes cache entries only and does not remove credentials or contribution queue |

## 4. Search, cache, serialization, and fault benchmarks

| ID | Workload | Pass oracle |
|---|---|---|
| PERF-21 | Run fixed search queries at 1k/10k/100k versions and existing separate 1M stress profile; capture `EXPLAIN (ANALYZE, BUFFERS)` | 100k DB-only p95 meets the existing 100 ms dedicated-runner target; 10k-to-100k p95 growth stays below 2x for indexed exact/keyword classes or has a documented plan-backed exception; quality, leakage, rows visited, p99, and index size are reported |
| PERF-22 | Benchmark representative JSON encoding/decoding and candidate cache formats in release mode | Report p50/p95, allocations, bytes, RSS, and exact round-trip/error behavior; keep JSON unless a repeatable WAM workload improvement exceeds measurement noise and maintains validation/version gates |
| PERF-23 | Run 100 RPS with 1/10/50/100 concurrency across anonymous/auth/account-shared IP, including quota boundaries | Correct 429/admission decisions; rate limiter adds ≤15 ms p95 in same-region test topology; no limiter outage silently admits writes; report 429/503 and throughput |
| PERF-24 | Compare cold/warm local cache, server cache miss/hit, ETag validation, publish, supersede, rollback | Cache hit reduces measured network/DB work; state changes invalidate within the specified generation window; stale inactive result 0; cache and DB latency reported separately |
| PERF-25 | Inject transient DB serialization/deadlock, DB timeout, Redis timeout, cache lock/corruption, and provider timeout under load | Bounded retry respects policy; final error is classified; no duplicate write/score/queue event; request tails/error budget and recovery duration are reported |

Benchmark seed, schema version, row distribution, host skew, query labels, and runner version are recorded with each run. Use at least five repetitions for performance claims and keep the 1M stress result separate from the 100k release gate. See [`retrieval-benchmark.md`](retrieval-benchmark.md), [`database.md`](database.md), and [`metrics.md`](metrics.md) for existing corpus and shared thresholds.

## 5. Mapping to implementation suites

| Test family | Planned location | Test layer |
|---|---|---|
| RL pure policy/key extraction | `crates/wam-server/src/rate_limit.rs` unit tests | L0 |
| Redis atomic quota, concurrent requests, denial accounting, and deletion fence | `crates/wam-server/tests/rate_limit_redis.rs` and `crates/wam-server/tests/account_cleanup.rs` | disposable Redis; 3 passed locally |
| Limiter store outage response, retry hint, and health endpoint | `crates/wam-server/tests/rate_limit_failure.rs` | injected storage failure; fail-closed 503 and `/healthz` verified |
| Redis outage/failover and two deployment replicas | pending service fault-injection/CI test | external topology required |
| API response headers/CORS and owner usage/author analytics | `crates/wam-server/tests/api.rs` | L1/L2 |
| CLI/MCP shared persistent cache | `crates/wam-client/src/cache.rs` and client adapter tests | L0/L1 |
| Dashboard quota/activity and leaderboard | `web/src/**/*.test.ts` and Playwright | L3 |
| Cross-layer quota, stale cache, recovery | `tests/user-flows/journeys/` | L3 |

The target paths are proposals; keep them small and aligned with the existing crate/module boundaries when implementation starts.
