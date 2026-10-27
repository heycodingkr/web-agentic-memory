# wam-cli

Small wam binary using the same wam-client as the MCP adapter.

Implemented commands:

- wam status
- wam vocab
- wam recall <url> [--keyword <term>] [--task <category>] [--kind <kind>] [--limit 1..3]
- wam expand <id> [--level 1|2]
- wam leaderboard
- wam rank [--json] (requires WAM_TOKEN for the private dashboard)
- wam propose <url> --task <compose-draft|navigate|research> --action <navigate|edit|save>... --consent [--json]
- wam contributions [--json]
- wam withdraw <id> [--json]
- wam outcome <receipt-id> <success|partial|failed> [--used-version <version-id>]... [--json]

Add --json to emit the response. Set WAM_ENDPOINT to select the API; the default is http://127.0.0.1:3000. Private rank and contribution commands require WAM_TOKEN. `outcome` also requires the `report:outcome` scope and a receipt owned by the authenticated account. Recall text output includes the version IDs and authenticated receipt ID. Pass `--used-version` for each version actually used from that receipt (up to 3); omitting it records the outcome without awarding points. Repeating the same outcome and version set is idempotent; changing either is rejected. Daily score limits remain tied to the first report's UTC day. `propose` requires the explicit `--consent` flag and at least one action; it sends a client-canonicalized URL and only selected task/action codes. Contribution receipts and outcome events use local file-backed stores; production database, secret-store, and device-link flows are not implemented.

Example: cargo run -p wam-cli -- recall https://example.com/guide --keyword blog --task compose-draft --json.

Example: `WAM_TOKEN=local-dev-token cargo run -p wam-cli -- propose 'https://example.com/guide?campaign=secret' --task compose-draft --action navigate --action save --consent`.

Example: `WAM_TOKEN=local-dev-token cargo run -p wam-cli -- outcome local-use-000001 success --used-version v1 --json`.

Run cargo test -p wam-cli for parser and command validation tests.
