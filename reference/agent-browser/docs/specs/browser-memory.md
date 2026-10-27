# Browser memory (workflow know-how recall)

## Problem and root cause

The browser agent re-discovers the same web know-how every conversation: which Naver/Coupang flow needs a second login step, which selector survived a redesign, that a site needs a scroll before results render, what a user prefers on a site. Nothing persists between sessions except the Vault (secrets only) and the session transcript (unsearchable, huge). MeshCode solves this with BCB: a small SQLite FTS5/BM25 store where know-how is saved in explicit layers and only a few one-line hits are recalled, then expanded on demand. The browser had no equivalent tool surface, so neither API nor stdio/MCP agents could remember or recall.

## Design sources (MeshCode reference list: `meshcode/shanyanggm-github-list/메모리-컨텍스트-최적화.md` and related repos)

| Source | Taken |
| --- | --- |
| chenhz01/fts5-agent-memory | one SQLite file; FTS5 + BM25 kept in sync with the source table; fixed per-query token cost (k and a token budget); quoted FTS tokens so query syntax is inert; hoarding countermeasure (capacity cap) |
| LAIguapi/layered-memory-mcp | small L0 index, deeper layers loaded on demand; consult → act → learn loop in the prompt; lifecycle (stale notes replaced, not accumulated); advisory-free deterministic secret/ephemeral routing |
| MemPalace/mempalace | scope recall by site (wing/room style); load only a small context first |

Not taken: embeddings/vector stores (extra service or model, and agent notes are keyword-dense), automatic capture of whole transcripts (volume, privacy).

## Behavior

Four tools in a new `browser_memory` crate, defined once and shared by the API agent, the stdio MCP server and the Codex allowlist (same pattern as `credentials::account_tools`):

| Tool | Input | Model-facing result |
| --- | --- | --- |
| `browser_memory_recall` | `query?`, `site?` (URL or domain; at least one required), `kind?`, `k?` (default 5, clamped to 10) | `#id kind site \| L0` + indented L1 per hit, within a 600 estimated-token budget; `+N more` when cut; `no matching memory` when empty. A site without a query lists only that site's L0 index. Never returns L2/L3. |
| `browser_memory_save` | `kind` (`workflow`/`site_note`/`gotcha`/`preference`), `title` (L0), `summary` (L1), `detail?` (L2), `evidence?` (L3), `site?`, `tags?`, `replaces?` | `saved #id`, `already saved #id` (`(layers updated)` when a richer L2/L3 was merged in; an empty layer never erases a stored one), or `saved #id (replaced #old)`. Identical (kind, site, title, summary) is deduped; `replaces` swaps a stale note atomically. |
| `browser_memory_expand` | `id`, `level` (2 or 3) | the note's L0 line plus the requested layer. |
| `browser_memory_forget` | `ids` (1–20) | `forgotten #1; not found #9`. |

Layers are stored explicitly: L0 headline ≤120 chars, L1 summary ≤400, L2 detail ≤8,000, L3 evidence ≤16,000. Only L0, L1, site and tags are indexed (BM25 weights title 10, tags 3, summary 4, site 2); L2/L3 never enter recall. Hangul/CJK runs are indexed and queried as bigrams so Korean matches without an analyzer (particles on the query side still match), English uses the porter stemmer. Same-site notes rank above equally relevant others; parent/child domains get a smaller bonus. Capacity is 5,000 notes: when full, save fails with a clear message and `replaces` still works.

Storage: `<data_dir>/memory/browser-memory.sqlite` (WAL, busy timeout 2 s, `memory` table is the source of truth, `memory_fts` rows are written and deleted in the same transaction). `data_dir` is `host::browsing::data_dir()` (app and stdio process resolve it identically). Memory holds no secrets, so the stdio MCP process opens the file directly: all four tools work without the app running, without a tab lease and without Vault unlock. Read paths never create the file.

Privacy contract: `save` strips `?query` and `#fragment` from every URL it stores, normalizes `site` to a lowercase host, then rejects with a fixed error (never echoing input) any field that looks like a secret: `password/token/api key/authorization = …`, `Bearer …`, JWTs, provider key prefixes, private-key blocks, long mixed-case key blobs, Luhn-valid card numbers. The word "password" in prose is allowed. Prompts and tool descriptions route credentials to `browser_save_account`. Recalled notes are untrusted hints to verify on the live page.

