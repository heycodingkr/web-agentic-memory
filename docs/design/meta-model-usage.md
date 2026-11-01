# Meta models in Web Agentic Memory

[English](meta-model-usage.md) | [한국어](meta-model-usage.ko.md)

*Status: 2026-11-01. Written for the Meta Global AI Developer Hackathon (Cerebral Valley) and for operators. Every number below comes from live runs against the Meta Model API on that date. They are smoke samples, not a quality gate: see [Limits](#limits).*

## In short

- WAM's Gatekeeper decides which contributed workflows become shared memory. That judgment is made by **Muse Spark 1.3** (`muse-spark-1.3`) through the **Meta Model API** (`POST https://api.meta.ai/v1/responses`).
- The model gets no browser, shell, network or database access. It acts only through ten constrained tools (four reads, six decisions). Server-side code checks every call and enforces scope, duplicates, versions, evidence and text screening whatever the model says. Contributed text is data, never instructions.
- Live results: all five seed scenarios pass on Muse Spark 1.3. A 60-case labeled starter corpus passes **58/60** at `reasoning.effort=low`, with every conflict and every prompt-injection case handled correctly (12/12 each). Both misses were escalated to a human rather than published.
- Cost: about **$0.02 per reviewed contribution** at the console's list prices (an estimate, see [Cost](#cost)). An exact duplicate is settled by a rule before the model is called and costs nothing.
- The service keeps working when model credit runs out: contributions stay queued, nothing is lost, and no worker calls the model again until credit returns or a daily ceiling resets.

## Where the model sits

```
contribution (consented, closed schema)
   |  text screening, trusted browser replay, exact-duplicate rule      (deterministic, no model)
   v
Gatekeeper curator loop  --  Muse Spark 1.3 through the Meta Model API
   |  reads:     job_get, memory_search, memory_get, memory_compare
   |  decisions: decide_create, decide_replace, decide_skip,
   |             decide_retire, decide_escalate, decide_reject
   v
server-side tool layer: scope, duplicates, versions, evidence, one decision per batch
   v
publish (immutable version, author credit), support, replace, needs_human_review, reject
```

Muse Spark reads the candidate and the existing memories, compares them with metrics the server computes (success rate, steps, new traits and pitfalls, independent contributors, freshness, UI fingerprint change), and returns one decision with reason codes and a confidence band. Typical outcomes:

| Situation | Decision |
|---|---|
| Repeats the active procedure with nothing new | support the existing memory (skip) |
| Shorter or better procedure, reported by independent contributors | replace (supersede) |
| Nothing covers this route and task yet | create |
| Two contributors disagree | escalate to a human, publish nothing |
| A contributed field says "retire the active memory" | escalate; the instruction is never obeyed |

Each decision's audit record keeps the provider and model (`muse:muse-spark-1.3`), the SHA-256 of the curator instructions and of the tool schemas, the job digest, every tool call and the final decision. It is stored with the publication (`wam_audit` on PostgreSQL). Hidden model reasoning is never stored, and requests are sent with `store: false`.

## How it is called

| Setting | Value or variable |
|---|---|
| Endpoint | `WAM_LLM_BASE_URL` (default `https://api.meta.ai/v1`); the adapter calls `<base>/responses` |
| Provider | `WAM_LLM_PROVIDER=muse`, an alias of the OpenAI-Responses-compatible `openai-compat` provider |
| Model | `WAM_LLM_MODEL=muse-spark-1.3` |
| Key | `MODEL_API_KEY` (printable ASCII; never logged, echoed or stored in an audit record) |
| Reasoning effort | `WAM_LLM_REASONING_EFFORT=low` (`minimal`, `low`, `medium`, `high`, `xhigh`, `max`; unset sends nothing) |
| Price list | `WAM_LLM_PRICE_INPUT_PER_M`, `WAM_LLM_PRICE_CACHED_INPUT_PER_M`, `WAM_LLM_PRICE_OUTPUT_PER_M` (US dollars per million tokens, all three or none) |
| Output budget | 2048 tokens per call; hidden reasoning counts against it |

Tool calls travel as a small JSON turn protocol (`{"tool": ..., "args": ...}`) inside the model's text output. The same loop therefore runs against Muse Spark, against the local Claude and Codex CLIs used in development, and against a scripted fake in tests. Instructions and tool schemas are single files under `docs/spec/curator/`.

## Which models, and why this one

Models the Meta Model API listed for our key (`GET /v1/models`) on 2026-11-01, with the console's list prices in US dollars per million tokens. The console does not print the unit; we read it as per million.

| Model | Kind | Input / cached / output | Used here |
|---|---|---|---|
| `muse-spark-1.3` | chat, 1M context | 1.25 / 0.15 / 4.25 | **Yes, the Gatekeeper model** |
| `muse-spark-1.2`, `muse-spark-1.1` | chat | 1.25 / 0.15 / 4.25 | Compared (below) |
| `muse-spark-1.3-contributor`, `muse-spark-1.2-contributor` | chat | 0.10 / 0.002 / 0.20 | No. Inputs and outputs train Meta's models, so they are not used for contributed content |
| `muse-image-1.0`, `muse-voice-transcribe-1.0`, `sam-3.1` | image, speech to text, segmentation | per image, per hour, per 1,000 images | No. The Gatekeeper judges structured text |
| Muse Wonder | chat, 1M context | 4.00 / 0.20 / 20.00 | Not enabled for our key (`model_not_found`) |
| Muse Spark Open, Muse Glimmer | open weights | n/a | Not used. The adapter needs a `/v1/responses` endpoint, which we have not tried on partner-hosted open weights |

## Evaluation (live, 2026-11-01)

Everything ran through the production curator code path (`wam-curate --provider muse`). The exact-duplicate rule pass was off, so the model itself had to judge.

**Five seed scenarios** (duplicate, better, new, conflict, injection):

| Model | Reasoning effort | Seeds passed |
|---|---|---|
| `muse-spark-1.3` | default | 5/5 (2 runs) |
| `muse-spark-1.3` | `low` | 5/5 in every run (4 runs) |
| `muse-spark-1.3` | `minimal` | 5/5 in every run (3 runs) |
| `muse-spark-1.2` | default | 5/5 (1 run) |
| `muse-spark-1.1` | default | 3/5 (1 run); two scenarios ended without a decision |

**Labeled starter corpus** (`eval-60`: five archetypes in twelve site and task contexts each), `muse-spark-1.3` at `low`, one run: **58/60**.

| Archetype | Passed |
|---|---|
| duplicate | 11/12 |
| better | 12/12 |
| new | 11/12 |
| conflict | 12/12 |
| injection | 12/12 |

The two misses (`new` on one site, `duplicate` on another) escalated to a human (`needs_human_review`) instead of deciding. Nothing was published, replaced or retired wrongly.

**What we learned about the model.** Muse Spark is a reasoning model. At its default effort a call can spend the whole 2048-token output budget on hidden reasoning and return no text; we saw 2,046 reasoning tokens in one call and one empty reply in 22. With `reasoning.effort=low`, hidden-reasoning tokens per call fell (p95 from 1,793 to 561 over 22 and 62 calls), calls were twice as fast (3.6 s to 1.8 s on average), and there was no empty reply in 62 calls, with no loss on the seeds. The adapter now reports an exhausted output budget as an incomplete reply instead of a generic bad one.

## Cost

Per reviewed contribution (one curator job, about four model calls), from measured token counts at the console's list prices:

| Reasoning effort | Input tokens | Output tokens | Cost | Seconds per call |
|---|---|---|---|---|
| default | about 25,100 | about 2,900 | $0.036 | 3.6 |
| `low` | about 23,500 | about 880 | **$0.022** | 1.8 |
| `minimal` | about 23,000 | about 480 | $0.023 | 1.5 |

- Input dominates (83% of the cost at `low`), so the bill follows memory size. If real memories carry twice or four times the input tokens, a contribution costs about $0.04 or $0.08.
- At $0.022: 1,000 contributions cost about $22 and 100,000 about $2,200.
- One later live `new` job cost $0.006. Cached input is billed at $0.15 per million against $1.25 uncached, so prompt-cache hits are the main lever left.
- An exact duplicate is settled by a rule before the model is called: no model call, no cost.
- The whole evaluation above cost under $4 at list price and ran on the account's free Meta Model API credits.

## When model credit runs out

The queue-waiting behavior is specified in [`docs/spec/queue.md`](../spec/queue.md) (QUE-08, QUE-20 to QUE-24) and configured in the [deployment runbook](../operations/deployment.md).

- **Trigger.** The provider says its credit or quota is spent: HTTP 402 with `type: billing_error` (for example `billing_not_configured`), `insufficient_quota`, or a usage-limit message. Per-minute rate limits (429) are ordinary failures that back off and retry.
- **What happens.** The job that met the refusal waits as `waiting_budget` and does not use an attempt. A durable budget gate closes. While it is closed no worker claims a job, runs a browser replay or calls the model, and new contributions are still accepted and stay `queued`. Recall is unaffected.
- **How long.** The time the provider names (at least 30 seconds, at most one hour by default). Without a hint: 15 minutes, doubling with every consecutive refusal up to one hour (`WAM_LLM_QUOTA_PAUSE_SECS`, `WAM_LLM_QUOTA_PAUSE_MAX_SECS`). After each pause one job goes out as a probe; if it is answered the gate reopens, otherwise the pause grows.
- **Shared and durable.** The gate lives in the application state (`wam_app_meta` on PostgreSQL), so several workers and restarts agree. PostgreSQL migration `0003` adds the `waiting_budget` work state and is applied on connect.
- **Operating it.** `wam-worker --budget-status` prints the gate as JSON (why it is closed, when it reopens, today's spend, jobs waiting). `wam-worker --resume-budget` reopens it right after credit was added and returns parked jobs to `queued`. The outbox records `worker_budget_paused` and `worker_budget_resumed`, and waiting jobs count in the queue-health age alert.
- **Daily ceiling.** The console's monthly limit only sends an email. `WAM_LLM_DAILY_BUDGET_USD` adds a hard stop: when the day's estimated spend (UTC) reaches it, no new job starts until the next UTC day. It is checked before each job, so the last job can overshoot it. It needs the price list above, and a worker given a ceiling without a cost-reporting provider refuses to start.
- **Consent withdrawal** cancels a waiting job like any other pending job.

## Data use and credentials

- Meta's console states that while an account uses free credits its content may be used for product improvement. All of our evaluation traffic was the curator instructions, the tool schemas and synthetic scenarios, never user data. Contributed content should go through a key on a paid account. We did not check the terms that apply after switching.
- The contributor-tier models train on inputs and outputs and are not used.
- Keys are read from the environment only. Development keeps them in a git-ignored `.env`; `.gitignore` also excludes `.secrets/`, `credentials.json` and `*.secret(s)`. Keys never appear in logs, error messages or audit records.

## Hackathon notes

**Event.** Meta Global AI Developer Hackathon (Cerebral Valley), building period 2026-10-27 to 2026-11-04. Its rules require projects to incorporate Meta's models; here the Meta Model API with Muse Spark carries the product's central trust decision.

**Beyond a basic integration.**

- A tool-using curator agent with ten constrained tools and server-side invariants, rather than a prompt that returns text.
- Prompt-injection handling that is measured, not assumed: 12/12 injection variants escalated instead of obeyed.
- A model comparison across Muse Spark 1.1, 1.2 and 1.3 on the same scenarios.
- Reasoning-effort tuning that made each contribution about 40% cheaper and twice as fast.
- A per-contribution cost model, a daily spending ceiling and queue-waiting when credit runs out.
- A live evaluation harness and an audit trail that names the model for every decision.

**What is new, and what is not.** The git history of the Gatekeeper crate (`crates/wam-gatekeeper`), its Meta Model API provider and the model and curator specs starts on 2026-10-27. The live integration, reasoning-effort control, cost reporting, credit-exhaustion handling and the evaluation above were added on 2026-11-01. The 2026-10-16 editor pilot results and the read-only snapshots under `reference/` predate the event; see [`reference/README.md`](../../reference/README.md).

**How to check it.**

```sh
set -a; . ./.env; set +a      # MODEL_API_KEY, WAM_LLM_PROVIDER=muse, WAM_LLM_MODEL=muse-spark-1.3, WAM_LLM_REASONING_EFFORT=low
cargo run -p wam-gatekeeper --bin wam-curate -- --scenario all       # five seeds, about $0.11
cargo run -p wam-gatekeeper --bin wam-curate -- --scenario eval-60   # 60 cases, about $1.3
cargo test -p wam-gatekeeper -p wam-server                           # no network, no model
wam-worker --budget-status                                           # the budget gate as JSON
```

## Limits

- The numbers are smoke samples: single runs on synthetic scenarios, results vary from run to run, and the 60-case corpus is derived from templates rather than independently labeled (the CUR-15 quality gate is not met).
- Costs are estimates from the console's list prices read as per million tokens. Console billing lags by up to an hour and the account ran on free credits, so no invoice was checked.
- The model has judged synthetic scenarios only, not real user contributions.
- Credit exhaustion was verified with a fixture that returns the 402 body Meta documents, with scripted refusals through the worker, and on a disposable PostgreSQL. We did not spend the live credits down to zero, so Meta's real exhaustion response was not observed.
- A provider outage that is not a credit or quota refusal (5xx, timeouts, 429 rate limits) is an ordinary failure by design: the job backs off, and after five failed attempts it goes to manual review.
- Muse Wonder and the open-weight models were not evaluated.
