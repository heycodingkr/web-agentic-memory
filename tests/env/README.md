# Test environment

Shared fixtures and launchers for the smoke, user-flow, and security suites. Service fixtures run on loopback and clean up only what they created. The native browser MCP bridge check below additionally opens its own public example.com tab. The component list follows [docs/test/README.md](../../docs/test/README.md) section 3, plus `fake-oauth` for the OAuth stand-in that WEB-02 needs.

`cargo build -p wam-cli` followed by `python3 -B tests/env/check_browser_mcp_bridge.py` checks the optional persistent `wam browser-mcp` adapter against the installed macOS Arise MCP binary. It preserves the browser's tools, opens one owned background example.com tab, verifies local query/fragment removal before a loopback capture API receives recall, and closes its own tab through the same child connection. It does not use accounts or models. See [native acceptance and limits](../../docs/test/hosts/arise.md); deterministic negative cases run with `python3 -B -m unittest tests/lib/selftest/test_tab_adapter.py -v`.

Status (2026-10-29): editor fixtures, UI drift v1/v2, Chrome/Arise adapters, common benchmark execution, Claude/Codex/generic MCP process/trace adapters, and a 500-query synthetic retrieval corpus are implemented. Its scope/filter labels are generated and not human-reviewed. The product smoke fixture supplies a loopback application and synthetic OAuth; the older placeholder directories are not their implementation. Real model/browser captures and independent human-labeled evaluation corpora remain pending.

| Directory | Role | Used by | Porting source and notes |
|---|---|---|---|
| `wam-test-server/` | Loopback `wam-server` with a disposable Postgres and seed catalogs (normal, malicious for BEH-04, stale for BEH-05) | smoke, user-flows, security | Use `crates/wam-server/examples/smoke_fixture.rs` through `web/playwright.config.ts` for a disposable local application with synthetic OAuth and trusted fake replay. PostgreSQL integration runs separately against `WAM_TEST_DATABASE_URL`; do not duplicate the fixture into this placeholder |
| `fixture-sites/` | Local editor sites, each with an independent saved-oracle over HTTP that the agent cannot see | smoke (BEH), later benchmarks (BM-FX) | Implemented in `fixture-sites/fixture_server.py` and `static/editor.html`, written fresh. Behavior is modeled on `Fixture` in `reference/agent-browser/scripts/probe_agent_memory_live.py`; that script's HTML template is missing from the snapshot and is not needed |
| `host-profiles/` | Per-host disposable `HOME`, `XDG_CONFIG_HOME`, and `TMPDIR`, plus an adapter to install, register, run non-interactively, and clean up | smoke, later benchmarks (plan W3-1) | Chrome/Arise use `../lib/wamtest/chrome.py` and `arise.py`. CLI/generic MCP processes use `../lib/wamtest/bench/host.py`; native JSONL normalization is in `host_trace.py`. Short temporary paths and own-process cleanup follow docs/test/host-smoke.md section 3 |
| `fake-llm/` | Deterministic stand-in for the G2 sanitizer, including injection attempts | GK-05 to GK-07, SEC-A17, SEC-B12 | |
| `fake-oauth/` | GitHub OAuth stand-in | WEB-02, SEC-D09 | Synthetic `FixtureOAuth` lives in the smoke fixture; durable protocol tests and `web/e2e/oauth.spec.ts` cover consent/PKCE/revoke without real GitHub credentials |

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

