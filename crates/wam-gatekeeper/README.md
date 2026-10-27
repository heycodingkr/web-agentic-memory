# wam-gatekeeper

The memory curator: a model decides, deterministic code disposes.

A candidate memory is judged by a model that can only act through ten tools (four reads, six decisions). A server-side layer enforces the invariants of [docs/spec/curator.md](../../docs/spec/curator.md) whatever the model says: scope, duplicates, versions, evidence, text screening, one decision per job. Jobs reach the curator through a queue with fairness, backpressure, retries, and a model budget ([docs/spec/queue.md](../../docs/spec/queue.md)). Memories are layered (L0 URL and keywords, L1 summary, L2 detail; [docs/spec/memory.md](../../docs/spec/memory.md)).

Status: in memory only. It is not wired into wam-server or Postgres yet; contribution intake, transactional publication, and score attribution remain separate gaps. Submitted content stays data: nothing a contributor wrote is executed or published unreviewed.

## Layout

| Module | Role |
|---|---|
| `tools`, `record`, `text`, `error` | the ten tools, record validation, text screening, the eight error codes |
| `store` | the `Store` trait and the in-memory `MemStore` (a Postgres adapter implements the same trait later) |
| `curator` | the loop: instructions + tools + one provider = one decision per job; the rule pass for exact duplicates |
| `provider` | the model seam, `LlmProvider`: scripted fake, local Claude/Codex CLI, HTTP API (attached last) |
| `queue`, `budget`, `worker` | job queue, model budget (token buckets, a daily ceiling, and a pause when the provider says its quota is spent), the worker loop |
| `scenario`, `scenarios/` | labeled scenarios: seed memories, a job, a scripted answer, the expected decisions |

The instructions and tool schemas are single files under `docs/spec/curator/`, embedded at build time. Their hashes are written to every decision's audit record.

## Switching the model

The model is a configuration choice. Nothing else changes: instructions, tools, validation, queue, and budget are the same for every provider.

| Provider | For | Needs |
|---|---|---|
| `fake` | tests and deterministic scenario runs | nothing |
| `claude-cli` | local development | the installed `claude` CLI and your login (no API key) |
| `codex-cli` | local development | the installed `codex` CLI and your login (no API key) |
| `openai-compat` (`deepseek`, `muse`) | deployment | not built yet; will read `WAM_LLM_BASE_URL` and `MODEL_API_KEY` |

Select with `--provider` / `WAM_LLM_PROVIDER` and `--model` / `WAM_LLM_MODEL`. The CLI providers run in an empty temporary directory with no tools and no session; tool calls travel as a JSON turn protocol (`{"tool": ..., "args": ...}`), the fallback [curator.md](../../docs/spec/curator.md) describes for models without native tool calling. Attaching DeepSeek or Meta Muse means one new `LlmProvider` implementation and a line in `build_provider`.

## Try it

```sh
cargo run -p wam-gatekeeper --bin wam-curate -- --list
cargo run -p wam-gatekeeper --bin wam-curate -- --provider fake
cargo run -p wam-gatekeeper --bin wam-curate -- --provider claude-cli --model sonnet --scenario all -v
WAM_LLM_PROVIDER=codex-cli cargo run -p wam-gatekeeper --bin wam-curate -- --scenario better
```

Scenario runs switch the exact-duplicate rule pass off so that the model itself has to judge. A run with a real model spends tokens on your account (a few cents for all five scenarios).

## Tests

```sh
cargo test -p wam-gatekeeper                    # no network, no model
cargo test -p wam-gatekeeper -- --ignored       # live Claude and Codex CLI calls (your login, spends tokens)
```

The tool layer, the loop, the queue, the budget, and the worker are covered with the scripted fake, including random call sequences against the invariants (CUR-16) and random operation sequences against the queue. The test IDs are in [docs/test/functional-tests.md](../../docs/test/functional-tests.md) (`CUR-*`, `QUE-*`).

## Baseline with local CLIs

Five scenarios (duplicate, better, new, conflict, injection) on 2026-10-27, rule pass off. A first look, not an evaluation: the labeled set of CUR-15 has at least 60 jobs, and the same model can answer differently from one run to the next.

| Provider | Matches expectation | Notes |
|---|---|---|
| fake (scripted) | 5/5 | deterministic |
| codex-cli (default model) | 5/5 | one run |
| claude-cli sonnet | 4/5 | one run; escalated `new` instead of creating |
| claude-cli haiku | 4/5 in the latest run | two runs: `duplicate` escalated both times, `better` once (about $0.008 for five scenarios) |

Escalating is the safe direction: nothing was created, replaced, or retired wrongly. The instructions lean toward escalation when unsure, so a cautious model escalates more. Do not retune the instructions on one sample; changes go through the evaluation of CUR-14.

## Not done yet

- The HTTP provider for DeepSeek and Meta Muse (last, by design). Whether Muse supports native tool calling is unverified; the JSON turn protocol works without it. It must map its out-of-credit answers (HTTP 402, `insufficient_quota`) to `ProviderError::QuotaExhausted`, with `Retry-After` when there is one, so that the queue parks jobs instead of retrying them.
- A multi-candidate job format, so that a whole queue batch is judged in one model call. Today a batch shares one scope lease and is judged job by job.
- The Postgres store and queue adapters, and the commit-time lease and consent checks of docs/spec/database.md.
