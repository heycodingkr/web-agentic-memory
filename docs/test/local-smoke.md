# Local browser smoke test

The browser smoke test uses a fresh temporary `ApplicationRepository` and a fixture-only server example. It exercises the real HTTP router, GitHub session state and PKCE checks, cookies, CSRF, contribution intake, curator worker, public catalog, recall, scoring, withdrawal, consent, and account deletion. OAuth exchange and replay evidence are deterministic local fakes. No GitHub, model, or browser replay service is called.

From the repository root:

```sh
cargo check -p wam-server --example smoke_fixture
cd web
pnpm install --frozen-lockfile
pnpm exec playwright test --list
pnpm test:smoke
```

Playwright uses the installed Google Chrome channel and Node 26's built-in `node:sqlite` module. The test starts `smoke_fixture` on `127.0.0.1:3105` and Vite on `127.0.0.1:5185`; both ports must be free. Vite sends `/v1`, `/mcp`, and `/healthz` to the fixture through `VITE_WAM_API_TARGET`. The fixture requires `WAM_SMOKE_DATA_DIR` under the OS temporary directory and `WAM_SMOKE_KEY` (at least 24 bytes). The Playwright config creates a fresh, empty directory and passes a fixture-only key, then removes that directory when the test runner exits. `reuseExistingServer` is false so the suite cannot silently target an unrelated local service.

The test begins with an empty public catalog and a 401 session response. It signs in through the fake GitHub redirect and checks a real browser session. It submits `navigate → edit → save` from the contribution page, calls the key-protected `/__smoke/curate` endpoint, and checks the published memory in catalog, detail, and recall. It then uses the page's action order controls to submit `navigate → save → edit`, curates again, and checks that the memory ID stays stable while the immutable version ID changes. It also checks the resulting 40-point owner total, withdrawal, consent, account deletion, unauthorized responses, and response trace headers.

Global setup builds the actual `wam` CLI and `wam-mcp` stdio binaries once. The test runs both against the fixture endpoint with cache and telemetry spool paths under the same temporary directory. An unauthorized CLI rank command must print a trace reference and store a `cli/http_error/401` diagnostic with that trace. A malformed MCP input line must return a framed JSON-RPC parse error with a trace reference and store an `mcp/invalid_input` diagnostic. The test reads the fixture's SQLite diagnostics directly, polls for these rows, and checks that synthetic private markers are absent from stored diagnostic documents. No public log-reading API is used.

The browser also raises an actual unhandled promise rejection and checks that the client hook records `web/unhandled_rejection` in the same SQLite database. Local missing-auth and malformed-stdin CLI errors must print the same references stored under `dashboard/invalid_input` and `recall/invalid_input`. Invalid UTF-8 on MCP stdin must keep stdout empty, print a reference on stderr, and store the matching `mcp/unhandled_error` event.

The final navigation case follows a real client-side link to a nonexistent page. It checks SvelteKit's normal `Not Found` display, a persisted `web/http_error/404`, absence of `unhandled_error/404`, and no path sentinel in diagnostic documents. It runs after the lifecycle case so the extra anonymous navigation requests do not consume its initial burst budget. The [tracing coverage cases](tracing-coverage.md) also cover remote JSON/SSE MCP responses and server-issued trace IDs in native clients.

`/__smoke/curate` exists only in the `smoke_fixture` example. It is absent from the production binary. The example refuses non-loopback binds and refuses a data directory outside the OS temp directory. Its replay verifier is deliberately trusted synthetic evidence; this test does not prove browser replay quality or production provider behavior.
