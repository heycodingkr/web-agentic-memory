# Benchmark execution

`tests/lib/wamtest/bench/runner.py` connects the crossover schedule, action guard, trace metrics, invalid-run classifier and statistics. Every planned run executes once in a fresh `Isolation`; failures and invalid runs stay in the results.

Generate a plan before running:

```sh
PYTHONPATH=tests/lib python3 -B - <<'PY'
from wamtest.bench.schedule import make_plan, write_plan
write_plan(make_plan(["rich-editor"], ["T", "M", "I"], repeats=5, seed=3), "plan.json")
PY
```

Before a live experiment, fill and commit [the preregistration template](../../docs/test/prereg/_template.md). The runner rejects unbalanced plans, undeclared task/block rows, duplicate IDs, missing T, and a fixed condition order. It saves the normalized execution order and plan SHA-256 before the first adapter call.

## Live adapter contract

Call `run_plan(plan, adapter, output, metadata=...)`. The adapter receives `(context, spec)`, uses `context.isolation` for all host configuration, and returns a boolean obtained from an independent oracle. It must emit its own `done` or `failed` event. RunDir finalization does not turn a silent host into a completed run.

Use `context.perform(name, callback, url=..., label=...)` before executing an action. The callback runs only after the guard accepts it; a denied attempt records `blocked-action` and is a real task failure. The default host allow list is loopback. This hook is not a process sandbox: host adapters still need browser/network interception and must stop and clean up every process they create.

Use `context.event` for normalized tool calls/results, submit, and completion. Times are finite, nonnegative, monotonic seconds; errors and terminal ordering are validated. Use `context.signal` for documented environment signals such as `rate-limited` or `service-5xx`. Unknown signals are harness errors.

Controlled comparisons require explicit `controlled=True` plus common `host`, `model`, `effort`, `prompt_hash`, and `tools_hash` metadata. The default is uncontrolled (grade C). The native adapters below reject controlled mode: their process/trace integration is implemented, while real model/browser interception and behavior acceptance remain under issue #10.

## Native CLI and generic MCP hosts

`wamtest.bench.host.CliHostAdapter` launches `codex exec --json`, `claude --print --output-format stream-json`, or an explicitly supplied generic MCP driver. Each run receives fresh HOME/XDG/TMPDIR, CODEX_HOME and CLAUDE_CONFIG_DIR directories and a new working directory. It never copies the user's settings or credentials. Model credentials must be passed explicitly through the adapter's `env` map or CLI `--pass-env NAME`; values stay out of command-line arguments and evidence.

Native built-in command/delegation tools are disabled. A task explicitly lists its permitted MCP tools; Codex exposes those names and preapproves each listed tool, while Claude's noninteractive allow rules name the same tools. This is task-specific authorization, not browser/network interception. A task factory must separately arrange its own browser policy and cleanup.

The preparation callback is a context-manager factory `prepare(context, spec)` that yields `HostRun`:

| Field | Contract |
|---|---|
| `prompt` | Nonempty UTF-8 text, at most 64 KiB; passed through a private stdin file |
| `oracle` | A callable returning a bool from an independently observed side effect; the model's answer cannot set this verdict |
| `mcp_servers` | Server-name mapping with absolute stdio `command`/`args`/`env`, or HTTP `url`/`headers`; saved only in the disposable profile |
| `allowed_tools` | Explicit server-name to tool-name lists, for example `{"fixture": ["save"]}`; no wildcard grants |
| `observe` | Optional callback receiving only normalized tool events; an independent fixture observer can emit a real `submit` event here |

The context manager cleans up task resources even if the CLI fails. The adapter stops its own process group on completion, timeout or malformed output; the installed-host acceptance also checks that its MCP child exited. Wall timeout, combined stdout/stderr size and individual JSONL line size are bounded. Do not attach a task factory to a user's existing browser/profile or oracle state from an earlier run.

Example invocation after defining `my_tasks.prepare` and freezing the plan:

