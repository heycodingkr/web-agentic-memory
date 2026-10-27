# MCP browser session ownership

## Behavior and design

The application, shared by all MCP processes and built-in agents, owns the lease registry. Window ownership is not agent ownership. Each lease identifies a session, a display client name, a window, a tab, a generation token and an expiry. Display names never identify sessions. Read-only tab discovery reports all windows and their tab owners. It does not reserve a tab.

`browser_acquire` reserves an explicit unoccupied tab, or creates a background tab when no tab is specified. A first page operation automatically creates a private background tab; embedded CLI agents may explicitly inherit their originating tab. `browser_new_tab` always creates and reserves a new tab. The server allocates the ID before dispatching creation and returns that exact ID, never a before/after list difference. `browser_select_tab` acquires before selecting. `browser_release` releases the session's leases without closing tabs. `browser_heartbeat` renews them. Normal calls and a client keepalive renew leases; EOF releases them, and killed clients expire.

Every page operation carries the selected lease. The socket handler validates it, installs an explicit window/tab scope and captures that target and generation in the queued command. The UI pump rejects stale generations. UI tab/window focus changes cannot redirect queued commands. Anonymous legacy clients cannot operate on leased tabs. OS-global native input is refused in owned requests. Per-tab screenshots use the browser's internal DevTools method, without selecting another tab. Built-in API and CLI turns first reserve their originating tab. If another session owns it, the turn atomically reserves a new background tab in that same window, copying an http(s) task URL and preserving the original owner and UI selection. API tools wait up to eight seconds for that tab to be created; inherited CLI MCP acquisition already waits for readiness. Failed creation releases only the new reservation. A closed tab/window remains an error, and explicit external acquisitions still return tab_busy. After startup, page actions remain pinned to the captured generation; uncertain actions are never replayed on another tab. CLI instructions also explain browser_new_tab recovery for later tool-level contention and plain-language failure reporting.

MCP clients get a generated session identity by default. A host that shares one MCP process between conversations must pass a distinct optional `session_id` on each browser call. The server-side generation token stays in the MCP process rather than being guessed from list results. Lease state is in memory and is reset on application restart. Browser profile/cookies remain shared between normal windows.

## Root cause

External browser tools currently act on the active tab. The existing registry maps tabs to windows, not agents. MCP clientInfo.name is cached for audit only. New-tab creation currently detects a tab by comparing two listings, which cannot attribute simultaneous creation requests correctly.

## Atomic goals in dependency order

| Goal | Route | Output | Independent parent check |
| --- | --- | --- | --- |
| 1. Acceptance regression | Inline parent | Focused failing control-socket test plus this input/result specification | Run the focused test against existing code and inspect the failure |
| 2. Shared leases and targeting | Inline parent; depends on 1 | Lease registry, expiry/generation checks, explicit command scopes, atomic tab creation, discovery and per-tab capture | Run lease/control/command tests; inspect target capture and stale-command guards |
| 3. MCP and built-in consumers | Inline parent; depends on 2 | Acquisition/release/heartbeat tools, client identities, keepalive/EOF cleanup, API and CLI integration | Parent inspects all changed call paths and runs existing MCP/chat/CLI tests |
| 4. Live smoke harness and review | Inline parent; depends on 3 | Two real stdio MCP clients against an isolated freshly built application, captured JSON evidence and verification notes | Parent builds, invokes tools, checks DOM and lease results, inspects final diff |

A read-only bulk inventory of existing isolation/build helpers may be delegated during the long session. It may not edit files, launch applications, or run builds/tests. Its summary is orientation; the parent opens cited source before acting. Design, implementation, builds, tests and final review stay with the parent.

## Test inputs and expected results

| Input | Expected result |
| --- | --- |
| Two distinct sessions both labelled Codex acquire the same tab | One succeeds; the other receives a busy error identifying the owner; names do not merge ownership |
| A owns a tab; B lists tabs | Owner session/client, window/tab, expiry, occupied=true and is_mine=false are visible |
| Simultaneous new-tab acquisitions | Distinct exact IDs and leases; original human tabs are not navigated |
| A owns tab A; B owns tab B; switch UI focus between calls | Each session's navigation/eval/type/read stays in its own tab, including a queued command before the focus switch |
| B selects, closes, or writes A's tab; anonymous legacy request targets A | Refused before dispatch; no page mutation or tab close |
| A releases; B acquires the same tab; A sends an old generation | B succeeds; A's stale request and queued old command are refused |
| Fake clock advances past the TTL; B acquires | Expired ownership is removed, B obtains a fresh generation, old heartbeat cannot resurrect A |
| Normal stdio EOF; abrupt MCP process death | EOF releases leases; death stops keepalive and leases expire without closing tabs |
| API agent or embedded CLI owns a tab; external MCP claims it | Same busy contract; internal tools stay on their captured turn tab |
| Screenshot a session tab while another tab is active | Image is captured from the session's browser, never from the other tab or desktop |
| Explicit invalid window/tab/session/TTL; closed window; old app without lease support | Clear tool error; no fallback to another window or unprotected active-tab operation |
| Existing pure helper tools and external MCP hub tools | Existing behavior remains, without acquiring a browser tab |

