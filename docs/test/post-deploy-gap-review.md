# Post-deploy customer journey and hardening review

Checked: 2026-11-01. This records a read-only Arise check of the deployed site plus synthetic local tests. The deployed site was not mutated: no real email was sent, no production contribution was submitted, and no production worker/replay was run. A screenshot is a visual check, not proof that an account or processing path works.

## Deployed customer-facing screens

Arise screenshots were captured and shown in the review conversation for the English and Korean home pages, `/login`, `/memories`, `/leaderboard`, `/guide`, `/contribute`, and `/dashboard`.

| Screen | Observed | Acceptance |
|---|---|---|
| Home (`/`, `/ko`) | Main message, setup links and demo clips rendered coherently. Demo clips are explicitly described as scripted local editor replays. | Visual smoke only |
| Login (`/login`) | Email-code sign-in form was visible. | Form rendering only; real inbox receipt and session acceptance were not run |
| Public memory list (`/memories`) | `0 of 0 workflows`; empty catalog state. | Fails the real recall/customer-journey acceptance because there is no production memory to use |
| My Page (`/dashboard`) | Deployed page still showed GitHub/local-demo wording from the old bundle. | Fails deployed email-account and post-login copy acceptance |
| Contribution (`/contribute`) | Deployed page still described a GitHub session/local demo token and local submission. | Fails deployed email-account copy acceptance; no submission was made |
| Leaderboard and guide | Policy and setup information rendered. | Read-only visual smoke only |

The working tree now has updated login/contribution copy and device-link return handling, but those local changes have not been published. The screenshots therefore document the current deployment, including its stale dashboard/contribution copy.

## Synthetic verification

These checks use synthetic accounts, messages, records, and memory fixtures unless explicitly marked as a deployed visual check. Local results do not substitute for production acceptance.

| Check | Result | Evidence and limit |
|---|---|---|
| Email authentication browser flow | 20/20 desktop/mobile passed | `pnpm --dir web exec playwright test --config playwright.email-auth.config.ts`; fake API and synthetic email codes, no real inbox |
| Device-link return and origin hardening | Focused regression tests passed after adding the previously failing malformed callback cases | CLI code survives email and GitHub sign-in, malformed/off-site values are rejected, and neither path auto-approves |
| PostgreSQL/Redis integration | Passed; PostgreSQL application 16, diagnostics 2, Redis limiter 2, erasure cleanup 1, gatekeeper queue/store 4; no failures | `target/e2e-logs/release-data-services/20261101-074939-c04d/`; owned disposable services, cleanup verified, dirty source |
| Rust workspace | `cargo test --locked --workspace` passed; service-dependent ignored cases are listed as such | Includes 13 durable lifecycle tests; owned PostgreSQL/Redis integration was run separately above |
| Web unit/type/build | 41 unit tests passed; Svelte check 0 errors/0 warnings; production build passed | Local build; deployment not updated |
| Customer journey smoke | 1/1 passed with synthetic email OTP, contribution, fixture worker/replay, publication/version replacement, points, withdrawal and account deletion | `pnpm --dir web run test:smoke`; fake sender is available only through the keyed loopback fixture; no production account/data changed |
| My Page browser | 14/14 desktop/mobile passed | `playwright.my-page.config.ts`; synthetic API responses, including result memory/version and model-budget wait copy |
| Security sink sweep | 1 scenario passed; 8/8 sinks, 500 synthetic sentinels, 0 hits, 0 collection errors; contaminated positive control detected | `target/e2e-logs/security-sinks/2026-11-01T07-26-40-963Z-85f7/`; local smoke server, not a complete production sink inventory |
| Search scale profile | Passed on all 1k/10k/100k synthetic profiles; maximum DB candidate p95 21.12 ms, maximum loopback HTTP p95 26.33 ms; all reported result IDs/scores matched the core oracle | `target/e2e-logs/search-profile/20261101-070212-d264/`; 100k profile had 1.25M index rows, 764,738,583-byte DB, concurrency 1 and warm repeated workload; peak process RSS includes the exhaustive oracle, excludes PostgreSQL |

