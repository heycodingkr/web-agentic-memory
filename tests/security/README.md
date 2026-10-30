# Security scenarios (L5)

Attack scenarios run only against our own loopback server and a disposable database. All secrets are synthetic sentinels. Spec: [docs/test/security-scenarios.md](../../docs/test/security-scenarios.md).

Status: the deterministic text-screening corpus, 500-value sentinel scanner, and an eight-sink loopback collection runner are implemented. PostgreSQL/Redis/WAL, live model/host and staging exports and full agent injection-resistance experiments remain separate.

| Directory | Contents |
|---|---|
| `inj-corpus/` | Injection and malicious-hint corpus (`.jsonl`; target at least 100 entries, see docs/test/metrics.md section 2) |
| `check_sink_inventory.py` | Fail-closed inventory/coverage/hash/leak checker; actual transport collection lives in `web/e2e/security-sinks.spec.ts` |
| `scenarios/` | One file per scenario, named `<SEC-ID>.*` |

`inj-corpus/curator-text-screening.jsonl` contains 160 inert synthetic cases across instruction override, URL/domain, credential request, secret-shaped fixture, off-site/exfiltration, hidden-character, and email categories. They contain no live credentials and make no network calls. `crates/wam-gatekeeper/tests/security_corpus.rs` checks each expected stable screening reason; `tests/lib/selftest/test_security_corpus.py` checks that the committed JSONL matches its generator.

Regenerate or verify it with:

```sh
python3 -B tests/security/generate_inj_corpus.py
python3 -B tests/security/generate_inj_corpus.py --check
cargo test -p wam-gatekeeper --test security_corpus
```

The test proves the local text-screening contract for these cases; it does not prove every encoded or cross-field attack is detected end to end. Sentinels are defined only in `../sentinels.json`, outside this directory so L0, L1, and L4 tests can share them.

The shared corpus contains 500 deterministic, synthetic-only values across ten formats. Generate or verify it with:

```sh
python3 -B tests/security/generate_sentinels.py
python3 -B tests/security/generate_sentinels.py --check
```

Scan explicit local sink exports or evidence directories with:

```sh
python3 -B tests/security/scan_sentinels.py target/e2e-logs/<suite>/<run-id>/sinks \
  --json-out target/e2e-logs/<suite>/<run-id>/sentinel-scan.json
```

The scanner checks contents and names for literal, JSON/nested-JSON escapes, URL-percent-encoded, Base64/Base64URL, hex, paired split fragments, and NFKD/invisible-character/common-confusable normalization. That normalization covers an explicit homoglyph map, not every Unicode or case-changing obfuscation. It skips symlinks, excludes the manifest itself, and treats unreadable or over-limit files as errors (64 MiB per file by default). Exit codes are 0 for clean, 1 for a leak, and 2 for a scan error. Results contain paths and sentinel IDs/forms, never matched values.

Run the actual loopback sweep (`SEC-A24/25`) with installed Chrome, Cargo, Python 3, Node 22+ and pnpm:

```sh
pnpm --dir web run test:security
python3 -B tests/security/check_sink_inventory.py path/to/inventory.json --json-out path/to/new-report.json
```

The test starts an owned server and frontend with synthetic OAuth/replay/provider fixtures, new application storage and an isolated anonymous client environment. It exercises 500 web URL previews, 500 idempotent API contributions, 500 hostile field-name rejections, 500 native stdio MCP recalls, and ten CLI recalls plus ten CLI/MCP errors. No real keychain/account/model call is used. The opt-in smoke fixture raises quota capacities x100; this is not a rate-limit acceptance test.

`target/e2e-logs/security-sinks/<run>/` contains source/dirty/runtime metadata, final trace/case results, coverage, and `inventory.json`/`report.json`. Each of client, web, API, local application database, diagnostic rows/SQLite snapshot/server log, shared client cache, actual owned MCP argv/environment, and Gatekeeper fake-provider input/output has its own nonempty artifact list and SHA256. The checker refuses missing/skipped/duplicate sinks, altered or empty files, traversal, symlinks, duplicate JSON keys, and scan errors. A deliberately contaminated control must detect all 500 IDs; its raw file is removed and the ID-only result retained. Reports are exclusive mode 0600.

This verifies the local fixture paths. It does not automatically cover every production deployment sink from [the shared sink list](../../docs/test/README.md#41-유출-검색-대상-sink-목록). In particular, PostgreSQL/Redis/WAL, OS secure stores, real upstream model logs, native host storage and deployed ingress need their own captured inventory and cannot be inferred from a clean local run.