- `fixture-sites/fixture_server.py`: five editor styles (`rich`, `iframe`, `popup`, `offscreen`, `ui-drift`), an independent saved-oracle on a second loopback port, a 503 save-failure mode, and stable reason codes. `ui-drift` accepts `ui_version="v1"|"v2"` in `new_run` or `/oracle/new`: v2 removes `#save-draft` and moves `#store-draft` above the editor while preserving the task and oracle.
- Chrome adapter (`../lib/wamtest/chrome.py`): launches the installed Chrome headless in a disposable profile on a random loopback port and drives it over CDP.
- Arise adapter (`../lib/wamtest/arise.py`): launches an isolated second instance of the installed Arise in place and drives it over its CDP port. Recipe and pitfalls: docs/test/hosts/arise.md.
- `selfcheck.py`: Chrome 40/40 scripted behaviors plus separation/cleanup checks passed after adding the UI drift v1 style. Earlier Arise evidence remains 32/32 and has not been rerun for the new style. `drift_check.py` separately verifies six Chrome v1/v2 save/recovery/failure cases and cleanup (7/7), without model calls.
- `repeatability_check.py`: repeats the same iframe-editor input against the independent save oracle, reports wall/monotonic elapsed-clock divergence (HARN-03), and assesses TTS CV (HARN-05). A 10-run Chrome 156 sample passed with 1 ms maximum clock divergence and 1.18% TTS CV; see `target/e2e-logs/env-repeatability/20261027-232656-3d58/`.
- `guard_check.py`: enables the CDP Fetch request guard before loading the fixture, clicks the local publish action, and requires `publish_count=0` plus a `blocked-action` trace (`target/e2e-logs/env-browser-guard/20261027-233017-a251/`, pass 2/fail 0).
- `../lib/selftest/test_browser_guard.py`: seven browser-free checks cover allow/deny interception, malformed events, policy failures, listener startup cleanup, and redaction of blocked request URLs to their origin.
- `arise_check.py`: HARN-06 now checks at runtime that isolated Arise read the intended XDG settings file; a rich-editor run passed 7/7 and left the user's Arise processes unchanged (`target/e2e-logs/env-selfcheck-arise/20261027-233155-77c8/`). Other state/cache paths still need host-specific runtime checks.
- `../bench/run.py` and `../lib/wamtest/bench/runner.py`: frozen crossover plans, run isolation, guarded callbacks, explicit terminal events, invalid-run retention and condition statistics. See [benchmark execution](../bench/README.md). Offline replay is grade C and cannot be enabled as a controlled live comparison.
- `../bench/run_host.py` with `../lib/wamtest/bench/host.py` and `host_trace.py`: isolated CLI/generic MCP execution, explicit per-tool grants, streaming JSONL normalization, bounded output/time, private opt-in raw capture, independent oracle verdicts and process-group cleanup. Twenty-two self-tests cover protocol/process/privacy boundaries. `check_host_traces.py` passed success and 401 classification against installed Codex 0.165.0 and Claude Code 2.1.312 with local synthetic model APIs and a real owned MCP child (zero paid model/account calls); see [acceptance and usage](../bench/README.md#native-cli-and-generic-mcp-hosts).
- `../bench/retrieval.py` with `../lib/wamtest/retrieval_corpus.py`: fixed 600-record/1,500-index-row, 500-query synthetic corpus; 100 group-held-out queries; manifest/file hashes; strict references; and a scorer for precision@3, recall@3, MRR, no-match, scope and inactive-memory errors. Labels encode synthetic filter rules, not independent relevance judgments.
- Local verification: run Rust/DB/Redis, Python harness, web checks and Chrome acceptance using the commands in [the test index](../../docs/test/README.md) and [local smoke guide](../../docs/test/local-smoke.md). GitHub Actions workflows were removed on 2026-10-30 at the user's request.

Remaining: real model/browser/WAM captures using the implemented host adapters, independently verified full interception for controlled comparisons, human-labeled recall/curator corpora, PostgreSQL/Redis/WAL and live provider/staging sink collection, dedicated-runner throughput/1M stress, real-site oracles, and runtime path checks for host-specific session/cache stores. The 500-value sentinel inventory and eight-sink local collection are implemented; they do not establish a complete production sink sweep. Local 1k/10k/100k profiles are described below. Product fixture OAuth and native-host model backends are synthetic; they do not establish live GitHub or model behavior acceptance. Issue #10 tracks these items.

## Native release rehearsal

```sh
python3 -B tests/env/check_data_services.py
python3 -B tests/env/verify_release_services.py
python3 -B tests/env/verify_native_install.py
cargo test --locked -p wam-client --test secure_store -- --ignored --nocapture
python3 -B tests/bench/profile_search.py
```

`verify_release_services.py` needs native `initdb`, `postgres`, `psql`, `pg_dump`, `pg_restore`, `redis-server`, `redis-cli`, and Cargo. It creates its own loopback cluster and two Redis processes, checks their data-directory/PID identities, and refuses external database names. It verifies a real contribution/publication/outcome (+20/+2), a custom-format dump restored into another database, all table/schema hashes and immutable-version constraints, device authentication after restore, and PostgreSQL restart. It then blocks an actual API write on a transaction lock, sends SIGTERM, releases the lock and checks HTTP 201, exactly one contribution/queue entry, diagnostics flush, restart and idempotent retry. A stopped Redis primary must cause actual API 503; manual replica promotion and explicit API reconnect must retain the consumed quota and recover. This proves local recovery, not automatic Sentinel discovery, production TLS/proxy topology, or live model replay.

`verify_native_install.py` builds and installs the actual CLI/MCP from this checkout using `cargo install --locked --offline` into an owned directory. It needs the locked dependencies cached plus installed Codex and Claude Code CLIs. It creates HOME/XDG/TMPDIR/CODEX_HOME/CLAUDE_CONFIG_DIR/WAM_CONFIG_DIR before invoking them. It checks installed CLI status, MCP initialize/list of ten tools/positive recall/EOF, repeated host registration and removal, and preservation of another owned registration. It never invokes a model or uses an existing host profile. The native secure-store test uses a unique disposable dummy credential namespace and removes it; it must be run on each target OS separately. Source installation is not signed-release acceptance or real-account OAuth acceptance.

Both runners retain private evidence under `target/e2e-logs/{release-services,native-install}/`, mark synthetic scope and dirty source, and fail on missing tools, timeout, failed oracle, or cleanup residue. `native_services.py` shares only owned native-service startup/cleanup. Search profile instructions and their narrower measurement scope are in [the benchmark guide](../bench/README.md#native-search-profile).

The tool-calling fake for the curator (`docs/spec/curator`) is not here: it is `ScriptedProvider` in `crates/wam-gatekeeper` (Rust), with the invariant fuzz of CUR-16 and the queue and worker tests of QUE-02 to QUE-12. The `fake-llm/` row above is the older G2 sanitizer stand-in and stays empty.
