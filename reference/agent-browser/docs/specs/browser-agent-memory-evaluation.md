# Browser agent memory evaluation

## Question and existing state

Does a site workflow memory reduce startup wandering and unblock writing, beyond the current screenshot/native-click/native-keyboard tools? At the start of this evaluation, existing browser_memory tools implemented domain-aware lookup and layered notes, but automatic turn-start recall and a real-model comparison were unverified. Both are now implemented and exercised below. A successful scripted tool probe does not establish that the chat agent can choose the workflow.

## Dependency-ordered atomic goals

| Goal | Route / owner | Output | Independent parent check | Depends on |
| --- | --- | --- | --- | --- |
| A: inventory | parent reads the exact chat/control/memory code; codex:gpt-6-luna may summarize existing scripts/tests read-only | cited entrypoint/fixture map; this spec | parent opens every cited location used in implementation; clean git status/diff first | none |
| B: chat test transport | parent | reuse or add an opt-in isolated-app entrypoint to submit a real chat request and observe completion, tool calls and errors | failing focused test first for any behavior change; run a synthetic prompt through the same UI send/agent path, not a scripted sequence impersonating the agent | A |
| C: controlled memory conditions | parent | empty memory; seeded verified domain/task memory with manual recall; identical notes injected as bounded turn context | assert domain isolation, token bound, untrusted-hint framing, and API/CLI parity; inspect the actual model input/evidence | A, B |
| D: editor benchmark | parent | fresh isolated app/config/profile/session for each condition; Medium-like rich editor and Naver-like iframe workflow; identical short title/two paragraphs | independent server-side/read-only oracle checks exact text, native keyboard events, saved draft and no publish; inspect screenshots | B, C |
| E: live attempts and report | parent | comparable real-model runs, metrics and blocker classification; authenticated live-site draft attempt when available | parent runs all builds/tests/browser loops, reads traces and images; distinguish local fixture results from Medium/Naver server persistence; do not claim general speedup from one trial | D |

Delegation is gathering only, read-only: no builds, tests, browser input, credentials, external writes or edits. Use the first usable authorized backend, explicit background mode, retain its job ID and collect completion. Implementations, judgments and verification stay with the parent.

## Test matrix and inputs

- Conditions: tools-only (empty DB), manual memory (seeded DB; existing recall tools), injected memory (same notes appended at turn start). All use the same current tool implementation, model, effort and user request; fresh sessions prevent carry-over. Timings are observations, not a statistical claim.
- Task: type a fixed title plus two short paragraphs and leave a draft. No publishing. Use separate domain-scoped fixture identities for rich editor and iframe editor, and the same expected text in all conditions.
- Fixture blockers: title/body focus distinction; editor input rejected without trusted native events; editor inside an iframe; delayed save and an explicit save error. A memory may explain how to focus or verify saving but cannot turn a server/authentication failure into success.
- Metrics: completion, exact title/body, save oracle, tool calls/errors/retries, screenshot calls, recall/expand calls, time to first valid input, wall time, and any false saved/published claims.
- Empty DB, unrelated domain and stale/adversarial note: no unrelated workflow injection, no instruction authority escalation, bounded L0/L1 only; deeper notes require expansion. Secrets stay in Vault, never benchmark notes.
- Missing authenticated real-site session: classify authentication as a blocker and retain that evidence. Do not silently substitute fixture success for a live Medium/Naver result.

## Implementation decisions

