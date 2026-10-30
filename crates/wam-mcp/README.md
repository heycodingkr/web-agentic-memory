# wam-mcp

MCP tool catalog and protocol dispatcher shared by the local stdio adapter and the server's remote Streamable HTTP endpoint. The stdio binary is backed by the shared wam-client. The dispatcher remains transport-independent so tool calls use the same contract on both transports.

Both transports validate the same JSON-RPC 2.0 request envelope before dispatch: object messages, string/integer request IDs, object params, and notifications without IDs. Malformed stdio requests return error `-32600`; malformed HTTP requests return HTTP 400 with the JSON-RPC error. Valid notifications produce no protocol response.

Available tools (four public reads and six authenticated account/contribution operations):

- web_memory_status
- web_memory_recall
- web_memory_expand
- web_memory_leaderboard
- web_memory_my_rank
- web_memory_my_usage
- web_memory_propose (authenticated; explicit consent fields required)
- web_memory_my_contributions (requires a linked OS credential, remote OAuth credential, or development WAM_TOKEN)
- web_memory_withdraw (authenticated contribution owner)
- web_memory_outcome (authenticated receipt owner with `report:outcome`)

`web_memory_my_usage` returns the caller's owner-only quota and 30-day request activity. `web_memory_status` reports the endpoint, `anonymous`/`linked`/`reauth_required` auth state, contribution consent state, and numeric MCP catalog contract version. It never returns credentials, account IDs, email, scopes, or device names. Local stdio also reports cache entry/byte counts; remote MCP has no local cache. `web_memory_expand` accepts an optional recalled `version_id` and pins the detail lookup to that immutable version. Superseded versions remain readable while the memory is still public; retired versions return not found.

Run stdio with `WAM_ENDPOINT=http://127.0.0.1:3000 cargo run -p wam-mcp`. Remote clients use `POST /mcp` with stateless Streamable HTTP responses and a 64 KiB request limit. JSON is the default; when SSE has a higher Accept quality, the response contains one `message` event. `GET /mcp` accepts an SSE connection with an initial comment and 15-second keep-alive, capped at 64 concurrent streams. Both methods enforce Origin, protocol and credential checks. Streams are `no-store`, release capacity on disconnect, and close during server shutdown. No unsolicited workflow events or replay are provided; `Last-Event-ID` is rejected. Configuring `WAM_MCP_PUBLIC_URL` enables standard protected-resource/authorization metadata, public DCR, S256 PKCE consent, refresh rotation, and revocation. See [OAuth setup and acceptance](../../docs/test/remote-oauth.md). Website login and host authorization are separate. Local synthetic Chrome and isolated native Codex OAuth have been verified; live GitHub, Claude/Arise OAuth and other OS acceptance are pending.

Contribution submit/list/withdraw and owner-bound outcome reporting use the server's transactional PostgreSQL or local snapshot repository. Contribution consent is public-sharing-v1; an optional nonce makes same-payload retries idempotent. A success report can add a score event only for known different authors of the explicitly selected `used_version_ids` from that receipt (up to 3 distinct IDs). Omitting the list or sending an empty list records the outcome without scoring. The outcome and selected version set are immutable after reporting, and retries preserve the first report's UTC day for daily score limits; verified version publication/author +20, successful-use scores, reversals and outbox are committed together by the server. Live curation requires model and trusted browser replay configuration (see wam-server/README.md). Private tools require the matching credential scope; remote clients use their OAuth bearer credential.

Local stdio loads the same endpoint/config-bound OS credential as `wam link`. `WAM_TOKEN` is an explicit development override; `WAM_ANONYMOUS=1` skips both sources. Link/install never writes tokens into environment variables or host config. See [CLI account and host verification](../../docs/test/cli-link.md).

The optional [`wam browser-mcp` bridge](../wam-cli/README.md) keeps a separate Arise browser MCP connection alive and adds an explicit current-tab recall tool. It is not part of this ten-tool memory catalog. A stale credential gives HTTP 401 on remote MCP; local `web_memory_status` can report `reauth_required` without retrying anonymously.

Run cargo test -p wam-mcp for tool catalog and protocol tests.