## Verification

Parent-run verification on 2026-10-15:

- The initial focused regression failed against the old server: `session_acquire` returned `unknown cmd` (red evidence: `/tmp/agent-browser-ownership-red.log`).
- `cargo test --locked -p cef_host -p agent_browser -p llm -p chat -p i18n -- --test-threads=1`: 1,484 passed, 0 failed, 10 existing ignored tests. Log: `/tmp/agent-browser-ownership-tests-final.log`.
- After the final legacy-client target guards, the affected host and chat library suites passed (116 host tests, 669 chat tests, 3 existing chat ignores), and all 9 real-process/mock-socket MCP integration tests passed. The host suite was run again after switching tokens to UUID v4. Logs: `/tmp/agent-browser-ownership-targets.log`, `/tmp/agent-browser-ownership-mcp.log`, `/tmp/agent-browser-ownership-host-final.log`.
- `RUN=0 bash scripts/bundle-cef-dev.sh` built the GUI, stdio MCP server and CEF helper, then assembled and signed `target/dev-bundle/Arise.app`. Log: `/tmp/agent-browser-ownership-build.log`.
- `PROBE_SCRIPT=scripts/probe_mcp_ownership_live.py PROBE_CEF_URL=about:blank PROBE_DEBUG_PORT='' bash scripts/probe_arise_isolated.sh`: all 23 live checks passed against a fresh isolated application and real stdio MCP clients. This covered simultaneous distinct acquisitions, same client names with different sessions, visible owner metadata, busy/foreign-close rejection, independent background typing/navigation, release and generation fencing, keepalive, EOF release, crash expiry, explicit conversation IDs in one MCP process, unchanged original tabs and pure helpers.
- The parent inspected `target/e2e-logs/mcp-ownership/session-a-background.png`: it shows the blue A-next fixture while the other tab was selected. The image came from that tab's internal DevTools capture, not a desktop capture. Evidence JSON and application log are retained in `target/e2e-logs/mcp-ownership/`; invocation log: `/tmp/agent-browser-ownership-live.log`.
- The first live run attempted an immediate eval during navigation and hit the existing renderer-transition timeout. The final smoke uses `browser_wait_for` to establish page readiness and then asserts the actual new title. Navigation success means dispatch, not completed page loading.
- Final review checked that public discovery omits lease tokens, reacquisition requires the existing token, queued old generations cannot affect a new owner, anonymous requests capture and recheck their actual target, and a pinned endpoint cannot silently rediscover a different application. `git diff --check` and Python smoke-driver syntax checks passed.

The live check used multiple tabs in one native window; the existing and new automated window-routing tests cover separate windows. Authenticated Claude/Codex model turns and native OS-input/dialog workflows were not exercised by this local MCP smoke. The installed `/Applications/Arise.app` and existing registered MCP processes were not replaced or restarted.

## Calling the tools

Use the MCP server from the same new bundle as the GUI application:

```text
<repo>/target/dev-bundle/Arise.app/Contents/MacOS/agent_browser --mcp-server
```

- `browser_tabs {}` discovers windows and occupied tabs without reserving them.
- `browser_acquire {}` reserves a new background tab. Optional `window_id` chooses an existing window; `tab_id` explicitly acquires an existing unoccupied tab instead.
- Normal `browser_navigate`, `browser_eval`, `browser_type`, `browser_read` and `browser_screenshot` calls use the selected lease, independently of UI focus.
- `browser_release {}` returns all of this session's leases without closing its tabs. Keepalive runs while the MCP process is alive; call release when finished.
- A host sharing one MCP process between conversations must pass a distinct `session_id` consistently, e.g. `browser_acquire {"session_id":"conversation-a"}` followed by `browser_navigate {"session_id":"conversation-a","url":"https://example.com"}`. Separate MCP processes get unique defaults.

Native OS input and native image dialogs are deliberately rejected in leased requests because they cannot safely address a background tab (workspace follow-up issue #33). DOM/CEF input and `browser_upload` file-input uploads remain available. Normal browser profile/cookies are shared; a tab lease is not a separate login profile.
