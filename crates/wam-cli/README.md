# wam-cli

Small wam binary using the same wam-client as the MCP adapter.

Implemented commands:

- wam link [--no-browser] [--json]
- wam unlink [--local-only] [--json]
- wam doctor [--json]
- wam guide print [--host codex|claude-code] [--write]
- wam install codex|claude-code [--local] [--scope user|local|project] [--mcp-bin <path>] [--dry-run] [--json]
- wam uninstall codex|claude-code [--scope user|local|project] [--dry-run] [--json]
- wam status
- wam vocab
- wam recall <url> [--keyword <term>] [--task <category>] [--kind <kind>] [--limit 1..3]
- wam recall --stdin --json (one closed-schema JSON object, up to 16 KiB)
- wam expand <id> [--version-id <recalled-version-id>] [--level 1|2]
- wam browser-mcp --browser-mcp <absolute-program> [--browser-arg=<arg>]...
- wam leaderboard
- wam rank [--json] (requires a linked credential with read:dashboard)
- wam usage [--json] (requires a linked credential)
- wam cache status [--json]
- wam cache clear [--json]
- wam propose <url> --task <compose-draft|navigate|research> --action <navigate|edit|save>... --consent [--nonce <id>] [--json]
- wam contributions [--json]
- wam withdraw <id> [--json]
- wam outcome <receipt-id> <success|partial|failed> [--used-version <version-id>]... [--json]

Add --json to emit one response document. Failures in JSON mode also emit one `schema_version: 1` document on stdout with `error.code`, a safe `error.message`, optional `error.trace_id`, and `error.retry_after_seconds` for rate limits. Exit codes remain unchanged; stderr retains human diagnostics, and help/version retain their normal output. Set WAM_ENDPOINT to select the API; the default is http://127.0.0.1:3000. Private commands use the endpoint-bound OS credential created by `wam link`; `WAM_TOKEN` remains a local development override. `--anonymous` skips both sources. Endpoints require HTTPS or loopback HTTP and cannot contain userinfo, query, or fragment. `outcome` also requires the `report:outcome` scope and a receipt owned by the authenticated account. Recall text output includes the version IDs and authenticated receipt ID. Pass `--used-version` for each version actually used from that receipt (up to 3); omitting it records the outcome without awarding points. Repeating the same outcome and version set is idempotent; changing either is rejected. Daily score limits remain tied to the first report's UTC day. `propose` requires the explicit `--consent` flag and at least one action; it sends a client-canonicalized URL and only selected task/action codes. The server supports transactional PostgreSQL or one local application snapshot. Verified publication awards +20, explicit successful use awards +2, and withdrawn/retired versions cannot earn new points. Reuse `--nonce` for retries of one payload. `--consent` now means public-sharing-v1; previous local-only submissions require fresh consent. The CLI and local stdio MCP share OS credentials. Native macOS Keychain and isolated Codex/Claude registration are verified; live GitHub approval and Windows/Linux host acceptance remain separate.

Example: cargo run -p wam-cli -- recall https://example.com/guide --keyword blog --task compose-draft --json.

Example: `WAM_TOKEN=local-dev-token cargo run -p wam-cli -- propose 'https://example.com/guide?campaign=secret' --task compose-draft --action navigate --action save --consent`.

Example: `WAM_TOKEN=local-dev-token cargo run -p wam-cli -- outcome local-use-000001 success --used-version v1 --json`.

Default contribution output includes available progress stage, reason, relative seconds until the next attempt, and the reflected `memory_id @ version_id (availability)`. JSON retains the full optional progress/reflection objects.

Recall with keywords or a task resolves the public vocabulary before cache lookup or HTTP serialization. Unknown keywords are normalized, deduplicated and dropped; unknown tasks are rejected. The vocabulary response is no-store, so these filtered calls need the API reachable even when recall data is cached. No raw-term fallback is made.

