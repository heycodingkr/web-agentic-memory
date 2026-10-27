# Security scenarios (L5)

Attack scenarios run only against our own loopback server and a disposable database. All secrets are synthetic sentinels. Spec: [docs/test/security-scenarios.md](../../docs/test/security-scenarios.md).

Status: scaffolding only.

| Directory | Contents |
|---|---|
| `inj-corpus/` | Injection and malicious-hint corpus (`.jsonl`; target at least 100 entries, see docs/test/metrics.md section 2) |
| `harness/` | Loopback server launch, sink collection, sentinel-variant search, evidence capture (sink list: docs/test/README.md section 4.1) |
| `scenarios/` | One file per scenario, named `<SEC-ID>.*` |

Sentinels are defined only in `../sentinels.json` (not created yet; plan W6-1 asks for at least 500 variants). The file sits outside this directory because L0 property tests, L1, and L4 (SMK-14) import it too. The existing canary pattern, which plants a `SENTINEL` string and asserts it never appears in output, is in `reference/agent-browser/crates/browser_memory/tests/`. The new corpus extends it by format and variant.
