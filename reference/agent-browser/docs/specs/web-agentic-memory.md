# Web agentic memory — server catalog and contribution consent

Status: v1 implemented and locally verified, 2026-10-17. Not deployed or installed into the running user app. Existing browser_memory was local/personal only before this change.

## Product contract

- The settings label is **Web agentic memory** / **웹 에이전틱 메모리 활용**. Do not put “public/shared/common” in the user-facing feature name.
- Personal memory keeps its own database and existing controls. Turning web agentic memory off must leave personal recall/save/expand/forget working.
- A separate persisted boolean controls web agentic memory. It defaults on so first use can benefit from curated workflows. The current value controls future lookup/injection dynamically, without restarting the app. Off means no server catalog request or catalog injection. Refresh must not block the UI thread.
- A separate **Contribute web workflow improvement data** consent defaults off. Reading approved knowledge never requires contributing data. The existing telemetry consent must not silently authorize this new data category.
- Contributions are append-only untrusted reports. A user or client must never be able to publish/edit approved knowledge. Only existing server administrators may create/edit candidates, record verification and approve a versioned release. Withdraw/revoke must remove stale guidance on next refresh.
- Approved workflows are bounded, domain/task-scoped, untrusted hints. They cannot change user permissions or authorize publishing, purchases, destructive actions or credential access. No private credentials/account identifiers in catalog records.
- The catalog and personal store have distinct ownership and identifiers; server content must never be imported as mutable personal records or overwrite/forget personal entries. Offline fallback uses bundled reviewed workflows and/or last validated catalog, subject to the current feature switch.

## Contribution minimization

Automatic opt-in collection should use a deliberately closed schema: site hostname, tool/action category, outcome/error category and aggregate/sequence statistics. Never send raw prompts, typed text, selectors containing text, page HTML, screenshots, full URLs, cookies, account identifiers or local personal notes. Any future free-text contribution must have a separate user-reviewed submission flow. Existing general telemetry keeps its existing narrow schema and endpoint.

## Dependency ordered goals and routes

| Goal | Route / owner | Output | Independent primary-agent check |
| --- | --- | --- | --- |
| G0 | Read-only delegated server/auth inventory; local exact files read here | Cite existing billing/server/admin/auth/test locations, instructions and git status; no edits or commands with side effects | Open every source location used for implementation; inspect status/diff/instructions |
| G1 | Primary: approved catalog, contribution inbox and admin release contracts in existing server | Validated schema, server-enforced admin write boundary, immutable versions and submission isolation | Focused endpoint tests: anonymous read, ordinary submit, non-admin write denied, unapproved candidate invisible, approved/revoked versions |
| G2 | Primary: isolated Rust catalog client/cache and personal-memory coexistence | Bounded domain/task hints, offline behavior, no personal DB mutation | Local HTTP oracle counts requests; inspect personal DB before/after; malformed/oversized/unapproved payload tests |
| G3 | Primary: settings + API/CLI/MCP routing, independent contribution consent | Requested Korean label, saved settings, dynamic toggles, no upload without consent | Toggle tests and real app smoke with isolated data dir; off means zero catalog calls/injection while local recall remains |
| G4 | Primary: minimized workflow report capture and delivery | Closed-schema reports queued/delivered only while consent permits; no raw tool parameters/results | Sentinel secrets in all raw inputs must not appear in queued/uploaded JSON; revocation blocks further upload |
| G4N | Mechanical worker: registered navigation/wait names in Rust allowlist, server allowlist and UI harness | Three-file identifier alignment only | Primary opens all three resulting locations; failing action-collection and endpoint tests must pass |
| G5 | Primary: review, builds, tests and smoke | Source diff review, verified docs and commit | Required crate/server tests/builds here, independent live fixture/oracle checks, clean scoped commit |

G0 may run while the primary reads local memory/settings code. Server edits wait for verified inventory. G2 and G3 depend on G1 contract. G4 depends on G1+G3. No production deployment, account creation or real-user data upload is part of development verification.

