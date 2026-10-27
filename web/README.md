# Web application

SvelteKit 3 static frontend. The public memory catalog/detail, leaderboard, public handle profile, personal rank, profile editor, and local contribution receipt flow call the typed API in src/lib/api.ts. Bio content uses a closed block schema; the renderer never inserts HTML. The editor offers paragraph/title/list, bold/italic, and HTTPS inline/service links.

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

This remains a development slice: the catalog is seeded in memory, while account/profile visibility, contribution, usage, and score records use local JSON stores. The dashboard reads the owner-only paginated score history at GET /v1/me/score-events. Manual review does not publish memory versions, and there is no production PostgreSQL transaction or worker. The server now exposes stateless Streamable HTTP at `/mcp` for anonymous reads and scoped existing credentials; standard remote MCP OAuth discovery, SSE, and verified host installation remain incomplete. The homepage and browser host adapters remain incomplete.
# Setup guide endpoint

The guide creates Codex and Claude Code remote HTTP MCP snippets. It uses `VITE_WAM_MCP_ENDPOINT` when configured at build time; otherwise it points local development at `http://127.0.0.1:3000/mcp` and production pages at their same-origin `/mcp` route. Configure the reverse proxy so that this public route reaches `wam-server`.

The endpoint is public for anonymous read tools. Private tools still require WAM credentials and the appropriate scope. The current endpoint has no standard OAuth discovery, so do not describe copy/paste as account linking or host installation completion.

## Landing page, languages and media

The homepage is one component, `src/lib/components/Landing.svelte`, shown in two languages: English at `/` and
Korean at `/ko`. All words live in `src/lib/landing/en.ts` and `ko.ts` (typed by `copy.ts`); the numbers come from
`src/lib/landing.ts` with their source and caveats. Keep those numbers in sync with the README's "Current
evidence" section and keep the pilot wording. `pnpm test` checks the pilot figures against the README table and
that both languages have the same shape and no untranslated strings.

`src/hooks.server.ts` sets `<html lang>` per page at render and prerender time. Only the landing page is
translated; the header, footer and every other page stay English.

Static files: clips and posters in `static/media` (`demo-*.mp4`, and `demo-*-ko.mp4` with Korean captions),
the social cards `static/og-image.png` and `og-image-ko.png`, the logo set in `static/brand`, and Latin subsets of
Manrope, DM Sans and DM Mono in `static/fonts` (SIL OFL, licence text included). They are produced by
`demo/` (see its README), `docs/assets/build_brand_assets.py` and `docs/assets/build_web_fonts.py`, not by the
web build. Set `VITE_SITE_URL` at build time so `og:image` and the `hreflang` links are absolute URLs.