- Automatic context uses the existing SQLite store, up to 100 recent domain-restricted candidates and task ranking. Older same-site notes beyond that candidate window still require explicit recall. It includes at most three L0/L1 notes in 400 estimated tokens; L2/L3 require expansion. A read-only connection waits at most 20 ms for locks, creates no DB and suppresses storage failures so a chat can proceed. Both API and CLI paths use the same context block. `browser_memory_auto_recall` is enabled by default and can be toggled in Settings > Chat.
- `AGENT_BROWSER_SELFTEST_PROMPT` / `AGENT_BROWSER_SELFTEST_MODEL` already enter the real UI send path. An optional `AGENT_BROWSER_SELFTEST_START_FILE` gate allows the harness to capture the untouched editor and seed notes before that turn starts. `AGENT_BROWSER_SELFTEST_TRACE` records the exact model prompt, tool calls/results and explicit Done/Failed events only when the startup selftest is enabled. No test-only network listener or production chat-control tool is added.
- `scripts/probe_agent_memory_live.py` owns the app, config, sessions, browser cache, HTTP fixture and result directory for each run. It invokes current browser tools through the real app in preflight, then the actual chat/CLI agent in all three comparison arms. The oracle stores the native-input document separately from visible DOM text and checks the server-side saved title/paragraphs plus zero publish actions.
- macOS Unix sockets require short paths: disposable profiles use `/tmp/am-*`, not the much longer default macOS temporary directory. The first preflight reproduced `path must be shorter than SUN_LEN`; the corrected run uses the short root.

## Bugs reproduced by the harness

1. Offscreen selector center was dispatched to native input even when below the browser viewport. Naver-like preflight found correct title/body but zero save attempts. MCP and API resolvers now reject the offscreen center with an actionable scroll/recheck error before dispatch; a focused generated-JS regression covers it.
2. The initial iframe fixture bound an empty `about:blank` document before the iframe loaded, throwing before Save listeners were installed. Native down/up/click events proved the browser clicked correctly. Fixed the fixture initialization, then preflight passed with one save. This was a fixture defect, not a CEF activation defect.
3. Actual Codex chat revealed a storage split: the app injected note #1 from its isolated XDG directory, but Codex's MCP environment omitted XDG_CONFIG_HOME, so expand returned `memory entry not found` and seeded manual recall returned no match. These first comparison runs are invalid as memory-effect evidence. Explicitly propagate the config root in all CLI MCP definitions and re-run before interpreting timings. Traces showed no generated browser_memory_save calls into the unintended store.

4. A real model reported that a typed title looked empty in browser_snapshot despite a visible title and independent successful saves. The snapshot read only form-field `.value`, leaving contenteditable `value` empty. Reproduced in a focused failing JS regression, then added bounded editor text (and masked password fields) in MCP/API snapshots. Preserve the six first comparison runs as measurements of the same pre-fix tool version; later runs use the corrected snapshot and must be reported separately.

5. Final review found API snapshot visibility included opacity-zero elements but ref-click resolution excluded them. With a transparent button before an editor, snapshot e0 named Hidden while clicking e0 resolved Write here. A generated-JS regression reproduced that mismatch; the snapshot now uses the same opacity filter as click resolution and reports editor text. CLI comparison timings are unaffected by this API-only correction.

## Evidence contract

Keep prompts, mode/seed metadata, structured turn events, tool timings/results, independent final editor state and before/after PNG paths under an ignored e2e output directory. The harness must be runnable again with explicit output/profile/socket selection and must not touch the user's current Medium draft or persistent memory. Test-only app controls require an explicit opt-in and an isolated app instance. Stop and clean up only the processes/tabs created by the harness.

## Observed results — 2026-10-16

Model: Codex `gpt-6-luna`, effort `max`, Korean UI/task. Each condition has **one trial**; order was not randomized. Counts are outer tool calls/round trips, including a `browser_script` batch as one call. These observations support further use/testing of verified workflow hints, not a general latency guarantee.

### Controlled editor fixtures

All six valid runs used the same pre-snapshot-fix tool version and fresh isolated storage/session state. All saved the exact title and two nonempty paragraphs through trusted native keyboard input; the independent HTTP save oracle recorded zero Publish actions. The parent inspected representative after images and the oracle/turn traces.

