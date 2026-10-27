# wam-contracts

Versioned JSON DTOs shared by the server, MCP, CLI, and web API boundary.

Request DTOs deny unknown fields. Untrusted profile documents still pass through wam-core constructors before storage; these transport structs do not replace domain validation.

Run cargo test -p wam-contracts from the repository root.
