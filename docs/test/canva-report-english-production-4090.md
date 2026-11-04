# Canva benchmark report English deployment

2026-11-04 13:37 KST (04:37Z): the translated benchmark report was deployed to the 4090 production server.

## Deployment result

| Item | Result |
|---|---|
| Public URL | https://webagenticmemory.com/benchmarks/canva.md |
| Release | `20261104T043407Z-main-04cb133` |
| Source commit | `04cb13318acb0c7dff7734b6613325c361deb827` |
| Previous release | `20261103T094011Z-main-2e62600` |
| R7 results | T: 473.976 sec / 123 tool boundaries; M: 423.193 sec / 106 tool boundaries |
| API binary | SHA-256 `d4e401ea6db1049e3e0d6983ad95a82bbd357f1843012a506feb7eba6c5b05bb`, unchanged |
| Production data | 38 official memories and 880 official points preserved; no schema change |

The static route is built from `docs/test/benchmarks/canva.md`. Its prose and table labels are now English; identifiers, evidence paths, measurements, and claim limitations were retained.

## Verification

| Check | Result |
|---|---|
| Production source comparison | 625/625 files identical to the candidate; 0 older committed versions and 0 production-only files |
| Production web build | Passed; 0 build errors and 2 existing `SetupGuide.svelte` warnings |
| Built report | Byte-for-byte match with the source; no CJK text |
| Fixed Linux builder | 59 test suites; 437 passed, 0 failed, 31 ignored; 1 filtered test; release build passed |
| Upload integrity | SHA-256 matched for source and web archives, manifest, release metadata, build script, and activation script |
| Activation checks | API readiness, admin/review boundaries, official authorship/score, and static pages passed; no rollback |
| Public HTTPS | `/benchmarks/canva.md` returned HTTP 200 with the English R7 rows; API `/healthz` returned `{"status":"ok"}` |

The active release metadata records activation at `2026-11-04T04:37:34Z`. The deployment archive, extracted source, public files, build log, and hash manifests are under `/srv/web-agentic-memory/builds/20261104T043407Z-main-04cb133/`. The database backup is under `/var/backups/web-agentic-memory/20261104T043407Z-main-04cb133/`.

Rollback remains available through the previous release at `/srv/web-agentic-memory/releases/20261103T094011Z-main-2e62600`.