# wam-server

Axum HTTP/MCP server with a shared transactional application repository.

## Storage and execution

Development defaults to `.wam-data/application.json`. Atomic replacement and an exclusive process lock keep profile, identity, contribution, catalog, receipt, score, lease and outbox changes together. Only one process may open this local file. The previous five JSON stores are imported once; originals are removed only after the complete new snapshot is persisted. Old local-only contributions become `consent_required` and cannot be published without a new submission.

Set `WAM_DATABASE_URL` to use PostgreSQL. The server applies the checksummed SQL migrations in `migrations/`. Profiles, contributions/consent, immutable versions/indexes/authors, receipts, score events, identity records, work, audit and outbox use relational tables with keys and constraints. PostgreSQL supports multiple server/worker processes. Compound writes use a short schema-scoped transaction lock; model calls and browser replay happen outside transactions. Compound state transitions still load transactional snapshots. Local 1k/10k/100k retrieval profiles now measure indexed candidates and real loopback HTTP; dedicated-runner read/write throughput and 1M stress remain unverified. Non-loopback connections require verified TLS. Connection strings and driver error details are not logged.

`WAM_ENV=production` requires PostgreSQL, either Resend email login or GitHub OAuth configuration, HTTPS origins (and HTTPS callbacks when GitHub is enabled), and no demo token. Shared admission also requires `WAM_RATE_LIMIT_REDIS_URL` and a private `WAM_RATE_LIMIT_KEY_SECRET` of at least 32 bytes; remote Redis uses `rediss://`. Configure `WAM_TRUSTED_PROXY_CIDRS` only for actual ingress proxies. `WAM_MAX_INFLIGHT_REQUESTS` sets the per-process API/MCP concurrency cap (default 256); `/healthz` is exempt. PostgreSQL starts empty; development file storage includes the demo catalog/accounts and uses an in-memory quota store.

Run local development:

```sh
WAM_DEMO_TOKEN=local-dev-token WAM_REVIEW_TOKEN=local-review-token cargo run -p wam-server
```

The default bind is `127.0.0.1:3000`. Set `WAM_BIND` and `WAM_DATA_DIR` to override it.

## Built-in example memories

With the local file store and outside `WAM_ENV=production`, the server adds the memories in `seed/example_memories.json` (templated Google Docs documents, Google Sheets work logs and expense trackers, Gmail, Calendar, Slides, Forms, Drive, Notion, Medium, Wikipedia) at start-up, so `/memories` and recall have something to return. They are additive and idempotent: a restart adds only the entries that are missing and never touches an existing row. `WAM_EXAMPLES=off` skips them, `WAM_EXAMPLES=on` also loads them into PostgreSQL.

They are examples, not verified site knowledge: every title starts with `Example:`, each entry has no author (recalling it never earns points), and the written date is the 2026-10-30 authoring date. Every entry passes the same validator as a published memory (closed codes, text screening, length limits), checked by `cargo test -p wam-server --test examples`. Scope is by path, so a `docs.google.com/document` entry is not recalled for a `docs.google.com/spreadsheets` URL. Keep each data file hand-edited JSON; a second file can be added next to it (see `seed/productivity_memories.json`) and listed in `examples::load`.

SIGINT and, on Unix, SIGTERM stop admission, close idle MCP SSE connections through the shutdown watch, await HTTP graceful shutdown, and flush queued diagnostics before exiting. `cargo test -p wam-server --test shutdown` exercises the installed test binary with an open SSE and SIGTERM. `python3 -B tests/env/verify_release_services.py` also verifies a real in-flight PostgreSQL write: blocked transaction, SIGTERM, unlock, HTTP 201, exactly-one queue/write, diagnostics flush, restart and idempotent retry. It rehearses backup/restore and Redis outage/manual promotion on owned native services. Production topology and deployment load still require [release acceptance](../../docs/test/release-readiness.md).

## Verified publication

Contribution intake requires `consent=true` and `consent_version="public-sharing-v1"`. Only canonical URL scope, fixed task/action codes and consent metadata are stored. Optional `nonce` makes same-account retries of the same payload idempotent. Nonces are hashed; conflicting payloads and retries of withdrawn submissions are rejected.