The first-sign-up/mailbox-help and production contribution-copy assertions passed in the final 20-case browser suite. The local full-journey smoke initially exposed two stale accessible-name assertions (`No public memories yet.` and `Version and URL scope`); the test was updated to the current UI labels, then passed end to end. These are test expectation fixes, not production behavior changes. See [email authentication production acceptance](email-auth-production.md) for the live email recipient, account and DNS gates.

## Database and performance

The pre-fix 100k release profile used 100,000 synthetic records and 1.25 million index rows. Worst-trial DB candidate p95 was 25.149 ms, while HTTP p95 was 502.558 ms against a 500 ms local target. Query-plan inspection showed the candidate lookup was not the slow part: each recall also rebuilt the allowed keyword vocabulary with a full `SELECT DISTINCT` across the active index. This matched the earlier 553.871 ms HTTP result and the gap recorded in issue #97 and [release readiness](release-readiness.md).

The PostgreSQL path now stores a shared catalog generation, bumps it with statement-level triggers when records, versions or index rows change, and caches the derived vocabulary per database connection. Each recall reads the one-row generation and only rebuilds the vocabulary after a change. Migration `0004_catalog_vocabulary_generation.sql` and the cross-writer regression test are in the working tree. This cache must keep passing the disposable PostgreSQL invalidation test; do not remove the generation check or substitute a time-only TTL.

The pre-fix 100k run measured 25.149 ms candidate DB p95 and 502.558 ms loopback HTTP p95. After vocabulary caching, a fresh 1k/10k/100k run measured worst-case 100k DB p95 21.117 ms and HTTP p95 26.332 ms. The 100k hot-host case selected 98 candidates with DB p95 8.454 ms and HTTP p95 12.895 ms. `wam_memory_scope_lookup` appears in the actual candidate query plan, and seven fixed query classes retained candidate/result parity with the exhaustive oracle. Evidence includes the corpus digest, raw trials and `EXPLAIN (ANALYZE, BUFFERS)` plans. This local, concurrency-one warm profile does not certify a dedicated runner, 100 RPS, extreme host skew, cold cache or 1M production performance. Continue those checks under [search performance](../spec/search-performance.md) and [release readiness](release-readiness.md).

## Cache, optimistic UI, and coverage review

The server already has a PostgreSQL per-connection vocabulary cache keyed by a shared catalog generation. The CLI/MCP client also has a bounded local cache, but only anonymous recall responses can use it after the server's explicit X-WAM-Client-Cache opt-in; authenticated recall remains online because receipts, usage and score rules are side effects. The web UI does not maintain a response cache and now forces Fetch cache mode no-store for API calls. The server now sends Cache-Control: no-store for health, vocabulary, memory detail, dynamic API errors, and its already-protected catalog/owner routes. Detail GET counts a view, vocabulary can change with publication, and errors can change after state changes.

Optimistic behavior is limited to local form preview and busy indicators. After the contribution POST returns a receipt, the receipt is shown immediately; if the follow-up list request fails, the page keeps the confirmed receipt and says only that the list could not refresh. Review status, replay/publish results, version links, account points, profile saves, withdrawal, and consent changes remain server-confirmed. The local customer-path browser test now simulates the post-acceptance list failure.

Coverage snapshot from the focused server and web runs: Rust wam-server all-target coverage is 75.83% lines, 80.29% branches, and 79.45% functions. The server coverage run passed when serialized; disposable PostgreSQL/Redis tests are ignored by that coverage command but separately passed in the owned-service run above. Web Node tests passed 46/46 with 99.75% line, 87.05% branch, and 93.16% function coverage for the tested JavaScript helper modules. Those figures exclude Svelte component source; the loopback browser smoke passed 1/1, Svelte checking reported 0 errors/warnings, and the production bundle built. Rust uncovered areas include durable/postgres.rs and rate_limit/redis.rs (environment-gated in coverage), durable/runtime.rs, the wam-worker executable, durable/adapters.rs (67.21% lines), and diagnostics storage (62.09% lines). See CACHE-12/13, DB-28, and PERF-26 for repeatable follow-up gates. These local tests do not update the live deployment.