| Fixture | Memory | Wall seconds | First input seconds | Tool calls | Tool errors |
| --- | --- | ---: | ---: | ---: | ---: |
| Rich editor | empty / tools | 130.312 | 29.232 | 16 | 0 |
| Rich editor | manual recall | 66.663 | 24.992 | 12 | 0 |
| Rich editor | automatic context | 55.192 | 15.938 | 11 | 0 |
| Iframe editor | empty / tools | 195.400 | 25.327 | 28 | 3 |
| Iframe editor | manual recall | 91.725 | 18.248 | 18 | 0 |
| Iframe editor | automatic context | 130.939 | 25.356 | 17 | 0 |

Memory reduced calls in both fixture pilots. The iframe tools-only errors were missing editable focus twice and one offscreen Save center. Injection was slower than manual recall in that case, so do not claim automatic injection is invariably the fastest route. The rich-editor tools-only agent also doubted an empty snapshot `value` despite successful visible input; that tool defect was fixed separately after preserving these measurements.

Evidence roots: `target/e2e-logs/agent-memory-rich-valid` and `target/e2e-logs/agent-memory-iframe-valid`. The earlier `*-run1` roots used the wrong MCP memory directory and are excluded.

### Actual authenticated sites

The harness copied the existing CEF profile to disposable app/cache/config directories. It submitted the task through the real chat send → CLI → MCP path and left only clearly labeled `[Arise QA]` drafts. The running user's tabs/profile files were not opened for writing by the test app. No Publish/발행 action was selected.

| Site | Condition | Agent seconds | Tool calls | Tool errors | Independent observation |
| --- | --- | ---: | ---: | ---: | --- |
| Medium | empty memory | 85.480 | 10 | 0 | Saved; new browser session reopened edit URL with title/two nonempty paragraphs, plus one unwanted empty P |
| Medium | observed workflow injected | 44.300 | 7 | 0 | Saved; reload retained title and exactly two P blocks, no empty P |
| Naver Blog | empty memory | 84.056 | 16 | 0 | Exact title/body visible; native Save changed draft count 0→1 and showed automatic-save time |
| Naver Blog | observed workflow injected | 58.804 | 12 | 0 | Exact title/body visible; saved count 1→2 and completion notice; earlier QA draft retained |

Medium tools-only used screenshot ratios for title/body and typed `\n\n` between paragraphs. A separate browser process confirmed persistence; the first verifier falsely missed the title because Medium uses NBSP. The verifier now normalizes NBSP for text comparison while retaining newlines and inspecting P blocks. The injected note described the observed workflow and recommended one Enter before checking the result. That run used native `browser_type` with `\n`, and a reload confirmed the formatting improvement. This is an observed pair, not an independently repeated causal estimate.

The real Naver editor was accessible. The first inspector incorrectly assumed top-document text was nonempty; its legacy write URL redirected into an iframe shell. The inspector now traverses accessible frames and captures failure state/images. The direct `/<blogID>/postwrite` editor showed a resume-existing-work dialog. For the new QA draft, the task explicitly required declining resume; the agent clicked Cancel, inspected current refs/images, typed title/body natively, and clicked Save. The injected Naver run used the same task but the remote state had changed: its initial draft count was 1 and the resume popup was absent. It observed this difference and left the earlier QA draft untouched, saving a second one. Therefore the Naver live timings are not a controlled A/B comparison. Naver server draft reload has not been independently verified; the table claims only the observed UI save state.

Real-site evidence roots: `agent-memory-medium-live-tools`, `agent-memory-medium-live-tools-persistence`, `agent-memory-medium-live-injected`, `agent-memory-naver-live-tools`, and `agent-memory-naver-live-injected` under `target/e2e-logs/`. Existing real drafts were not edited. The two Medium QA drafts remain private test artifacts; their edit URLs are in local result JSON files.

### A blocker memory cannot solve

With an injected verified workflow and the fixture save endpoint returning HTTP 503, the agent entered the exact native title/body, saw `Save failed`, tried Save once, then explicitly reported that the draft **was not saved**. There were two total save attempts, no whole-document retyping and no Publish. The parent read the final response and inspected the error-banner screenshot. Result: expected blocker, not a successful save (`agent-memory-save-error/medium-injected-0`, 93.428s agent time, 12 tool calls).