```sh
PYTHONPATH=tests/lib:. python3 -B tests/bench/run_host.py \
  --host codex --model YOUR_EXPLICIT_MODEL --factory my_tasks:prepare \
  --plan plan.json --output target/e2e-logs/bench/native-run \
  --pass-env CODEX_API_KEY
```

For Claude, select `--host claude-code`, an explicit model, and the appropriate explicitly supplied credential variable. `--max-budget-usd` forwards Claude's native budget option. `--timeout` bounds either host's wall time. All live invocations can use model quota; the command above is not part of the automatic self-tests.

Normalized evidence contains tool names, local call IDs, receive times, booleans and closed failure signals. It drops native session IDs, prompt text, tool arguments/results and model text. Raw native stdout/stderr is discarded by default; `--retain-raw` explicitly retains bounded files with mode `0600`. These files can contain private task data. Per-run metadata records prompt/config hashes, host/model/effort, byte counts, stream hashes and exit status.

Host completion alone cannot pass a run: the child must exit successfully, complete a valid trace, and pass the independent oracle. Failed MCP initialization is unavailable; missing terminal events and malformed streams are harness failures. Structured authentication/quota/service rejection stops a known-invalid attempt before the CLI's retry backoff turns it into a generic timeout. Error messages are interpreted only at CLI error boundaries, never in model text or tool output. Codex metadata warnings represented as error items do not abort a later successful turn. A native trace never invents `submit` from a tool name or "saved" in the model's text; without an independent submit observation, TTS remains null.

The generic driver receives the prompt on stdin and the path to private MCP JSON in `WAM_HOST_MCP_CONFIG`. Use `--host generic-mcp --executable /absolute/driver --host-arg ARG`. Its stdout is JSONL with versioned MCP boundary envelopes:

```json
{"schema_version":1,"type":"mcp","direction":"client","message":{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"save","arguments":{}}}}
{"schema_version":1,"type":"mcp","direction":"server","message":{"jsonrpc":"2.0","id":1,"result":{"content":[],"isError":false}}}
{"schema_version":1,"type":"host.completed","success":true}
```

The driver must emit its explicit host terminal event; an MCP server's `initialize` or `tools/list` response is not task completion. The supplied driver owns its model and tool execution policy. This interface does not infer an arbitrary host's transcript format or sandbox that external program.

### Installed-host acceptance without model charges

```sh
python3 -B tests/env/check_host_traces.py
python3 -B tests/env/check_host_traces.py --scenario auth
```

This runs the installed Codex and Claude executables against local synthetic Responses/Messages APIs and one owned stdio MCP. The MCP can only save fixed synthetic text, and a separate file oracle verifies the effect. Native Codex 0.165.0 and Claude Code 2.1.312 passed on macOS (2026-10-29), with two loopback model requests per success case, one per 401 case, verified child cleanup, and zero real model/account calls. Evidence: `target/e2e-logs/host-native/20261029-132106-f461/` (success) and `20261029-132536-d965/` (401). The script retains its synthetic-only native streams as private artifacts. This establishes native schema/config/process compatibility; real model/browser performance and live account acceptance remain separate.

