# Test environment

Shared fixtures and launchers for the smoke, user-flow, and security suites. Everything here runs on loopback only and cleans up only what it created. The component list follows [docs/test/README.md](../../docs/test/README.md) section 3, plus `fake-oauth` for the OAuth stand-in that WEB-02 needs.

Status: the fixture sites and the Chrome and Arise adapters are implemented and verified (see Implemented so far). The other components are still empty.

| Directory | Role | Used by | Porting source and notes |
|---|---|---|---|
| `wam-test-server/` | Loopback `wam-server` with a disposable Postgres and seed catalogs (normal, malicious for BEH-04, stale for BEH-05) | smoke, user-flows, security | Empty until the server exists. Seed reference: `reference/meshcode/packages/billing/src/web-memory/` (`seed.json`). The old server regression suite is not in the snapshot, only its results |
| `fixture-sites/` | Local editor sites, each with an independent saved-oracle over HTTP that the agent cannot see | smoke (BEH), later benchmarks (BM-FX) | Implemented in `fixture-sites/fixture_server.py` and `static/editor.html`, written fresh. Behavior is modeled on `Fixture` in `reference/agent-browser/scripts/probe_agent_memory_live.py`; that script's HTML template is missing from the snapshot and is not needed |
| `host-profiles/` | Per-host disposable `HOME`, `XDG_CONFIG_HOME`, and `TMPDIR`, plus an adapter to install, register, run non-interactively, and clean up | smoke, later benchmarks (plan W3-1) | Isolation setup in the same probe script. Rules in docs/test/host-smoke.md section 3 (short `TMPDIR`, for example `/tmp/wam-*`). Chrome and Arise are implemented in `../lib/wamtest/chrome.py` and `arise.py` (see docs/test/hosts/arise.md); the CLI hosts are not built yet |
| `fake-llm/` | Deterministic stand-in for the G2 sanitizer, including injection attempts | GK-05 to GK-07, SEC-A17, SEC-B12 | |
| `fake-oauth/` | GitHub OAuth stand-in | WEB-02, SEC-D09 | |

`fixture-sites/` subdirectories follow plan W4-1 and W4-2:

| Directory | Site | Used by |
|---|---|---|
| `rich-editor/` | Rich text editor (`contenteditable`) | BEH-01, BM-FX |
| `iframe-editor/` | Editor body inside an iframe | BEH-01, BM-FX |
| `resume-popup/` | "Resume your draft?" popup | BEH-03, BM-FX |
| `offscreen-save/` | Save button outside the viewport | BM-FX |
| `ui-drift/` | The same site in v1 and v2, for stale-hint regressions | BEH-05, PERF-06 |

If demo scripts need the same sites, reference them from here instead of copying.

`host-profiles/` has one directory per host: `claude-code`, `codex`, `meta-muse`, `arise`, `chrome-mcp`, `generic-mcp`. They pair with the run docs in docs/test/hosts.

The sentinel corpus (`../sentinels.json`) and the injection corpus (`../security/inj-corpus/`) are part of the environment too, but they live at the paths the test docs name.

## Implemented so far

- `fixture-sites/fixture_server.py`: four editor styles (`rich`, `iframe`, `popup`, `offscreen`), an independent saved-oracle on a second loopback port, a 503 save-failure mode, and a verdict with stable reason codes. `ui-drift/` is not built yet.
- Chrome adapter (`../lib/wamtest/chrome.py`): launches the installed Chrome headless in a disposable profile on a random loopback port and drives it over CDP.
- Arise adapter (`../lib/wamtest/arise.py`): launches an isolated second instance of the installed Arise in place and drives it over its CDP port. Recipe and pitfalls: docs/test/hosts/arise.md.
- `selfcheck.py` and `arise_check.py`: 32 scripted behaviors per host check that the oracle's verdict matches the expected one (HARN-01) and that cleanup leaves nothing behind (HARN-04).

Not built yet: `wam-test-server` (needs the server), `fake-llm` (a tool-calling fake for `docs/spec/curator`), `fake-oauth`, `ui-drift/`, and the Claude Code, Codex, and generic MCP host adapters.