The application worker uses the existing Gatekeeper tool layer. A trusted browser replay executable must independently verify both the observed candidate and the proposed record. Its evidence must bind the input digest, include at least three successful candidate runs, and demonstrate improved success or fewer tool calls without worse success. Model output cannot attest to its own replay. Schema/secret/scope/consent, lease generation and current catalog checks run again before commit.

Publication atomically commits the immutable version, author attribution, contribution/job state, decision/audit, author +20 and outbox. Duplicate support adds independent evidence without a new author or +20. Low-confidence proposals wait for review. A reviewer can publish an already replay-verified proposal; approval without one only requests worker verification.

Configure `WAM_LLM_PROVIDER` / `WAM_LLM_MODEL` as documented in [wam-gatekeeper](../wam-gatekeeper/README.md). The HTTP provider reads `WAM_LLM_BASE_URL` and `MODEL_API_KEY`; do not put credentials in committed files.

Model credit and the worker. When the provider says its credit or quota is spent (HTTP 402, `insufficient_quota`, a usage limit), the worker parks that job as `waiting_budget` without using an attempt and closes a durable budget gate: no worker claims a job, runs a replay or calls the model until it reopens, and contributions stay `queued` (recall is unaffected). The gate closes for the time the provider names, or for 15 minutes doubling up to an hour (`WAM_LLM_QUOTA_PAUSE_SECS`, `WAM_LLM_QUOTA_PAUSE_MAX_SECS`). `wam-worker --budget-status` prints it as JSON and `wam-worker --resume-budget` reopens it after credit was added. `WAM_LLM_DAILY_BUDGET_USD` adds a daily (UTC) ceiling on estimated spend. It needs a provider that reports what it costs (for the HTTP provider set `WAM_LLM_PRICE_INPUT_PER_M`, `WAM_LLM_PRICE_CACHED_INPUT_PER_M` and `WAM_LLM_PRICE_OUTPUT_PER_M`, US dollars per million tokens), and the worker refuses to start without one. The ceiling is checked before a job, so the last job can overshoot it. A provider that cannot answer for now (HTTP 429 without a quota signal, 408, 5xx, a connection that fails, times out or is cut off) is waited out the same way: the job waits as `waiting_budget`, the gate closes as `provider_outage` for the larger of `Retry-After` and a backoff from 30 seconds (doubling, at most 10 minutes: `WAM_LLM_OUTAGE_PAUSE_SECS`, `WAM_LLM_OUTAGE_PAUSE_MAX_SECS`), and a job that waits out more than `WAM_LLM_OUTAGE_FREE_WAITS` (default 8) outages starts to be charged attempts again so it cannot block the queue (`0` restores the old behavior). Authentication and request errors (401, 403, 404, 400, 413, 422) are ordinary failures. See [the queue spec](../../docs/spec/queue.md) and [the outage spec](../../docs/spec/provider-outage.md).

Set `WAM_REPLAY_PROGRAM` to an operator-controlled replay executable. Optional `WAM_REPLAY_ARGS_JSON` is a JSON string array passed directly without a shell. The executable receives a `ReplayRequest` JSON object on stdin and returns exactly this evidence on stdout:

```json
{
  "input_digest": "the request's input_digest",
  "samples": 3,
  "baseline_successes": 3,
  "candidate_successes": 3,
  "baseline_tool_calls": 30,
  "candidate_tool_calls": 15
}
```

The numbers must come from actual browser runs and a completion oracle. A constant response is a test fake, not a browser verifier. The adapter bounds output and process duration. A host-specific browser runner is an external deployment input; universal live-site replay is not implemented here.

Use `WAM_RUN_WORKER=1` to run the configured worker inside the server, including local file mode. For PostgreSQL, a separate worker process can share the same database:

```sh
cargo run -p wam-server --bin wam-worker -- --once
cargo run -p wam-server --bin wam-worker
cargo run -p wam-server --bin wam-worker -- --purge-only
```

A separate worker cannot open a file already locked by the local server. Lease heartbeat protects long model/replay calls, expired leases are recoverable, provider errors back off, and quota exhaustion parks without charging an attempt. Fake model providers are available only through tests, not the live worker entrypoints.

## Usage, rollback and privacy

Authenticated recalls issue random owner-bound receipts lasting 30 days. `outcome=success` awards +2 only for explicitly selected receipt `used_version_ids` belonging to another active author. Missing/empty lists award nothing. Outcome and version set are immutable; retries retain the first UTC report day and quotas. Receipt/outcome, scores and outbox commit together.

