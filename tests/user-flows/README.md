# User-flow tests

Web UI end-to-end tests and cross-component user journeys. Specs: WEB-01 to WEB-15 in [docs/test/functional-tests.md](../../docs/test/functional-tests.md) and the flow list in [docs/test/user-flows.md](../../docs/test/user-flows.md) (skeleton; no flows are defined yet).

Status: scaffolding only.

| Directory | Contents | IDs | Porting source |
|---|---|---|---|
| `web/` | Web UI end-to-end tests (Playwright). Login goes through `../env/fake-oauth/` | WEB-01 to WEB-15 | |
| `journeys/` | User journeys across web, server, and MCP | FLOW-xx (not defined yet) | `reference/agent-browser/scripts/probe_web_memory_ui.py` (admin approval flow; includes a geometry check that stabilizes coordinate clicks) |

- Start the server with `../env/wam-test-server/`. Never use a production server.
- Pass or fail comes from an independent oracle (server state, database, logs). A success message on screen is not enough.
- Evidence: `target/e2e-logs/user-flows/<run-id>/`.
