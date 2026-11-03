# Trip.com flight search: T vs M, preliminary observation

Last checked 2026-11-03 · issue #270 · preregistration [`flight-trip-20261103.md`](../prereg/flight-trip-20261103.md) (commit 7b03558)

**No speed claim is made and none should be copied from this page into the README or the landing page.** Each condition has at most one run with a usable verdict, one run has no verdict because of a harness defect, one planned run was stopped, and the oracle was amended while the study ran. The preregistration says more than 20% invalid runs make the study invalid; 1 of 3 completed runs has no verdict (2 of 4 started, counting the stopped one). Read this as an exploratory log. The same limits apply to the call, time and token numbers.

## What was run

- Task: on Trip.com (English site, USD, anonymous), find nonstop one-way flights Seoul to Tokyo on 2026-12-15 for 1 adult in economy, show them sorted by lowest price, and answer `<airline> <departure time> <price>` of the cheapest flight. The model starts at `https://www.trip.com/flights/` and has 15 Arise browser tools, no eval and no memory tools.
- Host: codex-cli 0.166.0, `gpt-6-luna`, effort `max`, a fresh ephemeral session per run. Browser: the installed signed Arise app, one disposable instance with an empty profile per run, the MCP pinned to that instance.
- T is the task only. M is the same prompt plus the recalled L1 text of one candidate memory (`web:example-trip-flight-search`): a results-page link recipe, the sort tab, and snapshot limits. It contains UI-operation facts only, no prices and no personal data.
- Live-site limits (owner decision 2026-11-03, recorded on #270): Trip.com only (Skyscanner is not run, its terms forbid software agents), no sign-in, booking or payment, at most 12 results-page loads, a bot check ends a run as `invalid-auth` and is never solved.

## Runs (every run)

| Run | Cond. | MCP calls | First call to done | Failed calls | Results loads | Input tokens (cached) | Frozen-oracle verdict | Final answer |
|---|---|---:|---:|---:|---:|---|---|---|
| b1-M | M | 11 | 66.0 s | 0 | 2 | 364,640 (312,704) | **no verdict** (`invalid-env`, harness) | ZIPAIR 17:15 US$92 |
| b1-T | T | 35 | 108.5 s | 0 | 3 | 2,170,689 (2,033,280) | **fail**: `params_ok` only | ZIPAIR 13:05 US$92 |
| b2-T | T | stopped | | | 0 | | not a result | |
| b2-M | M | 12 | 53.1 s | 0 | 2 | 352,955 (297,344) | **pass**, 10 of 10 checks | ZIPAIR 13:05 US$92 |

Order: b1-M, b1-T, b2-M (b2-T was stopped before b2-M). Started 2026-11-03 07:21:29Z, 07:24:07Z, 07:32:18Z. Time is the primary metric of the preregistration (first MCP tool-call event to model completion); calls are direct MCP calls. No run had a failed call or a turn failure.

Oracle checks per run (Y = true), in the order results page open, params, route, sorted by Cheapest, every card nonstop, first card is the minimum price, answer has price, answer has departure time, answer has airline, no booking page visited:

| Run | results | params | route | cheapest | nonstop | first=min | price | departure | airline | no booking |
|---|---|---|---|---|---|---|---|---|---|---|
| b1-M | n | n | n | n | n | n | n | n | n | Y |
| b1-T | Y | **n** | Y | Y | Y | Y | Y | Y | Y | Y |
| b2-M | Y | Y | Y | Y | Y | Y | Y | Y | Y | Y |

- **b1-M, no verdict.** The oracle read the page while the agent's tab lease was still active (`tab_busy`), got `about:blank`, and every page check came out false. That is a defect in my harness, not a model result. The instance is gone, so the page cannot be re-judged, and the final answer cannot be checked against it. Under `benchmarks.md` section 8 it is `invalid-env` and is left out of success statistics.
- **b1-T, one failed check.** The address shows `nonstoponly=off` because the model used the on-page Nonstop checkbox (the transcript shows a click on the element named Nonstop with role checkbox), and that checkbox does not change the address. The page listed 8 cards, all nonstop, Cheapest was selected, the first card was the minimum price, and the answer matched it. The frozen oracle also requires the address flag `nonstoponly=on`, which the memory teaches. The task as worded never mentions an address flag, so this fails a page that satisfies the task.
- **b2-M, pass.** The page listed two nonstop ZIPAIR cards tied at US$92 (13:05 first, 17:15 second); the answer matched the first. b1-M answered the other tied card, 17:15, which the frozen oracle would accept only if that card were first on its page; that cannot be known.
- **b2-T, stopped.** I tried to stop it about 55 s after the start; the runner ignored SIGINT, so I killed it by hand about 95 s after the start, before any results page had loaded (24 calls had started, none reached the results path). See the deviations below.

## Where the calls and tokens went

Calls and time, split at the call that makes the results page appear (T: the Search click; M: the link navigation), read from the transcripts and `calls.json`:

| Run | Calls to reach the results page | Time to reach it | Calls on the results page | Time on the results page |
|---|---:|---:|---:|---:|
| b1-T | 27 | 69.7 s | 8 | 38.8 s |
| b1-M | 4 | 10.9 s | 7 | 55.1 s |
| b2-M | 5 | 17.8 s | 7 | 35.3 s |

- T filled the search form in calls 3 to 25 (departure and arrival city, calendar, passengers), clicked the Search button by position after a screenshot (call 27) because the snapshot did not list it (the memory says so), and then used the on-page Nonstop checkbox and the Cheapest tab.
- Both M runs opened the memory's results link at call 4 or 5, and the page showed the right route, date, one adult, economy and nonstop on the first try. The memory removed the form phase: 27 calls and 69.7 s to reach the results page for T against 4 to 5 calls and 10.9 to 17.8 s for M.
- On the results page the M runs did not beat T: 7 calls each against 8, and b1-M took longer (55.1 s against 38.8 s); its calls on that page included a scroll and extra screenshots after the sort click. All three runs clicked the Cheapest tab from a screenshot position, and b2-M's first selector click did not change the sort. The candidate says to click Cheapest but not how, so this step is what the numbers show; a better instruction would be a new candidate and a new measurement.

Tokens, as reported by the Codex turn (input includes the cached part):

| Run | Input, total | of which cached | Input, uncached | Output | of which reasoning |
|---|---:|---:|---:|---:|---:|
| b1-T | 2,170,689 | 2,033,280 (93.7%) | 137,409 | 5,191 | 2,629 |
| b1-M | 364,640 | 312,704 (85.8%) | 51,936 | 2,375 | 1,670 |
| b2-M | 352,955 | 297,344 (84.2%) | 55,611 | 3,176 | 2,060 |

- Total input is summed over every model step, and each step re-reads the whole conversation, so it grows with the number of calls and is mostly cache reads. T used 6.0 to 6.2 times the total input of M, but only 2.5 to 2.6 times the uncached input and 1.6 to 2.2 times the output tokens. The 6x figure is therefore not a 6x cost saving; what the split saves in money depends on the price of cached input against uncached input and output, which I did not model. The M figures include the memory text in the prompt (1,004 characters).
- Same limits as the rest of this page: one T run that fails the frozen oracle, one M run with a verdict, a memory written from the same route and date.

## What this does and does not show

- By the preregistered rule, a failed run has null time and calls, so the primary metrics are undefined for T. The preregistered claim rule (20% median reduction with a positive bootstrap interval) cannot be met with at most one valid run per condition.
- Descriptively, the two M runs used 11 and 12 calls (66.0 s and 53.1 s) and the one T run used 35 calls (108.5 s). Input tokens followed the call count: about 6 times the total for T, about 2.5 times the uncached part and 1.6 to 2.2 times the output (previous section). With one T run there is no estimate of how much T varies between runs.
- Dropping the address flag from the oracle would pass b1-T. That reading was chosen after seeing the result, so it is exploratory. It is shown only so the frozen verdict is not mistaken for "T did not reach the page".
- The difference in the `params` check is partly built in: the memory carries the exact link parameters that this oracle checks, and T reached the same visible state through the form.
- The call difference is what a recipe for the identical search should give. The memory was written from Phase 0 on this same route and date, so this is not evidence for other routes, dates or sites; there is no held-out task.

Other limits: one model, effort and host; one day of live data (prices and the order of tied cards change); a fixed order that is not randomized; sort-tab clicks count as results-page loads (b1-T had 3 loads in total); a fresh profile removes cookies but not site-side effects; the Mac must stay untouched during a run (b2-T's first clicks were refused as hidden, which suggests the disposable instance had lost focus; I did not investigate further).

## Deviations from the preregistration

| Item | Preregistered | What happened |
|---|---|---|
| Oracle lease wait | oracle `0bda03c1…` reads the page when the model exits | b1-M hit the still-active lease. Version 2 (`8ec97be8…`) waits up to 150 s for the lease before reading, and records `invalid-env` if it still cannot read. b1-T used version 2 (waited 99 s). |
| Oracle tie evidence | none | Version 3 (`a9959533…`) also records every card tied at the minimum price and whether the answer matches any of them. It never changes `pass`. b2-M used version 3 (waited 102 s). |
| Stopping b2-T | "a run in progress is never aborted for the budget" | I stopped b2-T with 0 results loads. After b1-T the budget stood at 9 of 12, which could not fit both b2-T and b2-M, and no M run had a verdict. The M run was run instead. The runner did not react to SIGINT (it was started in the background), so I killed the runner, the Codex process group and the disposable Arise instance by hand and removed their temp roots. |
| Navigation guard | none | `navwatch.py` (`ec877aa5…`) would have stopped b2-M's Codex process if it had exceeded 3 results loads. It did not fire. |
| Run order | b1-M, b1-T, b2-T, b2-M | b1-M, b1-T, b2-M |
| Invalid rate | at most 20% | 1 of 3 completed runs (2 of 4 started) |

Each oracle version is kept beside the frozen one in the evidence directory; every run's `oracle.json` records which behavior applied.

## Seed decision

The preregistered rule: add the candidate to the default seed only if every statement was verified live in Phase 0 and both M runs pass the oracle with no more tool calls and no more time than the paired T run. Every statement was verified live on 2026-11-03, but the second condition is **not met**: b1-M has no verdict and b2-M has no paired T run. The candidate is therefore **not added to the default seed**.

The candidate is kept byte-identical to the file hashed in the preregistration (`1fdf24e7…`) as [`flight-trip-20261103.candidate.json`](../prereg/flight-trip-20261103.candidate.json). No build loads it. To enable it later: copy it to `crates/wam-server/seed/travel_memories.json`, add `pub const TRAVEL_SOURCE: &str = include_str!("../seed/travel_memories.json");` and the `("travel_memories.json", TRAVEL_SOURCE)` entry in `examples::load()`, then run the seed tests. With the candidate wired in on 2026-11-03, `cargo test -p wam-server --test examples` passed (8 tests, including the record validator), so enabling it needs no content change. Any public text about it should say only what the individual runs above showed.

## Budget, safety and cleanup

- Results-page loads, counted from each disposable profile's Chromium History: 4 in exploration, 2 + 3 + 2 in the runs, 0 in the stopped run, 11 of 12 in total. One load is left, which cannot fit another run.
- The History of each finished run shows only `/flights/` and `/flights/showfarefirst`: no sign-in, passenger, booking or payment path. No verification page appeared in the pages the oracle read (b1-T, b2-M); b1-M's agent reached results and answered.
- The Codex login copy lived in a per-run 0600 temp directory and was removed after every run (by hand for the stopped one). `~/.codex/auth.json` was not modified (the three finished runs record `original_auth_unchanged: true`). The Arise instance used for everyday work, with its tabs and profile, was not touched.

## Options for continuing (owner decision)

1. Stop here; this page is the record.
2. Raise the cap from 12 to 16 for one more pair (T then M, about 5 loads) after committing an amendment: the nonstop requirement is judged from the page state (every listed card is nonstop) instead of the address flag, the answer may match any card tied at the minimum price, the oracle waits for the lease, and focus loss is `invalid-env`. That makes the seed rule evaluable. It still leaves n = 2 per condition, so no speed claim.
3. Enable the candidate in the seed as an owner override of the preregistered rule.

## Evidence (local, gitignored)

`target/e2e-logs/bench/flight-20261103/`: `runs/<run>/{result.json,oracle.json,calls.json,final-message.txt,stdout.jsonl,History.sqlite}` for b1-M, b1-T and b2-M; `runs/b2-T.aborted/` (partial transcript and note); `harness/` (frozen and amended oracles, `run_one.py`, `iso_arise.py`, `navwatch.py`); `drive.log`, `navwatch.log`, `tos-check.md`. Hashes (sha256): `oracle.prereg.py` 0bda03c1fe117db3e123416239bf460f12170217bb14495f2bce48eba949bda7; `oracle.v2.py` 8ec97be89ef67fae285c4537fc1bd0aff3759412ce6cdd79fab6e5dcf2753f03; `oracle.py` (v3) a9959533525639d1077855fa149882e35c7c45a6044f40c4bf9c7b1c8b228679; `navwatch.py` ec877aa5b9b1c408384cb5cce7454cdde0ac171bbfff90929b761e7a22537931. Page captures and full transcripts are not committed.