Protocol references: [official OpenAI noninteractive mode](https://developers.openai.com/codex/noninteractive), [Codex MCP configuration](https://developers.openai.com/codex/mcp), and [Claude CLI reference](https://code.claude.com/docs/en/cli-reference). Installed-host acceptance determines compatibility with the local versions above.

## Native search profile

```sh
python3 -B tests/bench/profile_search.py
# A smaller run, still five warm repetitions of ten samples for each of seven cases:
python3 -B tests/bench/profile_search.py --sizes 1000 10000
```

The runner builds `release_search_profile` with the locked optimized profile, creates a new native PostgreSQL cluster, and seeds 1k/10k/100k immutable synthetic versions (12.5 index rows/version). It needs PostgreSQL native tools and Cargo; it never takes a supplied service URL. Each size starts with a fresh database and is removed afterward. A fixed seed and hash chain identify the actual corpus. `environment.json` records source/migration/lockfile hashes, probe hash, toolchain, CPU architecture/count and relevant DB settings.

Seven fixed classes cover exact/prefix routes, a second task, three no-match reasons and a task-unfiltered host. Every observation checks full ID/version/score parity between exhaustive core, actual repository candidates and real loopback HTTP. Positive controls must hit and negative controls must be empty. Reports retain raw samples, p50/p95/p99, five-trial variation, first observation, EXPLAIN (ANALYZE, BUFFERS), corpus/database sizes, error count and sampled peak process RSS. Missing trials/samples/plans, malformed/nonfinite measurements, and percentiles inconsistent with the samples fail validation. The core measurement includes the preloaded exhaustive oracle and document projection; the repository measurement includes query dispatch and decoding. RSS includes those oracle records and the embedded API, excludes PostgreSQL, and is not a standalone production API memory measurement.

Evidence is `target/e2e-logs/search-profile/<run>/`. DB p95 100ms and HTTP p95 500ms are reported separately from correctness; a missed latency goal stays visible. Admission uses an explicit in-memory x100 fixture quota, concurrency is one, the keyword mix is synthetic English, and the case order is fixed. OS cache is not flushed. The serial service-rate estimate is not concurrent throughput. These runs do not complete the dedicated-runner 100 RPS/5-minute workload, broad keyword/kind distribution, human relevance labels, extreme host skew or production topology gates.

The completed 2026-10-30 local run `20261030-032442-429e` retained full parity/error0 at all three sizes, but 100k HTTP worst-trial p95 was 553.9ms (500ms goal missed, issue #97); DB candidates were 15.8ms. An earlier 100k run measured HTTP 415.2ms. Preserve both instead of selecting the faster result. All three corpus hashes matched previous executions. The RSS monitor failure regression was then reproduced and fixed: losing measurement after an earlier valid sample now fails. Actual 1k execution after that correction passed in `20261030-032805-56b5`.

The default RSS stop budget is 4096 MiB and the wall limit is 1200 seconds/size. The initial 2048 MiB attempt is retained as a failure; a completed 100k run measured about 2046 MiB including the exhaustive oracle. `--max-rss-mib` changes the declared resource stop budget, not a release latency threshold. The 1M stress profile (`--sizes 1000000`) must be a separate suitably provisioned run; this 16 GiB development machine has not completed it. All failed attempts remain in evidence.

## Offline replay

```sh
python3 -B tests/bench/run.py --plan plan.json --traces traces.json \
  --output target/e2e-logs/bench/offline-replay
```

The output directory must be new. `traces.json` is an object keyed by exactly the plan's run IDs. Each value contains normalized `events`, optional `signals`, and an `oracle_pass` bool from the independently recorded verdict:

```json
{
  "rich-editor-b1-T": {
    "events": [
      {"t": 0.0, "type": "tool_call", "name": "save"},
      {"t": 1.0, "type": "submit"},
      {"t": 1.2, "type": "done"}
    ],
    "signals": [],
    "oracle_pass": true
  }
}
```

Provide an entry for every run; the example shows one record's shape. Replay stays uncontrolled and rejects controlled mode, so recorded or scripted timing cannot earn a live performance claim. A suite above 20% invalid returns exit code 1; valid task failures remain visible in `successes` and individual verdicts.

Output: `plan.json`, `meta.json`, `results.jsonl`, `summary.json`, `summary.md`, and numbered per-run evidence directories with the normal RunDir artifacts. Missing submissions retain null TTS/calls and count against the success rate; they are not assigned fabricated times or quietly excluded. Success comparisons use rates when invalid exclusions change the denominators.

Verify:

```sh
PYTHONPATH=tests/lib python3 -B -m unittest selftest.test_bench selftest.test_bench_runner selftest.test_host_adapters -v
```