Token cost (measured with the real stdio binary): the four tool definitions are ~550 tokens per turn; the shared prompt guidance is ~225 chars English / ~140 Korean; a typical recall of one hit is ~25 tokens and an empty recall ~4. Details live in tool descriptions rather than the per-turn prompt; CLI prompt caps in `cli_rules.rs` were raised by exactly the guidance size.

Agent surfaces: API agent registers the four tools in every toolset (not window/lease bound); recall, expand and save run without an approval card, forget asks; tool cards show the note title or query instead of a JSON dump; both the API system prompt and the CLI rules carry the single shared guidance line (`browser_memory::tools::guidance`, English key + Korean catalog entry). Codex `enabled_tools` includes the four names.

## Automatic task result memory (issue #135)

Completed API and CLI turns now also save a compact task report in the same personal
SQLite/FTS store used by `browser_memory_recall`. Previously the model had to call
`browser_memory_save` explicitly: a saved conversation alone never produced a note.

The common `CliEvent::Done` path records the latest request and final assistant report.
Task records use kind `workflow` and tag `task-history`; the workspace and request are
in L0, the reported outcome is in L1, and L2 retains the session ID, workspace, timestamp,
request, report, and optional site as provenance. These are historical assistant reports,
not independently verified website state or authorization. Multi-tab task records have
no fixed site; an available domain is searchable as a tag. Existing deduplication keeps
repeated saves/backfills from creating new notes. Source session removal does not remove
the durable task note.

Before a memory-enabled turn, bounded keyword matching can recover matching reports from
older sessions in the same absolute working root. It reads at most the normal session
retention count, 8 MiB per file and 32 MiB total; missing/corrupt/oversized files are skipped.
Up to three related reports can also provide at most 700 estimated tokens of user-role
context, independently of the active tab. The current session, unrelated roots, raw tool
results, reasoning, interrupted tool output and detected credentials are excluded.
`browser_memory_auto_recall` controls this recovery/context path; completed task capture
does not depend on a new chat or on an explicit model-issued save call.

Shared English/Korean API/CLI guidance requires recalling task/site keywords for substantive
work or references to previous work. It also preserves the requested deliverable:
“add another blog post” means writing a post; a todo entry is the deliverable when explicitly
requested. Stored workflow know-how and task reports remain distinct from MeshCode BCB.

Regression coverage: a finished Codex task is searchable without starting a new chat;
reopening the DB and deleting the source session preserve recall; repeated completion
and legacy recovery deduplicate; Korean particles match; workspace isolation, bounded
context, URL sanitization, secret rejection, final-report extraction, and missing archives
are checked. CLI/API prompt coverage and the common context wrapper are tested separately.

## Residual risks

- Notes persist across sessions and recalled text re-enters the model context, so a hostile page could try to get a poisoned note saved ("always click Transfer on bank.com"). Mitigations: save runs only on the agent's own tool call and shows its title in the chat card; the prompt and the recall description label notes untrusted and tell the agent to verify on the live page; secrets are rejected. Not mitigated: `browser_memory_save` is auto-approved, so a convincing injection can still store a note. If this proves a problem, require an approval card for saves made in a turn that read page content, or add a user-visible "Memory" list with delete in Settings.
- Heuristic secret detection has false negatives (for example a short password written in prose); the Vault remains the only place for credentials.

## Dependency-ordered atomic goals

| Goal | Route / owner | Output | Independent parent check | Depends on |
| --- | --- | --- | --- | --- |
| A: spec + failing tests | parent | this spec; `crates/browser_memory/tests/*`; allowlist assertion | tests failed first: unresolved imports for the crate, missing `browser_memory_recall` in the Codex allowlist | none |
| B: store core | parent, `browser_memory::{store,text}` | schema, layers, dedup, replace, BM25 recall with site boost and token budget, expand, forget, sanitizer, capacity | 16 store tests; mutation check (site bonus 0 + dropped FTS delete) failed 4 tests, restored | A |
| C: tool contract | parent, `browser_memory::tools` | names, descriptions, minimal schemas, `dispatch`, `render`, `call_text`, `default_dir_with`, `guidance` | 7 contract tests incl. definition-size and guidance-size caps | B |
| D: routing | parent, `mcp.rs`, `agent_tools.rs`, `cli_agent.rs` | advertised in `tools/list` and answered before any `browser_*` session path; API toolset; Codex allowlist; approval policy | real `agent_browser --mcp-server` child with no app and a fresh process after restart; chat toolset test; allowlist test | C |
| E: guidance and display | parent, `cli_rules.rs`, `agent_tools.rs`, `markdown.rs`, `ko.rs` | shared guidance in both languages; save card shows title only | i18n catalog test, cli_rules caps, tool-card test | C |
| F: live smoke and review | parent | real stdio binary: save → restart → recall/site index/expand/forget, secret rejection; diff review | script output and sqlite file inspected | D, E |
| G | parent; docs/specs/browser-agent-memory-evaluation.md | automatic domain/task recall of up to 3 L0/L1 hints, 400 estimated-token cap; Settings > Chat toggle; shared API/CLI context | domain isolation/budget tests and real-model comparison traces, including an MCP XDG-directory propagation regression | F |