## Remaining work and acceptance gates

| Work | Required proof | Tracking |
|---|---|---|
| Publish the corrected web bundle and verify signup help/My Page/contribution routes on the real HTTPS origin | Build hash matches deployment; Arise confirms new wording and deep links after deployment | #160–#162; deployment portion of #157 |
| Verify real email delivery and login | Approved QA mailbox receives a code from the verified sender; same-browser verify, reload, logout and host authorization work | #157; EMAIL-PROD-01~05, 19~22 |
| Populate production with independently reviewed memories and start the trusted replay worker | Real worker/replay evidence; published memory appears in public catalog; worker restart/retry is idempotent | #159 |
| Accept the owner result on the deployed worker path | Local owner-scoped progress/reflection is implemented and synthetic lifecycle-tested; deployed worker result, memory/version, another account's eligible +2 and ledger must agree | #162; #157/#159; MYPAGE-07~22, 25, 39~40 |
| Complete hosted-service DB, load and security rehearsal | Restore/failover, concurrent writes, retention, indexes/plans and headers/auth boundaries on production-like PostgreSQL/Redis and ingress | [DB-01~27](../spec/database.md), [REL-08~10](release-readiness.md) |

## README and landing copy check

The README's pilot evidence appears once in the summary table and again in the detailed results narrative. On the landing page, the blog/Slides coverage appears in both workflow cards and the demo section, and the blog pilot figures repeat. This is repetition rather than conflicting data: keep the summary table/cards as the scan-friendly overview and let the later sections explain method and limits without repeating the same figures or coverage list. The editorial cleanup is tracked in issue #166.

Update 2026-11-01: the landing part is done. The separate demo section was removed, so Slides and Blog appear once, in the workflow cards, and the blog pilot chips show once, on the Blog card. Repeated reassurances (no account, keep your browser tools), the Slides/Docs/Sheets/Email/Forms "not benchmarked" notes and the benchmark section's second summary paragraph were shortened or merged, and the EN/KO copy now uses one name for the hints, the setup page and the memory library. The README part is still open.

## Database, performance and security follow-up specification

1. Re-run the fixed synthetic 1k/10k/100k profile after every query/migration change. Assert result parity against the core oracle, no HTTP errors, DB p95 target ≤100 ms, and local HTTP target ≤500 ms. Preserve raw plans and buffers. Add hot-host skew, no-match, warm/cold split, and 100-request-per-second load as separate cases; run 1M only on a provisioned host.
2. In production-like PostgreSQL, record `EXPLAIN (ANALYZE, BUFFERS)`, rows scanned/returned, index name, cache hit ratio, table/index size and write overhead. Check the public catalog, owner dashboard, queue lease and score-event plans. Add an index only when a measured plan demonstrates a scan or latency problem.
3. Stress cache invalidation across two independent API connections and one worker connection for new publication, version replacement, support, retirement, rollback and index repair. The visible vocabulary must reflect committed state immediately after the generation changes; failed transactions must not publish a partial vocabulary.
4. Repeat security cases on a staging origin: HTTPS/HSTS, cookie flags, CSRF and Origin rejection, CORS/frame policy, open redirects, email code brute-force/replay/expiry, device approval/expiry, secret-free diagnostics, anonymous quota outage behavior and account isolation. Use synthetic secrets and test accounts; run sentinel inventory for API, DB/WAL, Redis, CDN/proxy, worker and host logs separately.
5. Keep deletion/restore, PostgreSQL/Redis outage and failover, worker restart and in-flight SIGTERM checks in the release gate. A clean loopback sentinel sweep or `/healthz` response is not a production operations pass.
