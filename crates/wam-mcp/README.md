# wam-mcp

MCP tool catalog and protocol dispatcher shared by the local stdio adapter and the server's remote Streamable HTTP endpoint. The stdio binary is backed by the shared wam-client. The dispatcher remains transport-independent so tool calls use the same contract on both transports.

Available tools (five reads and four local contribution operations):

- web_memory_status
- web_memory_recall
- web_memory_expand
- web_memory_leaderboard
- web_memory_my_rank
- web_memory_propose (requires WAM_TOKEN and explicit consent fields)
- web_memory_my_contributions (requires WAM_TOKEN)
- web_memory_withdraw (requires WAM_TOKEN)
- web_memory_outcome (requires WAM_TOKEN with `report:outcome`; caller must own the receipt)

Run stdio with WAM_ENDPOINT=http://127.0.0.1:3000 cargo run -p wam-mcp. Remote clients connect to the server's `POST /mcp` endpoint using stateless Streamable HTTP. The HTTP route currently returns JSON responses, accepts requests up to 64 KiB, and does not provide GET/SSE streaming or OAuth discovery. Contribution submit/list/withdraw and owner-bound outcome reporting use local file-backed stores. A success report can add a local score event only for known different authors of the explicitly selected `used_version_ids` from that receipt (up to 3 distinct IDs). Omitting the list or sending an empty list records the outcome without scoring. The outcome and selected version set are immutable after reporting, and retries preserve the first report's UTC day for daily score limits; production PostgreSQL and score outbox are not implemented. Private rank, contribution writes, and outcome reports need an authenticated token with the matching scope.

Run cargo test -p wam-mcp for tool catalog and protocol tests.
