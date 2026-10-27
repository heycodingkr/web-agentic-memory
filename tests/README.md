# Cross-component tests

Planned location for integration and contract tests that span multiple crates or the HTTP/MCP boundary. Unit tests belong beside the crate they cover.

Use isolated local services, disposable data, and synthetic secrets as described in docs/test. Never point an automated test at a user's live browser profile or production account.

Status: scaffolding only. The directories below give test code an agreed home. There is no runner, dependency manifest, or test code yet. The test plan and case IDs live in docs/test.

## Layout

| Path | Layer | Covers | Spec |
|---|---|---|---|
| `env/` | shared | Local server launcher, fixture sites, host profiles, fakes | [docs/test/README.md](../docs/test/README.md) section 3 |
| `lib/` | shared | Python harness library `wamtest` (evidence, processes, CDP, Chrome and Arise adapters, benchmark tooling) and its self-tests | [docs/test/harness-verification.md](../docs/test/harness-verification.md) |
| `smoke/` | L4 | Host smoke (SMK) and agent behavior (BEH) | [docs/test/host-smoke.md](../docs/test/host-smoke.md) |
| `user-flows/` | L3 and beyond | Web UI end to end (WEB) and cross-component user journeys (FLOW) | [docs/test/functional-tests.md](../docs/test/functional-tests.md), [docs/test/user-flows.md](../docs/test/user-flows.md) |
| `security/` | L5 | Attack scenarios (SEC), injection corpus, leak-search harness | [docs/test/security-scenarios.md](../docs/test/security-scenarios.md) |
| `sentinels.json` | shared | Fake secrets, defined in exactly one place (not created yet) | [docs/test/README.md](../docs/test/README.md) section 4 |

Where the other layers live:

- L0 unit tests (CORE, DEDUP) and L1 integration tests (GK, API, LINK, SCORE, PRIV) sit with the crates under `crates/`. They need crate internals and, for L1, a disposable Postgres.
- L2 MCP protocol tests (MCP) are planned as the `generic-mcp` host in `smoke/` (docs/plan/benchmark-readiness.md, W1-6).
- L6 performance benchmarks (PERF): the tooling that needs no browser or model (statistics, block-randomized scheduling, invalid-run classification, trace metrics, action guards) is in `lib/wamtest/bench/`. The runner that drives hosts through scenarios, and the Claude Code and Codex host adapters, are not built yet. Public result summaries go to `benchmarks/`.

The existing test docs name only `security/`, `sentinels.json`, and `target/e2e-logs/`. The other locations are proposals made by this scaffold. Contract tests for OpenAPI and the `/web-memory/v1/*` compatibility adapter (API-08, API-11) get a home when the server work starts.

## Running

Python 3 standard library only; there is no install step.

```
python3 -B -m unittest discover -s tests/lib/selftest -t tests/lib   # no browser, a few seconds
python3 -B tests/env/selfcheck.py                                    # local Chrome, headless, about a minute
python3 -B tests/env/arise_check.py                                  # local Arise, isolated, a window appears briefly
```

The browser checks use a disposable profile under /tmp/wam-*, never touch a running browser or a real profile, and write evidence to `target/e2e-logs/<suite>/<run-id>/`. The cases and their status are in [docs/test/harness-verification.md](../docs/test/harness-verification.md).

## Rules

- The suite name equals the directory name (`smoke`, `user-flows`, `security`). Evidence goes to `target/e2e-logs/<suite>/<run-id>/`, which git ignores.
- Clean up only the processes, tabs, files, and databases the test created.
- Case IDs are agreed in docs/test first. Tests do not mint new IDs.

## Porting sources

Existing harness scripts and tests are snapshotted under `reference/` (read-only). The proposed default is to generalize those Python scripts here instead of rewriting them. Web UI tests use Playwright (TypeScript). The runner and dependency choices are made when `env/` is built.
