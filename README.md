# Web Agentic Memory

[English](README.md) | [한국어](README.ko.md)

> Once someone has blazed a trail, every agent should be able to follow it.

Web Agentic Memory (WAM) connects to Codex, Claude Code and other MCP clients to share short, verified website workflows. Your agent asks for hints for a public URL and task, then acts with the browser tools it already uses.

**Public reads need no account.** Contributions, usage receipts and contribution credit require a linked WAM account. Connecting a memory MCP alone does not start background recall or install browser controls.

## Get started

### Connect to an existing WAM service

Use the service's `/mcp` URL. `https://wam.example/mcp` below is a placeholder: replace it with your deployment's URL and choose your host.

| Host | Register remote HTTP MCP | Check registration |
|---|---|---|
| Codex | `codex mcp add web-memory --url https://wam.example/mcp` | `codex mcp get web-memory --json` |
| Claude Code | `claude mcp add --transport http --scope user web-memory https://wam.example/mcp` | `claude mcp get web-memory` |

Ask the agent to call `web_memory_status`, then look up hints for the current public site and task. A successful registration is separate from a working tool call and account authorization. The website's `/guide` page walks through host selection, first recall, optional account linking and removal.

Remote account features use the host's OAuth flow when the service enables it. For local stdio, `wam link` uses device approval and the OS credential store. A browser/site login is separate from WAM authorization. See [account and host verification](docs/test/cli-link.md) and [remote OAuth setup](docs/test/remote-oauth.md).

### Run from this checkout

Install Rust and Cargo, then build and install both local tools:

```sh
git clone https://github.com/heycodingkr/web-agentic-memory.git
cd web-agentic-memory
cargo install --path crates/wam-cli --locked
cargo install --path crates/wam-mcp --locked
wam --version
```

Keep Cargo's bin directory on PATH. Linux builds use D-Bus development packages (`libdbus-1-dev` and `pkg-config` on Debian/Ubuntu). Linked credentials use macOS Keychain, Windows Credential Manager or Linux Secret Service; there is no plaintext credential fallback. A headless Linux system without a usable secure store can still make anonymous public reads.

Start the development API in a separate terminal:

```sh
cargo run -p wam-server --locked
```

The default is `http://127.0.0.1:3000`; local application data is stored in `.wam-data/`. Public reads need no development token. Verify the service and try the seeded example:

```sh
wam --endpoint http://127.0.0.1:3000 --anonymous status --json
wam --endpoint http://127.0.0.1:3000 --anonymous recall https://example.com/guide/editor --task compose-draft --keyword blog --json
```

To let an agent use the installed `wam-mcp` binary, choose one local registration command:

| Transport | Codex | Claude Code |
|---|---|---|
| Local stdio | `wam --endpoint http://127.0.0.1:3000 install codex --local` | `wam --endpoint http://127.0.0.1:3000 install claude-code --local --scope user` |

Use the API base URL for `wam --endpoint` and the `/mcp` URL for remote host registration. `wam install` defaults to remote HTTP; **`--local` selects stdio**. Add `--dry-run --json` to inspect the planned registration without changing host settings. The binaries must be installed together so the helper can find `wam-mcp` beside `wam`.

`wam guide print --host codex` or `wam guide print --host claude-code` prints instructions about when to recall; only adding `--write` updates the current project's `AGENTS.md` or `CLAUDE.md`. Follow the user's request to skip memory. Use the version ID returned by recall with `wam expand <id> --version-id <version-id> --level 2 --json` when more detail is needed.

For the web app, use the Node 22 and pnpm 10 versions configured in CI:

```sh
pnpm --dir web install --frozen-lockfile
pnpm --dir web dev
```

Open `http://localhost:5173` for the landing page, leaderboard and agent setup guide. Vite proxies API/MCP calls to the local server. [Web setup](web/README.md) and [server configuration](crates/wam-server/README.md) cover API origins, GitHub OAuth, production storage and trusted replay providers. Account linking requires configured authentication; it is optional for public reads.

### Remove a connection

Choose `wam uninstall codex` or `wam uninstall claude-code --scope user` to remove that host's `web-memory` entry. This preserves other hosts and their shared credential/cache. Use `wam unlink` to revoke the linked local device and `wam cache clear` to clear derived hints; use the same endpoint/config namespace as when linking. For remote OAuth, revoke the connection in the web app's Connected devices page. Source-installed binaries can be removed with `cargo uninstall wam-cli` and `cargo uninstall wam-mcp`.

## What is implemented

