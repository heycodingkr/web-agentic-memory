# Rust workspace

The Cargo workspace separates the pure domain, transport contracts, server use cases, HTTP client, and user-facing adapters.

- [wam-core](wam-core/README.md): pure URL, vocabulary, recall, memory, profile, and rank rules
- [wam-contracts](wam-contracts/README.md): versioned JSON DTOs shared across adapters
- [wam-server](wam-server/README.md): Axum routes and injectable application/repository ports
- [wam-client](wam-client/README.md): shared async HTTP client used by MCP and CLI
- [wam-mcp](wam-mcp/README.md): local stdio MCP read tools, including leaderboard, owner usage, and caller rank
- [wam-cli](wam-cli/README.md): status, usage, vocabulary, recall, expand, cache, leaderboard, and personal rank commands
- [wam-gatekeeper](wam-gatekeeper/README.md): memory curator (tool layer, in-memory queue, budget and worker), model-provider seam (fake, local Claude/Codex CLI; HTTP API later), scenario runner

The server supports local snapshot and PostgreSQL application repositories, GitHub OAuth/session and device linking, owner-bound usage receipts, score ledger events, and a Gatekeeper worker seam. Production startup requires PostgreSQL, Redis rate limits, and secret configuration; production OAuth credentials, trusted ingress, live Redis/PostgreSQL behavior, and scale measurements still require deployment verification. The CLI and local stdio MCP use the shared `wam-client` cache for anonymous recall. See the [module boundaries](../docs/design/module-boundaries.md) for adapter responsibilities and remaining production boundaries.
