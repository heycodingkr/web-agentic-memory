# Preregistration: 2026-11-03 / Trip.com flight-search memory (T vs M)

Status: frozen before any measured run. This is an uncontrolled development comparison (grade C) with an explicitly supplied candidate L1, not automatic WAM integration. The owner chose Trip.com only; Skyscanner is excluded (see "External runs").

## Hypothesis and primary metrics

- Hypothesis: one task-specific Trip.com hint (results-page link recipe, sort tab, snapshot limits) lets the model reach the verified nonstop, price-sorted results page and the correct answer in fewer tool calls and less time than the same task without the hint, with the oracle still passing.
- Primary metrics: time from the first MCP tool-call event to model completion; number of direct MCP tool calls through completion.
- Claim rule (from the template): at least 20% median reduction in both metrics with a positive 95% bootstrap CI (10,000 samples), success not lower, average errors not higher. **With 2 pairs this rule cannot be met**, so every number is a preliminary observation and no speed claim will be made.
- Other metrics (failed tool calls, reported token usage, results-page navigations) are exploratory.

## Fixed inputs

- Host: codex-cli 0.166.0, model gpt-6-luna, effort max; fresh ephemeral CODEX_HOME per run with a 0600 copy of the existing login (removed after each run), web search, shell and multi-agent off, approval never, sandbox read-only.
- Browser: installed Arise 0.1.67 app and its installed MCP, one disposable instance per run (own HOME, XDG, TMPDIR, empty CEF profile; update check, telemetry, Arise web memory and auto recall off). The MCP is pinned to that instance with an explicit control socket and cannot launch an app. The instance is brought to the front before the run. Allowed tools (15, no eval, no memory tools): browser_new_tab, browser_navigate, browser_snapshot, browser_screenshot, browser_click, browser_type, browser_press_key, browser_wait_for, browser_read, browser_url, browser_scroll, browser_dialog, browser_tabs, browser_select_tab, browser_close_tab.
- Site: Trip.com flights, anonymous, English UI, USD, as served on 2026-11-03.
- Prompts (sha256): T ea08161747ed057fde3f29b99b12efb8c9ca728abdebd7e1978760ab8d8c8373; M ce59efebb9923f32c598c2e2173e3e6b3c82aa706271a52b902e1aba1746b5b9. M is T plus one block: the recalled L1 text below the rules. Both start at the flights page and tell the model to open its own tab and select it (a tab created by the MCP is a background tab and screenshots time out until it is selected).
- Memory: id web:example-trip-flight-search, version ver-a3c2734812dc829a50f867eb9d402abc3b702e819b8506aa, recalled for the flights landing URL from an isolated local wam-server built from the working tree. L1 text sha256 0b2728bac9c4c3762a7bf75a613394ab82282bd06a95b3cfac8913a7d8910871 (1004 chars). Seed file sha256 1fdf24e702ce39bb0e45951efb0e7dd81d73dea31345ad06123d01316c1ea726. No L2 and no memory tools for the model.
- Harness (sha256): run_one.py 08fb0d9cf2ec8710ef66792bc3edc4e46bb5188ecab2b18fdf945d29159def41; oracle.py 0bda03c1fe117db3e123416239bf460f12170217bb14495f2bce48eba949bda7; iso_arise.py efe7535ffd2b979ab8753250ae65338a8f1832e99698c252dc0bdf78515c22f6. Evidence root target/e2e-logs/bench/flight-20261103/ (gitignored, mode 0600/0700).
- Task: nonstop one-way flights Seoul to Tokyo on 2026-12-15, 1 adult, economy, sorted by lowest price; the final answer is one line "<airline> <departure time> <price>".
- Order (alternating, not randomized): b1-M, b1-T, b2-T, b2-M. A third pair b3 (M, T) runs only if the owner raises the load cap; otherwise the study ends with n=2 per condition.

## Independent oracle and success definition

A separate MCP session pinned to the same disposable instance runs after the model has exited (its leases are gone). It checks: the page is a flights results page; the address carries ddate=2026-12-15, triptype=ow, quantity=1, class=y, nonstoponly=on, dcity in {sel, icn, gmp} and acity in {tyo, nrt, hnd}; the selected sort tab (aria-selected) is Cheapest; every listed card is nonstop; the first card has the minimum price among the listed cards; the final answer contains that card's price, departure time and airline; no booking, passenger, login, register or payment path was ever visited (Chromium History DB of the disposable profile). The model's words alone never decide success.
Controls run on 2026-11-03 against a real results page: the correct answer passed; a wrong price, a wrong departure time and a page left on the Recommended sort each failed for exactly the expected check.

## Exclusions and recovery

- Retain every run and its reason. Labels: invalid-auth (bot or verification check shown), invalid-env (the disposable Arise exited, focus lost), host-did-not-start, turn-failed, wall-timeout. More than 20% invalid makes the study invalid. Ordinary task or UI failures stay failures with null time/calls in success denominators.
- Bound per run: 600 seconds and 150 MCP calls; no retries. The runner stops its own process groups and removes its temp roots and the login copy on every exit path.

## External runs

- Terms check (2026-11-03, evidence tos-check.md): Skyscanner explicitly forbids unauthorized automated programs, software agents and bots, so it is not run. Trip.com has no explicit automation clause but limits use to private use, bars commercial or business purposes and heavy load, so under docs/test/benchmarks.md section 2 rule 2 it is unclear. The owner overrode that rule for Trip.com only, in chat on 2026-11-03 (recorded on issue #270).
- Override limits: no account, login, booking or payment; a CAPTCHA or bot check ends the run as invalid-auth and is never solved; fresh disposable profile per run.
- Load budget: at most 12 results-page navigations in total, counted per run from the disposable profile's Chromium History (one row per main-frame navigation; history.json over-counts and is not used). Exploration used 4 (search, Cheapest click, deep link, Recommended click: a sort tab click navigates to the same address). Each run needs at least 2 (the search and the price sort), so 8 remain and at most 4 runs fit. A run starts only if the cumulative count is at most 10; a run in progress is never aborted for the budget, and any overshoot is reported.
- Memory content is UI-operation facts only (no prices, no personal data).

## Reporting

- Report every run: success, tool calls, time, failed calls, tokens, navigations, with the oracle checks. With n=2 per condition report the individual runs, not medians or intervals.
- Limitations stated up front: one route and date; the memory was written from the same task (Phase 0 on the same route and date), so there is no held-out task and no claim of generalization; fixed alternating order; live site state and prices change; a fresh profile per run removes cookies but not site-side effects; sort clicks count as navigations; the model chooses its own path in both conditions.
- Seed inclusion rule (no speed claim): add the candidate to the default seed only if every statement was verified live in Phase 0 and both M runs pass the oracle with no more tool calls and no more time than the paired T run; otherwise do not add it and report why. Any public text says only what the individual runs showed.