| Area | Current behavior |
|---|---|
| Public memory reads | URL/task/keyword recall, up to three hints, version-pinned detail, status and public leaderboard |
| MCP | Ten memory tools shared by stdio and stateless Streamable HTTP, including GET/POST SSE; endpoint/auth/consent/catalog status |
| Accounts | Device linking, OS credentials, host registration, remote OAuth discovery/DCR/PKCE, refresh and revocation |
| Contributions and credit | Explicit sharing consent, Gatekeeper and trusted replay, immutable publication, eligible author +20 and explicit successful-use +2, withdrawals and reversals |
| Storage and privacy | Transactional PostgreSQL or locked local snapshot, owner-only usage/analytics, shared anonymous cache, rate limits, retention and account erasure |
| Browser integration | Existing browser MCP plus WAM; optional persistent Arise bridge with explicit recall from its owned tab |

Local verification includes macOS credential storage, isolated Codex/Claude registration, synthetic OAuth flows, native Arise leased-tab recall, PostgreSQL/Redis integration and web/CLI/MCP regressions. These are development checks. Live GitHub account journeys, Claude/Arise Hub remote OAuth, other OS runtime acceptance, real signed-artifact installation and production load/release evidence remain separate gates. See [release acceptance](docs/test/release-readiness.md).

### Browser and data boundaries

WAM supplies hints; your browser MCP handles navigation, clicks and typing. Chrome users keep their chosen browser connector. Arise users can follow the MCP Hub setup or the optional `wam browser-mcp` bridge in the [Arise host notes](docs/test/hosts/arise.md). The bridge keeps one browser MCP connection alive, verifies its selected owned lease, normalizes the URL locally and recalls only when the agent calls its tool. It does not use Arise's private control socket directly.

WAM requests use a normalized public URL pattern, task codes and allowed keywords/actions. Page contents and free-form prompts are not included in WAM recall/contribution requests. Browser and model services handle their own data under their respective settings. Remote hosts can make tool arguments visible to their models before contacting WAM, so server-side normalization is not a device-wide privacy guarantee.

Each recall returns at most **3 hints, each at most 400 estimated tokens**. Response metadata and detail lookups are additional. Hints are reference material: inspect the current page, follow user instructions and submit contributions only with consent. Successful-use scores require an owned authenticated receipt and the version IDs actually used; anonymous or cached hints do not create credit.

## Demos

Run the [web app](web/README.md) to view the English landing at `/` or Korean landing at `/ko`, with the connection flow and a concrete hint example. The clips below are **scripted replays on local demo editors**, not recordings of a model completing the task through the current WAM MCP.

| Clip | English | Korean |
|---|---|---|
| Same blog task, side by side | [Watch](web/static/media/demo-compare.mp4) | [보기](web/static/media/demo-compare-ko.mp4) |
| Blog draft | [Watch](web/static/media/demo-blog.mp4) | [보기](web/static/media/demo-blog-ko.mp4) |
| Slide editor | [Watch](web/static/media/demo-slides.mp4) | [보기](web/static/media/demo-slides-ko.mp4) |

## Current evidence

The figures below are an earlier memory-hint pilot, not an end-to-end benchmark of the current WAM MCP or all host integrations. Slide tasks have not been benchmarked. Local test results establish specific contracts and fixtures rather than a general speed or token-saving guarantee.

A small pilot compared memory-assisted and memory-free browser tasks across real sites and controlled editor fixtures. Each condition ran once, and the execution order was not randomized. These results are early observations, not a general performance guarantee. See the [benchmark notes](docs/test/benchmarks.md) and the [evaluation source snapshot](reference/agent-browser/docs/specs/browser-agent-memory-evaluation.md).

The 2026-10-16 pilot compared the same task without memory (tools only) and with memory hints.
Model: Codex `gpt-6-luna`, effort `max`. Each condition ran in a fresh, isolated environment.
Claim grade: **C** (a single observation per condition, for reference only), as defined in [`docs/test/metrics.md`](docs/test/metrics.md) (Korean).

### Real sites (logged-in live editors; drafts saved, nothing published)

| Site | Memory | Time | Tool calls | Time change | Call change |
|---|---|---:|---:|---:|---:|
| Medium | None | 85.5 s | 10 | | |
| Medium | Injected | **44.3 s** | **7** | **−48%** | **−30%** |
| Naver Blog | None | 84.1 s | 16 | | |
| Naver Blog | Injected | **58.8 s** | **12** | **−30%** | **−25%** |

Quality changed too. Without memory, Medium ended up with one unnecessary empty paragraph. With memory injected, exactly the title and two paragraphs were saved. (Checked by reopening the edit URL in a fresh browser session.)

### Controlled editor fixtures (results checked by an independent save oracle)

