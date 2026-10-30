# Web application

SvelteKit 3 static frontend. The public memory catalog/detail, leaderboard, public handle profile, personal rank, profile editor, and local contribution receipt flow call the typed API in src/lib/api.ts. Bio content uses a closed block schema; the renderer never inserts HTML. The editor offers paragraph/title/list, bold/italic, and HTTPS inline/service links.

Primary navigation starts with Leaderboard, followed by Agent setup, Contribute, and My activity. The memory catalog is a secondary reference named Shared workflows, linked from the guide and footer. It explains how the same workflows are recalled by an agent.

## Run locally

Start the API in one terminal:

~~~sh
WAM_DEMO_TOKEN=local-dev-token cargo run -p wam-server
~~~

Start the web app in another:

~~~sh
pnpm --dir web install
pnpm --dir web dev
~~~

Open http://localhost:5173. Vite proxies /v1, /mcp, and /healthz to 127.0.0.1:3000. Enter the configured demo token on the dashboard/profile page to read or save the seeded demo account. Public leaderboard/profile routes need no token.

## Check and build

~~~sh
pnpm --dir web run check
pnpm --dir web run build
~~~

The static adapter writes a 200.html fallback for dynamic /u/{handle} pages. Configure the deployment host to serve that fallback for unknown paths. Set VITE_WAM_API_URL at build time when the API is not same-origin/proxied.

The server uses PostgreSQL or one locked local application snapshot. Verified curation publishes immutable memory versions and +20 author awards; explicit successful use can add +2. Review can publish a replay-verified proposal, and approval without one requests verification. Profile settings include contribution-consent withdrawal and account deletion. See [server setup and replay configuration](../crates/wam-server/README.md). Remote MCP OAuth discovery, DCR, PKCE consent, refresh rotation/revocation, and CLI host registration are implemented. GET SSE and POST SSE responses are implemented. The optional local `wam browser-mcp` bridge retains an Arise browser MCP child and adds an explicit owned-current-tab recall tool; native macOS acceptance and isolated Codex/Claude registration have passed. Background automatic recall and SSE event replay are not provided. Live GitHub, Claude/Arise remote OAuth, other OS runtime acceptance, and signed release installation remain unverified.

## Setup guide

The guide separates connection, tool verification, and optional account linking. It generates remote HTTP or local stdio commands for Codex and Claude Code, and an endpoint URL for Arise MCP Hub with an explicit host-acceptance caveat. Command templates are shared with the CLI in `guides/install-templates.json`. Source installation builds both `wam` and `wam-mcp`; removal lives in its own collapsed section. Windows HTTP examples use PowerShell's `Invoke-RestMethod`.

Endpoint precedence: `VITE_WAM_MCP_ENDPOINT`, then the configured API base (`VITE_WAM_API_URL`) plus `/mcp`, then the page origin plus `/mcp`. The deployment's `/v1/auth/config` can supply its canonical OAuth resource when no explicit MCP build setting exists. Configure the public proxy route to reach `wam-server`. Local stdio, link, doctor, CLI recall, and HTTP examples derive their API base from the selected MCP address, preserving a deployment path prefix. Invalid or credential-bearing endpoints cannot be copied.

The endpoint supports anonymous public tools. Private tools require the right WAM identity and scope. Only the exact endpoint reported as OAuth-enabled by this deployment receives OAuth login instructions; other endpoints have an explicit local-stdio/device-link fallback. A clipboard success does not imply registration, discovery, authentication, or a working tool call. The inline recall form calls this website's configured API anonymously, strips URL query/fragment before sending, and uses the shared API error handling, including Retry-After.

Guide regression checks reuse an already-running frontend (default `http://127.0.0.1:5173`):

~~~sh
pnpm --dir web test
pnpm --dir web run test:guide
~~~

Set `WAM_WEB_URL` to test another frontend. The browser tests mock public API responses and do not modify real accounts, host configuration, or the system clipboard. They cover navigation, local/remote commands, endpoint changes, copy behavior, OAuth availability, recall normalization/errors, Korean chrome, and mobile overflow.

`pnpm --dir web run test:ui` instead starts an owned frontend and runs 70 cases across Chrome, Firefox, WebKit and mobile Chrome emulation: guide/dashboard failures, anonymous and signed-in accessibility, overflow, skip links, host selection and menu navigation. Install the pinned browsers with `pnpm --dir web exec playwright install chrome firefox webkit`. CI uses Node 22. On macOS the Firefox app-data lookup uses an owned temporary `CFFIXED_USER_HOME`; standard directories are created before launch and teardown failures fail the suite. Safari, physical mobile devices, screen readers and manual WCAG assessment remain separate from this engine/axe coverage.

`pnpm --dir web run test:security` collects eight actual loopback privacy sinks with 500 synthetic sentinels, source/dirty metadata, a mandatory leak positive control and explicit local-only scope. See [security collection](../tests/security/README.md). UI/security/lifecycle/OAuth outputs use separate `test-results/` subdirectories.

## Landing page, languages and media

The homepage is one component, `src/lib/components/Landing.svelte`, shown in two languages: English at `/` and
Korean at `/ko`. All words live in `src/lib/landing/en.ts` and `ko.ts` (typed by `copy.ts`); the numbers come from
`src/lib/landing.ts` with their source and caveats. Keep those numbers in sync with the README's "Current
evidence" section and keep the pilot wording. `pnpm test` checks the pilot figures against the README table and
that both languages have the same shape and no untranslated strings.

The page introduces MCP and account-free public reads, then shows three connection steps and an illustrative
URL/task → hint → action example before the demos and pilot figures. Sharing/ranks and a short FAQ follow.
The recall budget is **up to three hints, each at most 400 estimated tokens**; response metadata and detail
lookups are additional. Privacy copy describes what WAM requests include, without making promises about a
host's browser or model traffic. Keep the scripted-replay labels and the one-run-per-condition pilot caveat.

With the frontend running, `pnpm exec playwright test --config playwright.landing.config.ts` from `web/`
checks both languages at 1440, 390 and 320 pixels, keyboard-operated FAQ, guide navigation and claim wording.
It mocks APIs and writes screenshots to `web/test-results-landing/`, separately from the other browser suites.

`src/hooks.server.ts` sets `<html lang>` per page at render and prerender time. The landing page and its
shared header/footer are translated at `/ko`; other pages remain English.

Static files: clips and posters in `static/media` (`demo-*.mp4`, and `demo-*-ko.mp4` with Korean captions),
the social cards `static/og-image.png` and `og-image-ko.png`, the logo set in `static/brand`, and Latin subsets of
Manrope, DM Sans and DM Mono in `static/fonts` (SIL OFL, licence text included). They are produced by
`demo/` (see its README), `docs/assets/build_brand_assets.py` and `docs/assets/build_web_fonts.py`, not by the
web build. Set `VITE_SITE_URL` at build time so `og:image` and the `hreflang` links are absolute URLs.
