# wam-client

The shared client also provides endpoint/config-partitioned OS credentials and device start/poll/status/self-revoke transport for the CLI and local stdio MCP. `wam link` stores credentials in the native secure store without a plaintext fallback. HTTPS or loopback HTTP is required, URL userinfo/query/fragment and redirects are rejected, and private anonymous operations are blocked before transport. See [account/host verification](../../docs/test/cli-link.md).

Recall validates URL size, limit and kind locally. When keywords or a task are supplied, the client obtains the public vocabulary before constructing the cache key or HTTP body. It applies the core NFKC/lowercase normalization, drops unknown keywords, removes duplicates, and rejects unknown tasks. Vocabulary remains `no-store`; if it is unavailable, filtered requests fail without sending raw terms. URL-only anonymous fresh/stale cache behavior is unchanged. `for_request()` shares transport/cache resources with an isolated diagnostic reference for each concurrent stdio request.

Shared async HTTP transport for CLI and MCP. The WamApi trait is the injection point used by adapters and fake clients.

The client maps response status to safe, low-detail errors and never prints request data or bearer tokens. It canonicalizes public URLs before recall and contribution submission, removing query and fragment data at the shared transport boundary. The server supports public reads plus local demo-token profile, dashboard, and local file-backed contribution calls. Dashboard and contribution access are shared by the CLI and MCP adapters.

CLI and MCP failures send a closed diagnostic event to the configured API at `/v1/telemetry/events`. Events contain only random event and trace IDs, a fixed source, operation and error code, optional HTTP status, and bounded duration. Failed deliveries queue in a private OS cache directory under `wam/telemetry/<origin hash>/`; at most 100 events are retained for seven days. The next successful API request retries up to 10 events within an 800 ms total network budget. Set `WAM_TELEMETRY=off` to disable reporting and disk queueing, or `WAM_TELEMETRY_SPOOL_DIR` to choose the spool directory. This setting is separate from contribution consent.

Implementation responsibilities are separated: `api.rs` defines the shared trait and endpoint bindings; `http.rs` constructs/configures clients; `transport.rs` prepares requests and decodes errors; `endpoint.rs` validates and builds URLs; `recall.rs` applies anonymous cache policy; `diagnostics.rs` delivers and retries diagnostic events. Storage, device linking and credentials keep their own modules. Public imports remain `wam_client::{HttpClient, WamApi, ApiError}`.

Run `cargo test -p wam-client` from the repository root.
