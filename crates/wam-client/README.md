# wam-client

Shared async HTTP transport for CLI and MCP. The WamApi trait is the injection point used by adapters and fake clients.

The client maps response status to safe, low-detail errors and never prints request data or bearer tokens. It canonicalizes public URLs before recall and contribution submission, removing query and fragment data at the shared transport boundary. The server supports public reads plus local demo-token profile, dashboard, and local file-backed contribution calls. Dashboard and contribution access are shared by the CLI and MCP adapters.

Run cargo test -p wam-client from the repository root.