| Editor | Memory | Time | Tool calls | Errors | Time change |
|---|---|---:|---:|---:|---:|
| Rich editor | None | 130.3 s | 16 | 0 | |
| Rich editor | Manual recall | 66.7 s | 12 | 0 | −49% |
| Rich editor | Auto-injection | **55.2 s** | **11** | 0 | **−58%** |
| iframe editor | None | 195.4 s | 28 | 3 | |
| iframe editor | Manual recall | **91.7 s** | 18 | 0 | **−53%** |
| iframe editor | Auto-injection | 130.9 s | 17 | 0 | −33% |

### Summary

| Metric | Range of improvement | Simple average over 4 conditions\* |
|---|---|---|
| Elapsed time | −30% to −58% | about −42% |
| Tool calls | −25% to −39% | about −31% |
| Errors | 3 → 0 on the iframe editor | |

\* Medium and Naver Blog (injected), plus the rich editor and iframe editor with auto-injection.

### Limitations, stated honestly

- Each condition was run **once**, and the run order was not randomized. This is a **pilot observation**, not a general latency guarantee.
- On the live Naver site, the second run met a different "continue writing" popup and a different number of saved drafts, so it is not a controlled A/B.
- On the iframe fixture, auto-injection was slower than manual recall. Auto-injection is not always the fastest.
- Problems on the site's own side, such as the server returning a 503, are not solved by memory. In that case we confirmed that the agent reports the failure accurately.
- **TODO:** Report means and variance from 3 to 5 repeated runs per condition, measured separately per host (Arise's native-memory auto-injection vs. Chrome MCP + WAM manual recall).

## Architecture and development

![Web Agentic Memory architecture overview](docs/assets/architecture-overview.png)

```text
Agent and existing browser tools
   | MCP or CLI: public URL pattern + task + allowed keywords
   v
Web Agentic Memory server (Rust)
   ├─ Approved per-site workflow catalog ──▶ recall: up to 3 hints, each ≤ 400 estimated tokens
   └─ Consented contributions ──▶ Gatekeeper + trusted replay ──▶ immutable publication
```

The Rust workspace contains `wam-core`, `wam-contracts`, `wam-client`, `wam-cli`, `wam-mcp`, `wam-server` and `wam-gatekeeper`. The web app is SvelteKit/TypeScript. PostgreSQL is the shared application store, Redis backs shared quotas, and local mode uses a locked snapshot and in-memory limiter. Model providers and trusted browser replay sit behind explicit interfaces.

```sh
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
pnpm --dir web test
pnpm --dir web check
pnpm --dir web build
```

Database, Redis, live-provider and native secure-store tests have their own environment requirements. Use the [test index](docs/test/README.md), [owned local data-service runner](tests/env/README.md) and [release checks](docs/test/release-readiness.md) for those scenarios. Tests and evidence must distinguish fixtures from real host/account acceptance.

## Why shared trails

Agents repeatedly rediscover the same site-specific details and lose that work between sessions. WAM aims to let one verified discovery help the next agent, while keeping sharing optional and recognizing the contributor. The [history and design principles](docs/history.md) explain the project's inspiration and source material.

## Documentation

| Document | What it covers |
|---|---|
| [CLI guide](crates/wam-cli/README.md) · [MCP guide](crates/wam-mcp/README.md) | Commands, credentials, host registration and tool contracts |
| [Web setup](web/README.md) · [Server setup](crates/wam-server/README.md) | Local development and service configuration |
| [System overview](docs/design/system-overview.md) · [Module boundaries](docs/design/module-boundaries.md) | Components, data flow and test seams |
| [Browser-host integration](docs/design/browser-host-integration.md) | Browser ownership and WAM account boundaries |
| [Specifications](docs/spec/README.md) · [Test index](docs/test/README.md) | Requirements and verification procedures |
| [Release acceptance](docs/test/release-readiness.md) · [Signed archive verification](docs/test/releases.md) | Evidence gates and safe downloaded-archive installation |
| [CONTRIBUTING.md](CONTRIBUTING.md) · [SECURITY.md](SECURITY.md) | Repository conventions and data handling |
| [Open-source readiness](docs/open-source-readiness.md) · [Reference notes](reference/README.md) | License, reference rights and publication scope |

<a id="roadmap"></a>

## Next milestones

- [x] Public recall, MCP/CLI and web onboarding
- [x] Consent-based contributions, deterministic screening and duplicate handling
- [x] Transactional publication, version attribution and score ledger
- [x] Local integration, browser fixtures and regression coverage
- [ ] Real model + WAM task recordings and repeated, host-specific measurements
- [ ] Live account, native OS and real signed-artifact acceptance
- [ ] Production service/load evidence and public release decisions

## License

A project license has not been selected yet. No license is granted for reuse until a license file is added. Files under `reference/` may have separate rights and must be reviewed independently. Source publication does not establish that a binary release has passed its acceptance gates.
