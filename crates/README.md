# Rust workspace

The root Cargo workspace has two runnable crates: wam-core and wam-gatekeeper. The server, MCP bridge, and CLI directories remain responsibility boundaries and placeholders until their adapter contracts are implemented.

- [wam-core](wam-core/README.md): pure domain contracts, URL/keyword normalization, memory index validation, profile content validation, and deterministic recall/leaderboard ranking
- [wam-server](wam-server/README.md): HTTP API, authentication, catalog, and remote MCP endpoint
- [wam-gatekeeper](wam-gatekeeper/README.md): memory curator (tool layer, in-memory queue, budget and worker), model-provider seam (fake, local Claude/Codex CLI; HTTP API later), scenario runner
- [wam-mcp](wam-mcp/README.md): local stdio bridge and remote service client
- [wam-cli](wam-cli/README.md): setup, diagnostics, manual recall, and host configuration

Module boundaries and the test strategy are described in [module boundaries](../docs/design/module-boundaries.md).