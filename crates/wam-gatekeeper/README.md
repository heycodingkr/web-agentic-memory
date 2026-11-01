# wam-gatekeeper

The memory curator: a model decides, deterministic code disposes.

A candidate memory is judged by a model that can only act through ten tools (four reads, six decisions). A server-side layer enforces the invariants of [docs/spec/curator.md](../../docs/spec/curator.md) whatever the model says: scope, duplicates, versions, evidence, text screening, one decision per homogeneous scope batch. Jobs reach the curator through a queue with fairness, backpressure, retries, and a model budget ([docs/spec/queue.md](../../docs/spec/queue.md)). Memories are layered (L0 URL and keywords, L1 summary, L2 detail; [docs/spec/memory.md](../../docs/spec/memory.md)).

Status: the curator, worker, PostgreSQL memory store, and PostgreSQL queue adapter exist. The worker accepts either the in-memory or PostgreSQL queue backend. With a PostgreSQL queue configured, memory decisions require the current lease and attribution fence and commit the memory mutation and decision journal in one transaction. New contributions from one scope are reduced to one candidate per account and judged in one scope-level curator interaction; repeat submissions do not add independent evidence. `wam-server` integrates the curator/tool layer through its own durable `ApplicationRepository` worker: intake/consent/work are committed together and validated publication/attribution/score/outbox are atomic. Its application contribution work table is distinct from this library's standalone scope-batch queue; do not assume all standalone queue/budget features are active in that integration. See [server configuration](../wam-server/README.md) and [deployment](../../docs/operations/deployment.md). Submitted content stays data: nothing a contributor wrote is executed or published unreviewed.

## Layout

| Module | Role |
|---|---|
| `tools`, `record`, `text`, `error` | the ten tools, record validation, text screening, the nine error codes |
| `store`, `postgres_store` | `Store`, in-memory `MemStore`, and transactional PostgreSQL memory/version/decision/audit adapter |
| `curator` | the loop: instructions + tools + one provider = one scope-level decision; the rule pass for exact duplicates |
| `provider` | the model seam, `LlmProvider`: scripted fake, local Claude/Codex CLI, OpenAI-compatible Responses API |
| `queue`, `postgres_queue`, `budget`, `worker` | in-memory and PostgreSQL queue backends, model budget (token buckets, a daily ceiling, and provider quota pauses), versioned batch envelope, worker loop |
| `scenario`, `scenarios/` | labeled scenarios: seed memories, a job, a scripted answer, the expected decisions |

The instructions and tool schemas are single files under `docs/spec/curator/`, embedded at build time. Their hashes are written to every decision's audit record.

## Switching the model

The model is a configuration choice. Nothing else changes: instructions, tools, validation, queue, and budget are the same for every provider.

| Provider | For | Needs |
|---|---|---|
| `fake` | tests and deterministic scenario runs | nothing |
| `claude-cli` | local development | the installed `claude` CLI and your login (no API key) |
| `codex-cli` | local development | the installed `codex` CLI and your login (no API key) |
| `openai-compat` (`deepseek`, `muse`) | deployment | printable ASCII `MODEL_API_KEY`, `WAM_LLM_MODEL`; `WAM_LLM_BASE_URL` (defaults to `https://api.meta.ai/v1`); optional `WAM_LLM_REASONING_EFFORT`, `WAM_LLM_PRICE_*` |

