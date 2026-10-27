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
