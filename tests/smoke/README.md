# Host smoke tests (L4)

Checks that install, register, call, and clean up work end to end on real hosts. Spec: [docs/test/host-smoke.md](../../docs/test/host-smoke.md).

Status: scaffolding only.

| Directory | Contents | IDs | Porting source |
|---|---|---|---|
| `common/` | Host-independent scenarios, run through a host adapter in `../env/host-profiles/<slug>/` | SMK-01 to SMK-14 | `reference/agent-browser/scripts/probe_web_agentic_memory.py` (real stdio MCP smoke). It targets the old product, so it has to be generalized |
| `behavior/` | Whether the agent actually picks the tool: prompt, trace verdict, independent oracle | BEH-01 to BEH-07 | `reference/agent-browser/scripts/probe_agent_memory_live.py` (trace JSONL, oracle) |

- Per host, SMK-01 to 05, 07, 10, 14 and BEH-04 must pass. At least two hosts must pass for the M3 gate (docs/test/host-smoke.md section 5).
- Completion is judged by an explicit Done or Failed event in the trace. A quiet log is not evidence of completion.
- Each run uses its own HOME, XDG, TMPDIR, and session, and cleans up only its own process group.
- Evidence: `target/e2e-logs/smoke/<run-id>/`.
- Suggested first target: `generic-mcp`. It is fully automatic and needs no host CLI login.