## Local integration-test command

Run `./scripts/test-browser-memory.sh` for the memory integration suites. It sets `AGENT_BROWSER_NO_KEYCHAIN=1` only for the test process so synthetic member fixtures remain file-based. Without that override, macOS may migrate fixture tokens into OS storage and the catalog/endpoint-switch tests can fail. This flag is not a production setting. For a combined run, use `AGENT_BROWSER_NO_KEYCHAIN=1 cargo test -p browser_memory -p chat --lib --tests`.

The catalog HTTP fixture waits for the complete CRLF-CRLF request header, rejects truncated/oversized headers, and has deterministic split-read regressions. A single TCP read is not a complete HTTP request.

## Test inputs and expected results

1. Legacy settings missing new fields: web memory on, contribution off; existing personal auto-recall value unchanged. Serialize/reload preserves both new switches.
2. Personal note for medium.com plus approved server note for medium.com: both relevant hints available with web memory on. Off removes only server hints and causes zero catalog network calls. Turning on again works next lookup without restart.
3. Unrelated example.org note, draft/unapproved record, unknown schema, oversized document, invalid domain or credential-like catalog text: no cross-domain injection or invalid cache replacement. Existing valid cache survives transient failure.
4. Clean data directory offline: personal recall does not create unwanted DB and bundled reviewed notes are usable when web memory is enabled.
5. Public record ID passed to personal save/forget: cannot modify catalog. Equal local/catalog numeric identifiers do not collide.
6. Anonymous GET catalog: approved revision only. POST contribution: becomes pending inbox material, never a catalog record. PUT/POST admin candidate/release with missing or ordinary-user auth: denied server-side.
7. Admin candidate without verified evidence: publishing rejected. Approved verified release: new immutable revision served. Withdrawal/revocation: stale entry removed on refresh.
8. Contribution consent false, existing telemetry upload consent true: no workflow report queue/upload. Consent true records only allowlisted metadata. Turning consent off blocks pending upload and future capture.
9. Raw input sentinels (password, token, typed article text, email, query URL, selector text, local path, screenshot bytes): absent from report payload and logs. Only hostname and fixed action/outcome categories remain.
10. Selftest/evaluation sessions do not send data to production. Test servers bind loopback and use disposable stores. Tests and browser loops run in the primary session.

## Implemented v1 behavior and evidence

- Settings: `web_agentic_memory_enabled` (default true), `web_workflow_contribution` (default false), and optional development/staging `web_agentic_memory_endpoint`. The existing `browser_memory_auto_recall` remains the personal-memory control. Korean feature label is exactly **웹 에이전틱 메모리 활용**.
- The app reads cached/bundled notes without waiting for HTTP; refresh runs in a dedicated thread. API and stdio MCP recall/expand use the latest saved switch on each call. A response arriving after disable is discarded. Earlier model conversation content is not erased by a setting change.
- Server cache: `memory/web-catalog.json`, endpoint-bound, validated v1/approved records only, max256KiB/32 records, 3s request timeout and5min refresh interval. No page URL or task query is sent with the full catalog GET. Last valid cache survives transient failure; an approved empty release overrides bundled notes. Missing app settings use bundled notes offline.
- Public IDs use `web:<slug>`; personal IDs remain integers. Web IDs cannot be passed to local replace/forget. Automatic web hints are domain-scoped and <=400 estimated tokens; deeper steps/evidence are expanded on demand.
- Automatic contribution requires BOTH master collection and the new opt-in; ordinary telemetry upload consent neither authorizes nor blocks this distinct opt-in channel. Selftests can talk only to explicit loopback fixtures. Reports contain only an observed site hostname, fixed browser tool names, result flags, app version and terminal category. They are bounded samples of completed actions, not a full replay or proof that a site saved/purchased successfully. Raw tool input/result bodies never enter the collector API.
- Pending reports: `memory/workflow-reports/*.json`, private files, max100 queued and24 actions per report; server accepts <=2KiB. Consent is rechecked before capture and every upload; disabling clears pending local reports. Already-started network requests may finish. The existing delete-data action clears this queue along with general telemetry.
- Curator UI in MeshCode admin at `/web-memory`: submissions inbox, workflow form, candidate creation, smoke evidence recording, approval, retained releases/rollback. Candidate drafting is currently manual. AI candidate generation and user-reviewed free-text/image submissions are not implemented. Recording verification is an admin attestation to an externally run smoke artifact; the server never executes candidate/report commands.
- Backend routes mount only under `WEB_AGENTIC_MEMORY_ENABLED=true` (default off). Existing admin JWT role controls all curation. Anonymous catalog reading and minimized contribution do not create a new signup requirement. Existing SystemConfig/FeatureEvent storage is reused without migrations.
- Real PostgreSQL smoke uses a freshly initialized loopback cluster with an explicit guard and automatic cleanup. It verified concurrent updates, rollback after an exception and one successful publication for two candidates sharing a base revision.