### Retained know-how and limits

`assets/medium_draft_memory.json` and `assets/naver_draft_memory.json` contain the reusable, credential-free tool payloads grounded in these observations. Their scope/evidence/date and verification limits are included; no absolute coordinates, snapshot refs, account identifiers or guessed editor-internal causes are retained. They were also saved and recalled through the real app's persistent browser memory as #1 (Medium) and #2 (Naver), separately from all benchmark stores. Automatic context starts using saved notes when the updated app runs; navigation to a different site within a turn still requires the agent's explicit recall.

Stale/adversarial notes were checked at the context-contract level (untrusted user-role hints, domain isolation, bounded layers), not in a separate real-model red-team trial. Only this short drafting task was measured; long articles, images, shopping checkout, authentication/CAPTCHA, API model runs, and Linux/Windows live behavior are not established by this evaluation.

## Validation and repeatable commands

- Focused test-first failures reproduced missing automatic context, offscreen click dispatch, and empty contenteditable snapshot values before their fixes. After fixes: browser-memory store/contract/context suites 18 + 7 + 4 passed; final context-budget rerun 4 passed.
- Final library suites: `agent_browser` 176 passed / 2 ignored; `chat` 690 passed / 3 ignored; `llm` 285 passed. Host library tests: 109 passed. Embedded MCP subprocess integration: 11 passed.
- Final app + MCP debug build succeeded. Python harnesses compile. `git diff --check` passed. Existing unrelated compiler warnings remain.
- The broader `cargo test -p browser_memory -p chat --lib --tests` did **not** complete: it reached `mcp_settings_ui` with 3/4 passing tests and stalled. Tracked separately as issue #196; no full-suite pass is claimed. The relevant focused suites above completed. The API opacity/ref regression failed before the fix (`Hidden` versus `Write here`), then passed in the 690-test chat library run.

On macOS with the development CEF bundle and a signed-in Codex CLI:

```sh
cargo build -p agent_browser --bin agent_browser --bin agent_browser_app
python3 scripts/probe_agent_memory_live.py --cases medium naver --modes tools manual injected --timeout 240 --out target/e2e-logs/memory-comparison-new
python3 scripts/probe_agent_memory_live.py --cases medium --modes injected --save-error --out target/e2e-logs/memory-save-failure-new
python3 scripts/probe_agent_site_live.py --url https://medium.com/new-story --profile-seed "$HOME/Library/Application Support/CEF/User Data" --run-chat --memory-note assets/medium_draft_memory.json --verify-reload --out target/e2e-logs/medium-memory-new
```

Live-site runs create real private QA drafts. Default `probe_agent_site_live.py` without `--run-chat` is inspection-only. Use a new output directory for each run; `--profile-seed` is explicitly opt-in. Trace files record the exact prompt and terminal event with mode 0600, and ordinary chat sessions do not write smoke traces. Local fixture comparisons never access external accounts. The live harness labels its verification scope explicitly: ordinary runs check terminal state and content; `--verify-reload` additionally requires the Medium draft to survive a new document load.

## Final review follow-up goal

Route: parent only, `chat/src/agent_tools.rs`. Output: API snapshot/click ref visibility parity. Input: an opacity-zero button preceding a visible editor; take a snapshot and resolve its first ref. Expected: the snapshot identifies the visible editor and the same ref resolves to that editor without dispatch. Independent parent check: run generated JavaScript in Node; demonstrate failure before changing the visibility filter, then pass the regression and chat library tests. This follows the completed tooling/state review and does not change the recorded CLI benchmark version.

## Review decisions

Keep native screenshot/click/type and accurate tool state as the foundation. Use short domain/task memory to choose a known workflow, then verify it against the current page. Preserve explicit save errors as blockers. Do not add global permissive DOM mutation, hardcode editor IDs, inject whole histories, or equate a visible paragraph/Saved label with independently proven server persistence. The new automatic recall switch is default-on but can be disabled in Settings > Chat; missing, locked or corrupt memory never blocks a user turn.