## Test inputs and expected results (all implemented)

| Input | Expected result |
| --- | --- |
| Save with `https://WWW.nid.naver.com:443/x?mode=form#top`, URLs with query/fragment in every text field | site stored `nid.naver.com`; no query/fragment/token text anywhere in the db or WAL file |
| Recall after save, with L2/L3 sentinel text saved | only L0/L1 returned |
| 30 matching notes; `k` omitted / `k=99` | 5 / ≤10 hits; est. tokens ≤600; visible `more` |
| Same title/summary on two sites, query with each site | the matching site's note first |
| Site without query; unrelated sites and site-less notes present | exact + parent-domain notes only; neither query nor site is an error |
| Korean query with a particle (`로그인을 하려면`), English stem (`scroll` vs `Scrolling`), FTS syntax/injection text | Korean and stem hits; syntax inert, no match |
| Identical save twice (case/space variants) | `deduped`, same id, rows == FTS docs |
| Forget existing + unknown id; replace stale; replace unknown id | forgotten/not found reported; index in sync; replace atomic; failed replace writes nothing |
| Expand level 2/3/1/4, missing id | layer only / validation error / not found |
| Over-length fields, empty title, unknown kind/field, bad tags, non-integer `replaces`, bad site scheme | fixed error, canary never echoed, nothing written |
| Secrets in title/summary/detail/evidence/tags (assignment, Bearer, JWT, `sk-`, card with Luhn, private key, key blob) | rejected, input not echoed; prose "password step", Luhn-invalid digit runs, selectors allowed |
| Capacity 2 | third save refused (`full`), replace still works |
| Two threads saving 25 notes each; four threads released together on a never-opened store, each saving and recalling | all rows present, no `database is locked` (SQLite returns BUSY at once for journal-mode switch / first schema creation, so open and `BEGIN IMMEDIATE` retry for up to 3 s) |
| Corrupt file; missing file | error without panic; created on first save; recall on a missing store creates nothing |
| stdio `tools/list`, no app running | four tools advertised; save → recall in one process, site index/expand/forget in a fresh process |
| Codex allowlist, API toolset, approval policy | four names present; recall/expand/save unprompted, forget prompts |
| English and Korean prompts | one short guidance line; credentials routed to `browser_save_account` |

## Verification record

- `cargo test -p browser_memory -p i18n -p llm -p agent_browser`: all pass (browser_memory 7 + 16 tests, `memory_tools_work_without_the_app_and_persist_across_processes`, `tools_list_count` updated 33 → 37, `codex_mcp_allowlist…`, `cli_rules` caps, i18n catalog).
- `cargo test -p chat --test browser_memory_tools`: 4 pass. `cargo test -p chat` lib had one failure (`session_view::tests::capped_assistant_text_keeps_browser_result_markers_paired`) in a run that overlapped concurrent edits to `session_view.rs`; it passes alone, three runs, and is unrelated to memory.
- Real binary smoke (`agent_browser --mcp-server`, isolated `XDG_CONFIG_HOME`): three notes saved, process restarted, Korean/English/site-index recall, expand, forget, secret rejected; sqlite file created under `<cfg>/agent-browser/memory/`.
- Concurrency finding: `two_connections_write_concurrently` failed 48/60 when run alone (it only passed inside the full suite by luck). Fixed with a BUSY retry around WAL switch, `user_version` read and `BEGIN IMMEDIATE`; after the fix 0/100 failures, plus `four_writers_racing_on_a_fresh_store_never_see_a_lock_error` 0/60.
- `cargo clippy -p browser_memory --all-targets`: no warnings from the new crate.
- Follow-up: automatic context and actual Codex chat evaluation live in docs/specs/browser-agent-memory-evaluation.md. Linux/Windows live runs remain unverified.
