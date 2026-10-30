# Usage and memory analytics

Status: implemented working-tree contract for account request usage and version-level author impact. PostgreSQL application/analytics paths passed local disposable-database integration tests; production deployment behavior and Redis-backed account buckets have not been live-verified. Charts use server aggregates and do not claim historical rank movement.

## User-visible measures

The dashboard separates three questions so users can interpret each number:

1. **Request allowance:** available cost units, burst capacity, refill rate, and full-refill time. A denied 429 is shown separately and is not hidden as a successful call.
2. **My activity:** daily/weekly counts and cost units of admitted calls by operation (`recall`, `expand`, `catalog`, `contribution`, `outcome`), over the most recent 30 days. This is account usage, not a bill or a measure of memory quality.
3. **My memories' impact:** existing version-level author aggregates (recall matches, detail expands, other signed-in accounts, anonymous request counts, and valid outcomes). Anonymous counts are not unique people, and usage is not score.

The current private and public ranking cards remain separate. A rank history or rank movement indicator is shown only after dated snapshots exist; never infer or fabricate past rank from current score alone.

## Data and API

`GET /v1/me/usage` requires `read:dashboard`. It returns the account bucket's current limit, burst capacity, remaining units, refill wait, and a 30-day UTC series of admitted requests/units split into recall, detail, and other calls, plus account-attributed `denied_requests`. A denied request does not increment admitted requests or units. Only denials associated with an authenticated account are counted; anonymous and invalid-credential denials have no account history. The Redis usage keys expire after 90 days. The shared IP bucket state is never returned. The endpoint is `Cache-Control: no-store` and includes no IP, raw URL, request body, device token, email, other-user identity, or per-request event ID.

`GET /v1/me/memories/analytics?from=&to=&bucket=day|week` requires `read:dashboard`. It defaults to the latest 30 UTC dates and accepts up to 90 inclusive days. It returns only versions authored by the caller and daily aggregates grouped by day or Monday-starting UTC week. Metrics are recall result hits, detail views, and explicit outcomes for versions selected in a receipt. Anonymous hits are requests, not unique people. Other-account counts are per hit, not distinct accounts; the service stores no reader IDs. Self-reads are excluded. The endpoint is owner-scoped, `no-store`, and includes no reader identity, URL, receipt, or request event.

Account activity currently lives in short-lived, HMAC-keyed Redis day buckets; per-memory impact lives in a local aggregate state or PostgreSQL `wam_memory_usage_aggregates` rows keyed by `(version_id, utc_date)`. Both sources store aggregate counts, not per-request reader identities. IP enforcement state stays in short-lived limiter buckets.

Memory aggregates retain 90 dates and are pruned as writes arrive. Account usage buckets expire after 90 days in Redis. Production retention and Redis account erasure need service-backed verification; PostgreSQL aggregate transactions have local integration coverage but still need production workload measurement.

`GET /v1/metrics` remains a coarse public aggregate endpoint. It must not gain account, IP, memory/version, URL, receipt, or device identifiers. Suppress small cohorts where a breakdown could identify one person. A missing bucket is zero only when the API confirms the query completed; network/error states are never rendered as zero.

## Visualization and interaction

The dashboard shows account quota/activity above an authored-memory impact section. The impact chart supports daily or weekly grouping, uses server values, and includes a keyboard-accessible data table alternative. It labels the UTC range and separates recall hits, detail views, and successful uses. The current account activity chart covers 30 days; 7/90-day selection and per-operation quota meters remain future work. No rank-movement chart appears because persisted rank snapshots do not exist.

The leaderboard may add a restrained score/participation visualization, but rank movement is gated on persisted snapshots. Keep handle, rank, and points legible at mobile widths; public inclusion remains opt-in. Use specific, sourced measures only and no invented growth percentages. This follows the useful constraints in [Nutlope/Hallmark](https://github.com/Nutlope/hallmark). For Svelte, evaluate [LayerChart](https://github.com/techniq/layerchart), a composable Svelte chart library; confirm the installed Svelte version, bundle impact, accessibility, reduced-motion support, and license before adding it. Avoid adding React/Recharts to the Svelte app.

## Requirements

| ID | Requirement | Tests |
|---|---|---|
| F-USAGE-01 | Account sees current quota capacity/refill and admitted calls/cost units, including after action quota is exhausted | API-45, USAGE-01/06, WEB-26 |
| F-USAGE-02 | Account sees bounded activity trend and operation breakdown | USAGE-02, WEB-28 |
| F-USAGE-03 | Authored memory impact is separate from account calls and score | API-40, USAGE-03, WEB-25 |
| F-USAGE-04 | Anonymous aggregates remain coarse and non-identifying | USAGE-04, SEC-J03 |
| F-USAGE-05 | Quota errors and unavailable analytics are not shown as zero | USAGE-05, WEB-27, FAIL-35 |
| F-USAGE-06 | Rank movement is only shown from persisted historical snapshots | WEB-29 |

## Current gap

`/v1/me/usage`, quota error parsing, authenticated denied-attempt counts, the account activity chart, the owner-only memory analytics endpoint, PostgreSQL daily aggregate migration, and the author impact chart are implemented in the working tree. Tests cover API 429 accounting, owner isolation, anonymous versus other-account counts, explicit outcome selection, local persistence, and date validation. PostgreSQL aggregate and application integration tests passed as part of the 16-test disposable-database run. Redis account-bucket, erasure, failover, and deployed retention tests remain pending because no Redis service or `WAM_TEST_REDIS_URL` is available. Remaining product gaps are selectable 7/90-day account activity ranges and persisted rank snapshots if rank history is requested; production measurements also remain. The web chart uses native Svelte/CSS with a table alternative; no chart dependency was added.