`ApplicationRepository::retire_version` hides a revision and appends idempotent reversal events. `rollback_version` requires trusted replay, appends a new immutable revision of an independently supported previous payload, retires/reverses the rejected revision and preserves original authorship. Rollback itself earns no new +20. These are operator library operations; no public rollback/takedown endpoint is exposed.

Profile settings offer contribution-consent withdrawal and account deletion. `DELETE /v1/me/account` requires `manage:account`, CSRF for a web session, and `{"confirmation":"DELETE"}`. Device scopes cannot request account-management permission. Deletion erases profiles, identity bindings/sessions/devices, pending payload and identifying attribution/ledger links; a tombstone prevents old credentials from recreating the account. A later OAuth login creates a fresh account. Independently supported memories remain; sole-evidence versions retire before they can be recalled.

Account deletion also erases account-linked quota/history keys and installs a deletion fence, preventing an already authenticated request on another instance from recreating them. Failed external cleanup remains in a kind-filtered durable outbox and retries every 60 seconds after recovery/restart; primary identity deletion remains effective. Keep the rate-limit key secret stable, or explicitly migrate/delete projections when rotating it.

Retention policy `retention-v1-7d` removes contribution payloads after seven days and cancels pending work. Withdrawal removes payload immediately. Public sanitized versions and minimal anonymous score/audit facts remain. Receipt expiry is 30 days; expired/revoked identity artifacts are purged. The server runs retention every 60 seconds, and `--purge-only` supports explicit maintenance.

## Routes

- `GET /healthz`, `GET /v1/metrics`
- `GET /v1/memories`, `GET /v1/memories/vocab`, `POST /v1/memories/recall`
- `GET /v1/memories/{id}?level=1|2`
- `GET /v1/leaderboard`, `GET /v1/profiles/{handle}`
- `GET|PUT /v1/me/profile`, `GET /v1/me/dashboard`, `GET /v1/me/score-events`, `GET /v1/me/usage`
- `GET|POST /v1/me/contributions`, `GET|DELETE /v1/me/contributions/{id}`
- `GET|PUT /v1/me/consent`, `DELETE /v1/me/account`
- `GET /v1/review/contributions`, `POST /v1/review/contributions/{id}/{claim,decision}`
- `POST /v1/receipts/{id}/outcome`
- `GET /v1/auth/{config,session}`, `POST /v1/auth/email/{start,verify}`, `GET /v1/auth/github/{start,callback}`, `POST /v1/auth/logout`
- `POST /v1/link/{start,poll,preview,approve,deny}`
- `GET /v1/me/devices`, `POST /v1/me/devices/{id}/{rotate,revoke}`
- `POST /mcp` — stateless Streamable HTTP, JSON replies, 64 KiB body limit
- `GET /.well-known/oauth-protected-resource[/mcp]`, `GET /.well-known/oauth-authorization-server`
- `POST /oauth/{register,token,revoke}`, `GET /oauth/authorize`
- `GET /v1/oauth/requests/{request}`, `POST /v1/oauth/requests/{request}/decision`

Email login uses server-only `RESEND_API_KEY`, `WAM_EMAIL_FROM`, and an independent `WAM_EMAIL_IDENTITY_SECRET` of at least 32 bytes. All three must be configured together. Six-digit codes expire in 10 minutes, are bound to the requesting browser, allow five wrong attempts, and are limited to five sends per address per hour with a 60-second resend cooldown. Only keyed email identifiers and credential digests persist; email and GitHub accounts are never automatically merged. See the [email spec](../../docs/spec/email-auth.md), [verification record](../../docs/test/email-auth.md), and [domain deployment configuration](../../deploy/README.md).

GitHub web login uses `WAM_PUBLIC_ORIGIN`, `WAM_GITHUB_CLIENT_ID`, `WAM_GITHUB_CLIENT_SECRET` and optional `WAM_GITHUB_REDIRECT_URI`. Sessions expire after 12 hours and devices after 90 days. Review uses the separately configured `WAM_REVIEW_TOKEN`.

MCP preserves the same authentication/scopes as HTTP. Invalid supplied credentials return 401; missing scopes return 403. Browser MCP origins must match `WAM_MCP_ALLOWED_ORIGINS`. Owner usage reads use the same quota/history projection as HTTP. SSE and resumability remain separate work.

