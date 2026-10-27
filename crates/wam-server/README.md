# wam-server

Axum HTTP adapter around an injectable application service and repository port.

Implemented routes:

- GET /healthz
- GET /v1/metrics
- GET /v1/memories/vocab
- POST /v1/memories/recall
- GET /v1/memories/{id}?level=1|2
- GET /v1/leaderboard
- GET /v1/profiles/{handle}
- GET|PUT /v1/me/profile
- GET /v1/me/dashboard
- GET /v1/me/score-events?limit=20&before=score-00000003
- GET|POST /v1/me/contributions
- GET|DELETE /v1/me/contributions/{id}
- GET /v1/review/contributions
- POST /v1/review/contributions/{id}/claim
- POST /v1/review/contributions/{id}/decision
- POST /v1/receipts/{id}/outcome
- POST /mcp (stateless Streamable HTTP; JSON replies, no server-sent event stream)
- GET /v1/auth/config and /v1/auth/session; GET /v1/auth/github/start and /v1/auth/github/callback; POST /v1/auth/logout
- POST /v1/link/{start,poll,preview,approve,deny}
- GET /v1/me/devices; POST /v1/me/devices/{id}/{rotate,revoke}

Run locally with WAM_DEMO_TOKEN=local-dev-token WAM_REVIEW_TOKEN=local-review-token cargo run -p wam-server. The server binds to 127.0.0.1:3000 by default. `WAM_DEMO_TOKEN` is a development-only bearer credential and is rejected when `WAM_ENV=production`. Profile and contribution routes accept a GitHub web session or a scoped bearer token; contribution submission also requires explicit consent and its version. Review routes require the separate review bearer token. Public routes need no token.

Enable browser GitHub login by setting `WAM_PUBLIC_ORIGIN`, `WAM_GITHUB_CLIENT_ID`, and `WAM_GITHUB_CLIENT_SECRET`; optionally set `WAM_GITHUB_REDIRECT_URI` to the callback registered in the GitHub OAuth app. The local callback defaults to `http://127.0.0.1:3000/v1/auth/github/callback`. Production mode requires HTTPS for the public origin and callback, configured GitHub credentials, and no demo token. Sessions expire after 12 hours; device credentials expire after 90 days. Identity state is stored in `.wam-data/identity.json` with restrictive file permissions and hashed session/device secrets.

The catalog remains seeded process-local demo data. LocalRepository keeps account profiles, handles, rich bio, service links and public visibility in `.wam-data/profiles.json`; invalid snapshots fail startup, and failed writes retain the prior profile state. This file adapter is for one server process. Contribution receipts, review states and audit events use `.wam-data/contributions.json`; usage counters and 30-day owner-bound recall receipts use `.wam-data/usage.json`; append-only successful-use score events use `.wam-data/scores.json`. Atomic file replacement preserves these local records across a single development server restart. A successful outcome adds 2 points only to known other-account authors of explicitly selected receipt `used_version_ids` (up to 3 distinct IDs); an omitted or empty list records the outcome without scoring. Receipt idempotency freezes the outcome, selected version set and original report day, including across restarts and retries; daily consumer/version and consumer/author limits apply; totals feed the leaderboard and personal rank. Anonymous, partial, failed and self-use reports do not award points. Manual review approval still does not publish a memory version or award the separate +20 approval score. Active Gatekeeper jobs are reconstructed in memory on startup. PostgreSQL transactions, production publish/worker, rollback events, and MCP OAuth discovery are not implemented.

`POST /mcp` provides sessionless Streamable HTTP for remote MCP clients. It accepts JSON requests up to 64 KiB and returns JSON responses; GET and DELETE return 405, so unsolicited SSE notifications and resumability are not available. Anonymous read tools are supported. Existing bearer/session authentication and scopes are passed into the same WAM service for private tools. Invalid supplied credentials fail with 401. Set `WAM_MCP_ALLOWED_ORIGINS` to a comma-separated exact-origin allowlist when browser clients send `Origin`; requests without an Origin are intended for native MCP clients.

Run cargo test -p wam-server for router integration tests.
