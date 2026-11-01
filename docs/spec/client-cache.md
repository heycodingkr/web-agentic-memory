# CLI/MCP local cache and public response caching

Status: implemented subset. `wam-client` owns the bounded SQLite cache shared by `wam` CLI and local stdio MCP. Current server opt-in covers anonymous recall only; vocab and detail caching remain out of scope until they have explicit cache policy.

## Cache scope

Cache only successful anonymous recall responses that the server explicitly opts in to. Cache keys include API origin, schema/cache version, canonical request digest, and anonymous scope. Use an HMAC digest so the on-disk key does not reveal the canonical URL or keywords by simple inspection. Never cache authenticated responses, owner-bound receipt IDs, 401/403/429/5xx responses, contributions, profiles, dashboards, score events, usage reports, or any response marked `no-store`/`private` for an authenticated scope.

The server remains authoritative whenever online. A fresh local entry may satisfy a repeated anonymous CLI/MCP read; this is a local cache hit and does not count as a new server request. The client must report this distinction in status/output and must not imply that the server counted or scored an offline use. Linked/authenticated calls always contact the server so receipts, owner checks, analytics, and score rules remain intact.

Initial cache defaults: 5-minute freshness; at most 15 minutes of stale-if-network-error fallback; 50 MB and 2,000 entries, whichever limit is reached first. Never serve stale data after explicit 401/403/429, schema mismatch, endpoint change, or a server-provided revocation/kill-switch signal. Stale results are labeled with age and `offline/stale`; do not submit outcomes from them.

## Local storage

Use one small SQLite cache database owned by `wam-client`, under the platform cache directory (`$XDG_CACHE_HOME`, macOS `Library/Caches`, or Windows `LOCALAPPDATA`). SQLite gives atomic updates and a bounded LRU table across short CLI runs and a long-lived MCP process. A private HMAC key sidecar protects derived cache keys. The current implementation uses `rusqlite`; platform binary-size and cross-platform build measurements remain pending.

Store only an entry key, endpoint/schema version, status, safe cache metadata, body, ETag if supplied, expiry, last-access time, and byte size. Do not store auth headers, credentials, raw URL/query, IP, prompts, or usage receipt. Unix directory/database permissions are 0700/0600. Public memory content is not encrypted at rest, but the file must remain private to the OS user. Apply a short busy timeout, WAL where supported, atomic transactions, and corruption recovery that discards only this derived cache.

Provide `wam cache status` and `wam cache clear`; local stdio MCP `web_memory_status` reports cache enabled/disabled, entry count/size, and age range only. Remote MCP has no client-side cache status. Neither surface reveals cache keys, route values, or payload snippets. Cache clear never deletes credentials, contributions, or unrelated MCP data.

## HTTP contract and server cache

RFC 9111 requires explicit freshness and defines `no-store` as a prohibition on cache storage. A POST response is cacheable only when explicit freshness information is present and `Content-Location` equals the request target ([RFC 9111](https://www.rfc-editor.org/rfc/rfc9111.html), [RFC 9110](https://www.rfc-editor.org/rfc/rfc9110.html)). Therefore:

- Anonymous `POST /v1/memories/recall` opts the WAM local client in through `X-WAM-Client-Cache: anonymous-recall; max-age=300; stale-if-error=900`. HTTP responses still carry `Cache-Control: no-store`, so browsers and shared proxies do not store them. The app-specific local cache checks the custom opt-in and never stores authenticated results.
- Server error responses, 429 responses, and responses without explicit cache permission are not stored. Cache headers from a cached response are stripped of stale quota/reset metadata; return a local `X-WAM-Cache: hit|stale` marker instead.
- Public GET resources may use ETag/conditional revalidation and short explicit TTLs only after an authoritative generation or equivalent invalidation contract is tested. Personalized routes always remain `no-store`.
- The web API client sets Fetch `cache: 'no-store'` on every API request. Server responses also use `Cache-Control: no-store` for dynamic API success and error paths.
- `GET /v1/memories/vocab` changes when the published catalog changes and has no HTTP validator, so it is `no-store`. `GET /v1/memories/{id}` records a detail view and is `no-store`. Health, catalog, owner views, and API errors are also not stored.
- The public catalog is safe to expose but is not yet safe to cache: its PostgreSQL read currently reconstructs a full application snapshot, and the HTTP response is not tied to the catalog generation. First add a narrow catalog projection query, then prove ETag invalidation after publish, supersede, retire, account deletion, and rollback across independent connections.
- Redis may cache hot shared public aggregates or vocabulary only after query plans are healthy. PostgreSQL remains authoritative; updates increment a generation and invalidate/expire entries. Do not cache around incorrect SQL or use shared cache for personalized ranking/dashboard responses.

## Requirements

| ID | Requirement | Tests |
|---|---|---|
| F-CACHE-01 | CLI and MCP use the same cache contract and store | CACHE-01, CACHE-09 |
| F-CACHE-02 | Only anonymous explicitly cacheable responses are stored | CACHE-02~04, SEC-J04 |
| F-CACHE-03 | Cache is bounded, cross-process safe, and corruption tolerant | CACHE-05~07 |
| F-CACHE-04 | Keys and diagnostics do not expose user navigation data | CACHE-08, SEC-A22, SEC-H04 |
| F-CACHE-05 | Stale fallback is visible and cannot issue score/outcome | CACHE-10, FAIL-34, FLOW-10 |
| F-CACHE-06 | Cache can be inspected and cleared without touching credentials | CACHE-11 |

## Current implementation and remaining checks

`wam cache status`, `wam cache clear`, `wam usage`, and local stdio MCP cache status are implemented. The web Fetch client bypasses the browser HTTP cache. Fresh local hits are anonymous-only; stale fallback occurs only after transport or response-body failures and includes a visible age/status marker. Authenticated requests, receipts, 4xx/429/5xx responses, and successful transport responses with invalid payloads do not use stale fallback. Unit tests cover HMAC key handling, persistence, origin/schema isolation, bounds, expiry, clearing, auth bypass, and retry behavior. Multi-process stress and native Windows/macOS/Linux release builds still need environment verification. Public vocab/detail caching and HTTP intermediary caching are not implemented.