## Remote MCP OAuth

Set `WAM_MCP_PUBLIC_URL=https://api.example.com/mcp` to enable the public-client authorization server. Configure either email login or the GitHub provider above and set `WAM_PUBLIC_ORIGIN=https://www.example.com` to the frontend origin serving `/oauth/approve`. Production requires HTTPS; loopback HTTP is supported for local fixtures. The configured MCP URL must have exactly the `/mcp` path without credentials, query or fragment. Its origin is the OAuth issuer. URLs come from configuration, never request Host/Forwarded headers.

Route `/.well-known/oauth-*`, `/oauth/authorize`, `/oauth/register`, `/oauth/token`, `/oauth/revoke`, `/v1` and `/mcp` to this server. Serve `/oauth/approve` from the web build rather than proxying the entire `/oauth` prefix. Configure the frontend server with `Content-Security-Policy: frame-ancestors 'none'`, `X-Frame-Options: DENY`, `Cache-Control: no-store` and `Referrer-Policy: no-referrer` for the approval page. Its client code also refuses framed approval. For a browser-based MCP client, allow its exact origin through `WAM_MCP_ALLOWED_ORIGINS`.

Discovery advertises DCR (`token_endpoint_auth_method=none`), authorization-code + S256 PKCE, resource binding, opaque access tokens, refresh rotation and revocation. CIMD is not advertised. Register only HTTPS or loopback redirects; only a loopback port may vary. Every grant requires web sign-in through a configured email or GitHub provider followed by explicit client/scope approval. It grants only the four device scopes, never account or device management.

Access expires after one hour; grants/refresh credentials after 30 days. Refresh rotates both credentials. Reusing a refresh token or authorization code revokes its grant family, including after repository restart. Connected-device settings and account deletion revoke the whole family; native device-token rotation does not apply to OAuth clients. The shared application repository stores secret digests, not raw request/code/access/refresh values. Public registration and pending grants are bounded and rate limited.

Without `WAM_MCP_PUBLIC_URL`, discovery/token endpoints remain disabled, public reads stay anonymous and local `wam link` remains available. See [repeatable OAuth verification](../../docs/test/remote-oauth.md) for the local Chrome and native Codex evidence and remaining live-account/platform checks.

## Diagnostics

Requests and closed CLI/MCP/web error events are correlated by `x-wam-trace-id` and stored in `wam_diagnostic_events`. Set `WAM_LOG_DATABASE_URL` for a separate PostgreSQL log DB; otherwise the application PostgreSQL URL is used. Local storage defaults to `.wam-data/diagnostics.sqlite` (`WAM_DIAGNOSTICS_PATH` overrides). No raw payloads, URLs, tokens or stack messages are stored. Logs retain seven days with bounded queues and cleanup. See [diagnostics](../../docs/spec/diagnostics.md) for schema, failure behavior, opt-out and operator queries, and [local smoke](../../docs/test/local-smoke.md) for repeatable web/CLI/MCP validation.

HTTP JSON extraction failures preserve their 400/413/415/422 status and return only a fixed JSON error with `Cache-Control: no-store`. Serde's field names, enum values and parser detail never enter the response. This includes errors that occur before authentication in route handlers. The synthetic sink sweep in `web/e2e/security-sinks.spec.ts` exercises this boundary through the actual loopback server.

## Verification

```sh
cargo test -p wam-server
cargo clippy -p wam-server --all-targets -- -D warnings
WAM_TEST_DATABASE_URL='<disposable PostgreSQL URL>' cargo test -p wam-server --test postgres_application -- --ignored
WAM_TEST_REDIS_URL='<disposable Redis URL>' cargo test -p wam-server --test account_cleanup -- --ignored
WAM_TEST_REDIS_URL='<disposable Redis URL>' cargo test -p wam-server --test rate_limit_redis -- --ignored
```

Database tests create and remove isolated schemas. Use a disposable database. They cover migration replay, keys, atomic intake/publish/outcome/score/outbox, injected rollback, concurrent retries, restart, consent fences, immutable versions, reversals and identity deletion. Local tests also cover public-sharing consent, safe migration, scope/secret/replay rejection, independent support and HTTP CSRF/credential revocation. Tests use scripted models and replay fixtures; they do not claim live model/browser acceptance.