Select with `--provider` / `WAM_LLM_PROVIDER` and `--model` / `WAM_LLM_MODEL`. The CLI and HTTP providers use the same JSON turn protocol (`{"tool": ..., "args": ...}`). The HTTP adapter calls `<WAM_LLM_BASE_URL>/responses`, sends `store:false`, bounds its response to 1 MiB, and accepts HTTPS URLs (loopback HTTP is reserved for local tests). It rejects API keys containing whitespace, control characters, or non-ASCII bytes before creating a request and never includes a key in configuration errors. Quota exhaustion pauses the worker and honors numeric or HTTP-date `Retry-After`. The HTTP contract is verified against a local fixture and, on 2026-11-01, live against the Meta Model API (see [Baseline with Meta Muse](#baseline-with-meta-muse-live)); the labeled CUR-15 evaluation is still outstanding.

The HTTP adapter tells three kinds of failure apart by status, never by what the provider wrote, and never repeats the provider's text: spent credit or quota (402, `insufficient_quota`, a spent-balance message; even on a 429) is `ProviderError::QuotaExhausted`; an outage (429 without a quota signal, 408, any 5xx, a connection that fails, times out or is cut off) is `ProviderError::Unavailable`, which carries `Retry-After`; anything else (400, 401, 403, 404, 413, 422, an unreadable 2xx reply) is an ordinary `Failed`. The curator reports them as `Outcome::QuotaExhausted`, `Outcome::Unavailable` and `Outcome::ProviderError`. The server worker waits out the first two without charging the job an attempt ([`docs/spec/provider-outage.md`](../../docs/spec/provider-outage.md)); the standalone worker engine still retries an outage like any failed attempt.

`WAM_LLM_REASONING_EFFORT` (optional: `minimal`, `low`, `medium`, `high`, `xhigh`, `max`) is sent as `reasoning.effort`; unset sends nothing. Muse Spark spends the request's fixed 2048-token output budget on hidden reasoning before it answers, so at its default effort a turn can end `status: "incomplete"` with no output text (reported as an incomplete reply, not a generic bad one) and costs more. An unknown value is refused at startup without being echoed.

`WAM_LLM_PRICE_INPUT_PER_M`, `WAM_LLM_PRICE_CACHED_INPUT_PER_M` and `WAM_LLM_PRICE_OUTPUT_PER_M` (US dollars per million tokens, all three or none) make the provider report what each reply cost (`meta.cost_usd`), computed from the reply's token counts; hidden reasoning tokens are billed as output. The daily ceilings count that figure (`ModelBudget` in the standalone worker, `WAM_LLM_DAILY_BUDGET_USD` in the server worker) and so does the cost line of `wam-curate`. Without prices nothing is reported and no ceiling can trip, so a worker that is given a ceiling refuses to start without them.

## Try it

```sh
cargo run -p wam-gatekeeper --bin wam-curate -- --list
cargo run -p wam-gatekeeper --bin wam-curate -- --provider fake
cargo run -p wam-gatekeeper --bin wam-curate -- --provider claude-cli --model sonnet --scenario all -v
WAM_LLM_PROVIDER=codex-cli cargo run -p wam-gatekeeper --bin wam-curate -- --scenario better
cargo run -p wam-gatekeeper --bin wam-curate -- --provider fake --scenario eval-60
```

Scenario runs switch the exact-duplicate rule pass off so that the model itself has to judge. The `eval-60` command runs the starter regression corpus with the deterministic fake. Selecting a real provider for it makes 60 model cases and spends tokens on your account.

## Tests

```sh
cargo test -p wam-gatekeeper                    # no network, no model
cargo test -p wam-gatekeeper --test provider -- --ignored # live Claude/Codex calls (your login, spends tokens)
WAM_TEST_DATABASE_URL='host=localhost dbname=wam_test user=wam_test' cargo test -p wam-gatekeeper --test postgres_queue -- --ignored
WAM_TEST_DATABASE_URL='host=localhost dbname=wam_test user=wam_test' cargo test -p wam-gatekeeper --test postgres_store -- --ignored
```

The tool layer, loop, memory queue, budget, and worker are covered with the scripted fake, including mixed-scope batch rejection before a provider call, account-level candidate deduplication, post-decision retry idempotency, random call sequences against the invariants (CUR-16), and random operation sequences against the queue. Ignored disposable-PostgreSQL tests cover durable memory decisions, queue restart, idempotent nonces, same-scope claim races, kind-separated batches, expired leases, stale generations, commit-time consent fencing, withdrawal, purge, and stats. Queue admission uses a transaction-scoped lock; worker claim uses a per-scope advisory lock plus `FOR UPDATE SKIP LOCKED`. The web server uses its own application repository and durable work adapter with the shared curator/tools; these standalone queue/store tests do not by themselves establish application or production acceptance. The test IDs are in [docs/test/functional-tests.md](../../docs/test/functional-tests.md) (`CUR-*`, `QUE-*`).

## Baseline with local CLIs

Five seed scenarios (duplicate, better, new, conflict, injection) on 2026-10-27, rule pass off. This is only a smoke sample; the same model can answer differently from one run to the next.

The checked-in starter corpus expands those five decision archetypes across twelve explicit URL/task contexts (60 cases total). It verifies the scenario runner, labels, scope substitutions, and safety invariants deterministically. The cases are template-derived, so they are regression coverage rather than 60 independent human annotations and do not satisfy CUR-15's final quality gate. The independent ≥60-job evaluation set and live Meta/Muse scoring remain outstanding.

| Provider | Matches expectation | Notes |
|---|---|---|
| fake (scripted) | 5/5 | deterministic |
| codex-cli (default model) | 5/5 | one run |
| claude-cli sonnet | 4/5 | one run; escalated `new` instead of creating |
| claude-cli haiku | 4/5 in the latest run | two runs: `duplicate` escalated both times, `better` once (about $0.008 for five scenarios) |

Escalating is the safe direction: nothing was created, replaced, or retired wrongly. The instructions lean toward escalation when unsure, so a cautious model escalates more. Do not retune the instructions on one sample; changes go through the evaluation of CUR-14.

## Baseline with Meta Muse (live)

Muse Spark through the Meta Model API (`muse`, standard tier, `store:false`, account on free credits), 2026-11-01, rule pass off. These are smoke samples, not the CUR-15 gate: the same model can answer differently from one run to the next.

| Model | `WAM_LLM_REASONING_EFFORT` | Seeds (5) | Notes |
|---|---|---|---|
| muse-spark-1.3 | unset | 5/5 (1 run) | 1 of 22 calls ended `incomplete`; reasoning up to 2,046 tokens in one call |
| muse-spark-1.3 | `low` | 5/5 (3 runs) | no incomplete reply in 62 calls; reasoning up to 977 tokens per call |
| muse-spark-1.3 | `minimal` | 5/5 (3 runs) | no incomplete reply in 61 calls; reasoning up to 304 tokens per call |
| muse-spark-1.2 | unset | 5/5 (1 run) | 3 unusable replies in 25 calls |
| muse-spark-1.1 | unset | 3/5 (1 run) | `better` and `injection` ended without a decision; 9 unusable replies in 29 calls |

`muse-spark-1.3` at `low` on the 60-case starter corpus (`eval-60`, one run): 58/60. `conflict` and `injection` are 12/12. The two misses (`new-blogger`, `duplicate-notion`) escalated to `needs_human_review` instead of deciding, the safe direction.

Estimated from the measured token counts at the console's list prices (Spark: $1.25 input, $0.15 cached input, $4.25 output, read as per million tokens), a job costs about $0.036 at the default effort and $0.022 at `low` or `minimal`; input is most of it (about 23,000 tokens per job). The `*-contributor` models are cheaper, but they use inputs and outputs to train Meta's models, so they are not used for contributed content. The console's Billing page also says that while an account is on free credits its content may be used for product improvement: these runs sent only the curator instructions, tool schemas and synthetic scenarios, and contributed content should go through a key on a paid account only (the paid-account terms were not checked). The console offers email alerts for monthly spend but no hard cap, so the cap is the worker's own daily ceiling, which needs the `WAM_LLM_PRICE_*` prices. `muse-wonder-1.0` returned `model_not_found` for the account used.

## Not done yet

- Full standalone scope-batch/coalescing/fairness/budget behavior in the application's separate work path must be assessed independently; shared curator/tool integration and atomic application intake/publication are implemented. Live worker/replay and withdrawal-after-publish reevaluation acceptance remain deployment work.
- The labeled CUR-15 evaluation set of at least 60 jobs and live Meta/Muse provider scoring against it.
- Production load, recovery and live-account evidence for the implemented PostgreSQL application adapters (identity, profiles, contributions, receipts and score events); local fixtures are not production proof.