Completed checks: host110; chat693+3ignored; agent_browser176+2ignored; llm285; personal store18; memory tool contract7; personal context4; web catalog7; reports6; MCP transport11. Final app/MCP builds passed. Existing compiler/dependency warnings remain. Python smoke scripts compile. No production endpoint or real account was used.

The final real stdio MCP smoke passed in one process (off→on→off→on), with one loopback catalog GET, preserved personal notes and blocked web mutations. Artifacts: `target/e2e-logs/web-memory-mcp-final/result.json`. The personal-only persistence test explicitly disables web lookup so forgetting a personal note is not confused with an independent bundled workflow. A corrupt personal DB also preserves read-only web access without overwriting the corrupt file; invalid recall arguments still fail.

Administrator UI smoke passed through the actual isolated Arise CEF browser: native coordinates and typing created a candidate, confirmed it was absent from the public catalog, recorded smoke evidence, then published it. Artifacts: `target/e2e-logs/web-memory-admin-ui-v3/result.json`, `approved.png` and `after.png`. Representative browser PNGs were inspected. An earlier harness attempt missed a click while wheel scrolling was still moving; the final harness requires fresh snapshots, stable geometry and a hit test before dispatch. This validates the harness procedure, not a general product fix for every site editor.

Server regression suite578 PASS; real disposable PostgreSQL2 PASS; server typecheck/build and administrator Svelte check/build PASS (0 errors/0 warnings in Svelte check). [Server result and scope](../../../meshcode/docs/architecture/server/tests/results/2026-10-17-web-agentic-memory.md).

Local review preview: http://127.0.0.1:5714/web-memory (frontend PID64944; fixture API PID83051 at127.0.0.1:65467). These detached development servers use synthetic accounts and in-memory catalog/report storage only. Test-created Arise processes and PostgreSQL clusters were stopped; the existing user app/profile was preserved.

Native desktop capture attempted via MeshCode screenshot returned GUI IPC Connection refused. Browser-page captures are independently available through the tested Arise MCP; do not describe native Settings appearance as visually verified based on a browser-only capture.

## Initial cause and review constraints

The existing Usage statistics card has local collection and anonymous feature-event upload controls, but its schema excludes chats/page contents and provides no web-workflow curation/catalog channel. The existing memory database contains personal records with local mutation tools. Reusing either as a global writable prompt store would mix ownership and bypass administrator review. Implement separate catalog/contribution routes and explicit consent instead.

G0 source review confirmed the existing Hono createApp/JWT admin guard and Svelte API client; those boundaries are reused. Focused failing cases found and fixed: missing first-release rollback snapshot, streaming body overflow incorrectly mapped to400, noncanonical www domains, mismatched registered navigation/wait tool names, and personal-store failure incorrectly blocking independent web recall. Each was reproduced before its corresponding implementation fix. Scope review excludes other sessions’ inquiry/account-server edits.
