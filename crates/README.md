# Rust workspace

The Cargo workspace separates the pure domain, transport contracts, server use cases, HTTP client, and user-facing adapters.

- [wam-core](wam-core/README.md): pure URL, vocabulary, recall, memory, profile, and rank rules
- [wam-contracts](wam-contracts/README.md): versioned JSON DTOs shared across adapters
- [wam-server](wam-server/README.md): Axum routes and injectable application/repository ports
- [wam-client](wam-client/README.md): shared async HTTP client used by MCP and CLI
- [wam-mcp](wam-mcp/README.md): local stdio MCP read tools, including leaderboard and caller rank
- [wam-cli](wam-cli/README.md): status, vocabulary, recall, expand, leaderboard, and personal rank commands
- [wam-gatekeeper](wam-gatekeeper/README.md): memory curator (tool layer, in-memory queue, budget and worker), model-provider seam (fake, local Claude/Codex CLI; HTTP API later), scenario runner

The server catalog uses seeded process-local demo storage; account profiles and public visibility persist to the single-process profiles.json adapter. Contribution submit/list/detail/withdraw receipts use an atomic local JSON file for the single development server, with CLI and local stdio MCP adapters. They do not enqueue Gatekeeper work or award points. PostgreSQL, OAuth, receipt/outcome, and score ledger are not implemented. See the [module boundaries](../docs/design/module-boundaries.md) for the contracts and remaining production boundaries.