The CLI and local stdio MCP share the bounded anonymous recall cache in `wam-client`. `cache status --json` returns schema version, entry/byte counts and limits; `cache clear --json` returns schema version and `cleared: true`. Text `cache status` reports entry and byte counts; `cache clear` removes derived response data without touching credentials. Authenticated calls always use the server.

`wam status --json` reports the MCP endpoint, authentication (`anonymous`, `linked`, `reauth_required`), contribution consent and numeric catalog contract version. It excludes credentials and account/device identifiers. Use the version ID returned by recall with `expand --version-id` to retrieve that approved version; a missing or retired version returns not found instead of silently using a newer version.

For the optional local Arise bridge, register the following executable and arguments as a stdio MCP server named `wam-browser` in your host:

```sh
/absolute/path/to/wam --endpoint http://127.0.0.1:3000 browser-mcp \
  --browser-mcp /Applications/Arise.app/Contents/MacOS/agent_browser \
  --browser-arg=--mcp-server
```

This keeps one browser MCP child alive for the host session and adds `web_memory_recall_current_tab` alongside the browser's existing tools. Open and operate the tab through this same `wam-browser` entry: ownership proofs from another browser MCP process cannot be reused. The added tool accepts a task category, allowlisted keywords and optional session ID, reads only that connection's selected owned tab, and normalizes its URL locally before sending recall. It checks the target again before recall and rejects foreign, expired, unready or changed tabs. It never acquires another tab as a fallback. Honor the user's request to skip memory. This is explicit tool use; it does not inject automatic background recall or replace the ten-tool memory MCP server.

The added tool explicitly sets all four MCP behavior hints to `false`: recall records usage and can create receipts, repeats can have additional effects, and reading the owned tab's context stays within the bridge/WAM boundary. Negotiated 2024-11-05 omits its annotations; later versions include them. The bridge preserves the upstream browser tools' metadata.

The native macOS Arise browser connection and isolated Codex/Claude registration are verified by [host acceptance](../../docs/test/hosts/arise.md). Other browser connectors, Arise Hub remote OAuth and Windows/Linux runtime acceptance remain separate. Remove this optional entry with `codex mcp remove wam-browser` or `claude mcp remove --scope user wam-browser` according to the host used to register it.

Local setup from this checkout:

```sh
cargo build -p wam-cli -p wam-mcp
target/debug/wam --endpoint http://127.0.0.1:3000 link --no-browser
target/debug/wam --endpoint http://127.0.0.1:3000 install codex --local
target/debug/wam --endpoint http://127.0.0.1:3000 doctor --json
```

Approve the displayed code in the web UI. No token is copied into host configuration. Use the same endpoint and `WAM_CONFIG_DIR` for linking and host registration. Claude supports user/local/project scope; the current Codex registration helper supports user scope. A conflicting existing `web-memory` entry is preserved and returns exit code 4. Dry-run prints a reviewable command or JSON argv without changing settings. Registration does not claim an authenticated or working remote connection.

`unlink` revokes the current device before deleting its local credential; an offline failure retains it for retry. `--local-only` removes it locally and explicitly leaves web revocation to the user. Uninstall removes only the host entry so other hosts can keep their shared credential/cache. Run `wam unlink` and `wam cache clear` separately for complete local cleanup.

`wam guide print` works offline without reading credentials or contacting the API. `--host` selects `AGENTS.md` (default Codex) or `CLAUDE.md`. Only `--write` changes that file in the current directory: it appends or replaces one `<!-- wam:begin -->` / `<!-- wam:end -->` block, preserves surrounding bytes/line endings/permissions and is idempotent. Malformed markers, symlinks, oversized/non-UTF-8 files, a concurrent writer or a detected editor change fail without replacing the file. The source snippet is [`guides/recall-instructions.md`](../../guides/recall-instructions.md).

Run cargo test -p wam-cli for parser, stdin, account lifecycle, registration, and instruction-file preservation tests. See [verification and remaining limits](../../docs/test/cli-link.md).
