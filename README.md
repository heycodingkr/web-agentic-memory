# Web Agentic Memory

[English](README.md) | [한국어](README.ko.md)

> Once someone has blazed a trail, every agent should be able to follow it.

Share verified website workflows so AI agents can complete browser tasks with fewer retries and tool calls.

## Project status

The first runnable vertical slice is implemented: pure Rust domain rules, shared JSON contracts, an injectable Axum API with a seeded catalog and file-backed account profiles, a shared HTTP client, local stdio MCP tools, a CLI for recall/rank and consent-gated contribution intake, and SvelteKit leaderboard/profile/rank/contribution screens. See [local development](web/README.md) and [module boundaries](docs/design/module-boundaries.md).

This is not a production service yet. Account profiles/public visibility and contribution submit/list/withdraw/review use single-process local JSON stores; the dashboard exposes private paginated score history; authenticated recall receipts and successful-use score events also persist to local JSON files. A successful use by another account adds 2 points to a known memory author and feeds the local leaderboard/rank. PostgreSQL transactions, approval-driven memory publishing and +20 approval awards, production Gatekeeper worker integration, OAuth/device production wiring, and production token storage remain incomplete. The separate Gatekeeper crate has a deterministic curation engine and fake-provider scenarios. The code under reference remains for study and migration planning and may have separate licensing and redistribution terms; review the open-source readiness checklist before publishing or reusing it.

The code under reference is kept for study and migration planning. It may have separate licensing and redistribution terms. Review the open-source readiness checklist before publishing or reusing it.

## The idea

Browser agents repeatedly rediscover the same site-specific details: how an editor handles paragraphs, where a save action appears, or how an iframe receives input. Web Agentic Memory aims to turn verified workflows into short, reusable hints.

An agent identifies the site and task, retrieves a small set of approved hints through MCP or HTTP, then uses its existing browser tools such as ChromeBridge or Arise. Contributions are opt-in, checked by deterministic safety rules and the Gatekeeper Agent, and sent to human review only for exceptions such as low confidence or conflicting evidence.

## The story

### 1945: A machine that hands trails to others

As World War II was drawing to a close, the science administrator Vannevar Bush wrote "As We May Think" for *The Atlantic*. It grew out of a worry that people could no longer keep up with the flood of knowledge. He sketched an imaginary machine called the Memex. It linked one document to another by association, left those links behind as a **trail**, and let you **hand that trail to someone else**. He also foresaw that a new profession would arise, people who lay down such trails: "trail blazers."

### 1989: Knowledge that vanishes

Forty-four years later, Tim Berners-Lee at CERN wrote a proposal. It began not with a grand vision but with **information loss**. When a researcher left, what they knew left with them. His boss is said to have sent the proposal back with "Vague, but exciting" written on it. In 1993, CERN released the web technology into the public domain, and an open web that anyone could join spread across the world. The web became a place where people follow the trails they leave for one another.

### 2001: The dream of agents

In 2001, Berners-Lee and his co-authors imagined, in "The Semantic Web," agents that would get things done on the web on people's behalf. But that dream depended on **countless sites becoming machine-friendly**, and most sites had no reason to.

### The people who paved the way

When the World Wide Web opened, the world changed enormously. Knowledge came out of the library, and a single piece of writing left by a stranger could change someone's day. That change was possible because someone **paved the way first and gave it away for nothing**. Web technology was released into the public domain, anyone could open a page's source, and people learned by watching each other's work. We grew up on that path.

This project began as a thank-you to those people. Now there is a new kind of being that walks the web: the agent. We wanted to keep the work of paving the way open, and to make sure **the people who pave it are not forgotten**.

### Now

Twenty-five years later, agents really do use the web. But they don't wait for sites to change. They use **the screens built for humans, exactly as they are**, with eyes and hands. So they get lost on every site they visit for the first time. The Save button is off-screen, the editor sits inside an iframe, a popup blocks the way. What hurts more is that when the session ends, **what the agent learned disappears too**. It is the same problem as in 1989.

Web Agentic Memory gives the 1945 idea back to agents. When someone's agent gets lost once and finds the way through, that trail is verified and shared, and the next agent follows it. It asks nothing of the site. All it needs is the experience agents have observed.

And the person who first blazed the trail keeps their name on it. Each time a memory is approved and someone else succeeds by following it, the contribution is recorded and climbs the leaderboard. The design aims to give back, this time from the start, the recognition that the people who opened the web should have received.

A fuller lineage (Nelson, Engelbart, wikis and open source, robots.txt, Stack Overflow, MCP) and how it carries through into our design principles is written up in [`docs/history.md`](docs/history.md) (in Korean).

> The years and anecdotes are summaries based on public sources. Before publishing externally, re-check the citations against the source list in section 11 of `docs/history.md`.

## Why we are building this

Browser agents **inevitably get lost on a site they are seeing for the first time.**

- The editor lives inside an iframe, so the agent can't give it focus.
- The Save button is off-screen, so the agent clicks the wrong coordinates.
- Input is rejected unless it arrives as a "trusted" event.
- A popup like "Continue where you left off?" blocks the way every time.
- Medium uses NBSP, and one Enter between paragraphs is all it takes; typing `\n\n` leaves an empty paragraph.

These trials and errors are **fixed per site**. A site's UI is the same no matter who the user is.
Yet today every user, every agent, and every session repeats them from scratch.
Each wrong turn costs time, adds tool calls, burns tokens, and sometimes ends in failure.

## Why many people have to build it together

One person's private memory is not enough.

| Reason | Explanation |
|---|---|
| **Cost sharing** | Working out a site for the first time costs `C`, and each of `N` users pays it separately (`N × C`). Shared, it is paid once (`C + N × hint lookup cost`). A lookup returns a hint of 400 tokens or fewer, far cheaper than `C`. |
| **Long tail** | People use tens of thousands of sites. Any one person visits only a few of them, so a personal memory is empty for most sites. Shared, "a site I'm visiting for the first time" is usually "a site someone has already worked out." |
| **Freshness** | Site UIs change all the time. When one person meets a changed UI and finds a new route, everyone benefits at once. On your own, you are also slow to notice that anything changed. |
| **Verification** | When several independent contributors observe the same route, confidence goes up. This keeps one person's lucky success from being mistaken for a general rule. |
| **Network effect** | More contributors widen coverage, wider coverage draws more users, and more users bring more contributions. |

The point is **"learn it once, everyone uses it."**

## Current evidence

A small pilot compared memory-assisted and memory-free browser tasks across real sites and controlled editor fixtures. Each condition ran once, and the execution order was not randomized. These results are early observations, not a general performance guarantee. See the [benchmark notes](docs/test/benchmarks.md) and the [evaluation source snapshot](reference/agent-browser/docs/specs/browser-agent-memory-evaluation.md).

How much faster does an agent get with memory? This is the 2026-10-16 pilot measurement: we compared the same task done without memory (tools only) and with memory injected.
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

### At scale (simple arithmetic, not a measurement)

Assume the pilot's Medium saving (about 41 seconds) holds. If 1,000 users each write one post on Medium, that saves **about 41,000 seconds (about 11.4 hours)**. This is for one site and one task type. The number multiplies as sites and task types are added.

### Limitations, stated honestly

- Each condition was run **once**, and the run order was not randomized. This is a **pilot observation**, not a general latency guarantee.
- On the live Naver site, the second run met a different "continue writing" popup and a different number of saved drafts, so it is not a controlled A/B.
- On the iframe fixture, auto-injection was slower than manual recall. Auto-injection is not always the fastest.
- Problems on the site's own side, such as the server returning a 503, are not solved by memory. In that case we confirmed that the agent reports the failure accurately.
- **TODO:** Report means and variance from 3 to 5 repeated runs per condition, measured separately per host (Arise's native-memory auto-injection vs. Chrome MCP + WAM manual recall).

## How it works (design)

![Web Agentic Memory architecture overview](docs/assets/architecture-overview.png)

The picture shows how the queue, the Gatekeeper (a curator model boxed in by server-side rules), the model providers, and the memory catalog fit together. The [system overview](docs/design/system-overview.md) walks through the two paths (recall and contribute), the memory layers, how to swap the model, and what is not wired yet.

In short:

```
Agent (Arise, Chrome, etc.)
   │  MCP or instruction guide
   ▼
Web Agentic Memory server (Rust)
   ├─ Approved per-site workflow catalog ──▶ recall: up to 3 hints, ≤ 400 tokens
   └─ Contributions ─▶ Memory Gatekeeper ─▶ screen · dedupe · verify by measurement · publish
                               ▲
                               └─ Gatekeeper Agent decides routine cases; people review exceptions
```

- **Share**: When an agent finishes a task (only if the user has consented), it submits what it observed in a closed schema. Prompts, typed text, selectors, screenshots, and full URLs are not sent.
- **Memory Gatekeeper**: Removes API keys and personal information with deterministic rules, admits only what differs from existing memories, and publishes only what has shown an improvement in a measured A/B test.
- **Inject**: Arise can auto-inject its own browser memory; the WAM integration is planned through Arise's MCP hub. Other hosts, such as Chrome, use WAM through MCP tool calls or the instruction guide.
- **Leaderboard**: Only logged-in users can contribute; anonymous users can only recall. When a memory you created is approved and a different logged-in user uses it successfully, you earn contribution points. Self-use, anonymous use, and repeated use of the same memory by the same user on the same day earn nothing.

## Tech stack

| Area | Choice | Why |
|---|---|---|
| Server and CLI | Rust (axum, tokio, sqlx) with PostgreSQL and pgvector | A small, read-heavy API with low latency and memory use. The server, worker, MCP bridge, and CLI share one core crate |
| Gatekeeper | A Rust worker that calls the Meta model directly over the Meta Model API (OpenAI-compatible HTTP) behind a trait | No Node SDK is needed, and the security boundary stays small |
| Node | Only if needed, as an isolated queue worker | SDK-only features or ported TypeScript |
| Web | SvelteKit (TypeScript) | Reuses the existing Svelte admin screen and static deploy flow, with small bundles |
| API contract | OpenAPI generated from the Rust types | No type drift between the server and the web app |
| MCP | Remote Streamable HTTP by default, plus a local stdio bridge | One tool contract for both transports |

The full design is in [`docs/design/architecture.md`](docs/design/architecture.md) (Korean). Browser-host boundaries and pre-implementation acceptance tests are in [`docs/design/browser-host-integration.md`](docs/design/browser-host-integration.md) (Korean).

## Install (planned)

Nothing is installable yet. The planned paths let people set the memory up from Claude Code or Codex CLI with at most two commands ([`docs/spec/install.md`](docs/spec/install.md), Korean):

| Form | Claude Code | Codex CLI |
|---|---|---|
| Remote MCP, no local install | `claude mcp add --transport http web-memory <MCP endpoint>` | `codex mcp add web-memory --url <MCP endpoint>` |
| Local `wam` binary (stdio MCP and CLI) | install `wam`, then `wam install claude-code` | install `wam`, then `wam install codex` |

Anonymous users can only recall. Contributing needs a login: OAuth inside the host for the remote MCP, `wam link` for the local binary. The remote commands use the syntax verified against Claude Code 2.1.310 and Codex CLI 0.164.0 on 2026-10-27, and `<MCP endpoint>` comes from the deployment. Test scenarios for every form are in [`docs/test/tool-forms/README.md`](docs/test/tool-forms/README.md) (Korean).

## Test environment

Tests run in two browser environments, Chrome first and then an agentic browser:

| Order | Environment | How memory reaches the agent |
|---|---|---|
| 1 | Chrome with a browser MCP | Through MCP tool calls or the instruction guide (manual recall) |
| 2 | [Arise Browser](https://arisebrowser.com), an agentic browser | Through its native browser MCP and MCP hub; WAM integration is planned |

Results will be reported separately for each environment (see [Current evidence](#current-evidence)). Setup and smoke-test notes for these hosts, and for Claude Code, Codex CLI, Meta Muse CLI, and generic MCP clients, are indexed in [`docs/test/hosts/README.md`](docs/test/hosts/README.md) (Korean). The overall test strategy is in [`docs/test/README.md`](docs/test/README.md) (Korean).

### Arise and Chrome integration

[Arise Browser](https://arisebrowser.com) already provides its own stdio browser MCP and an MCP hub for external servers. The planned WAM connection is to add WAM as an external MCP in that hub. Arise's browser bridge talks to its app over a native local control channel (Unix-domain socket on Unix-like systems); this is not CDP. WAM should not depend directly on that internal socket. Automatic tab-aware WAM recall still needs an Arise host adapter and is not implemented.

Chrome users connect a separate browser MCP such as ChromeBridge or Chrome DevTools MCP. CDP belongs to that local browser connection; it is not a requirement for the remote WAM MCP itself.

## Demo

We plan to record demo videos in [Arise Browser](https://arisebrowser.com), such as editing slides in Google Slides and writing a blog post on Medium, and add them here. The demo scenario draft is in [`docs/hackathon/README.md`](docs/hackathon/README.md) (Korean).

### Editing slides in Google Slides

_Video: coming soon._

### Writing a blog post on Medium

_Video: coming soon._

<!-- Add more demo subsections, and the video links or embeds, here once the videos are uploaded. -->

## Repository layout

- crates: planned Rust core, server, gatekeeper, MCP adapter, and CLI
- web: planned public site and administration UI
- migrations: planned database schema changes
- guides: planned host setup and agent instruction guides
- benchmarks: public, reproducible result summaries
- demo: sanitized demo scripts and fixtures
- tests: cross-component tests as implementation begins
- docs: architecture, security, testing, planning, and project history
- reference: read-only source snapshots from related projects

The Rust workspace currently builds wam-core, which contains pure normalization, index validation, and deterministic recall modules. The server, gatekeeper, MCP bridge, CLI, and web app remain scaffolding.

## Documentation

Most of the documents below are written in Korean. `docs/README.md`, `docs/design/system-overview.md`, and `docs/open-source-readiness.md` are in English.

| Document | Contents |
|---|---|
| [`docs/README.md`](docs/README.md) | Documentation index |
| [`README.ko.md`](README.ko.md) | Korean version of this README |
| [`docs/design/system-overview.md`](docs/design/system-overview.md) | Architecture overview: read and write paths, queue, curator, model providers, memory layers, trust boundaries (Korean: [`system-overview.ko.md`](docs/design/system-overview.ko.md)) |
| [`docs/design/architecture.md`](docs/design/architecture.md) | Tech stack, server / web / MCP layout, leaderboard, deployment |
| [`docs/design/module-boundaries.md`](docs/design/module-boundaries.md) | Testable module boundaries and adapter contracts |
| [`docs/design/memory-gatekeeper.md`](docs/design/memory-gatekeeper.md) | Memory review and validation: screening, deduplication, reproducibility checks, publication gate |
| [`docs/test/README.md`](docs/test/README.md) | Test strategy: functional tests, host smoke tests (Claude / Codex / Muse, etc.), security scenarios |
| [`docs/test/security-scenarios.md`](docs/test/security-scenarios.md) | Security test scenarios |
| [`docs/spec/README.md`](docs/spec/README.md) | Functional requirements and acceptance criteria |
| [`docs/spec/install.md`](docs/spec/install.md) | Install and host registration for Claude Code and Codex CLI |
| [`docs/spec/cli.md`](docs/spec/cli.md) | The `wam` command surface and automation contract |
| [`docs/test/tool-forms/README.md`](docs/test/tool-forms/README.md) | Test scenarios per tool form (MCP, CLI, snippet), classified by form, host, and phase |
| [`docs/test/hosts/README.md`](docs/test/hosts/README.md) | Index of per-host setup and smoke-test documents |
| [`docs/test/user-flows.md`](docs/test/user-flows.md) | User-journey test design |
| [`docs/history.md`](docs/history.md) | Intellectual lineage: Bush, Berners-Lee, the Semantic Web, robots.txt, MCP, and how it translates into design principles |
| [`docs/hackathon/README.md`](docs/hackathon/README.md) | Hackathon notes, demo scenario |
| [`docs/open-source-readiness.md`](docs/open-source-readiness.md) | Items to check before a public release |
| [`reference/README.md`](reference/README.md) | Notes on the read-only reference snapshots |

## Roadmap

- [ ] M1: Contribution schema, deterministic redaction, sentinel-secret tests
- [ ] M2: Novelty / duplicate detection
- [ ] M3: Rust server + host-neutral MCP + guide
- [ ] M4: Web app and leaderboard
- [ ] M5: Wire up automated A/B verification; publish repeated-measurement results
- [ ] Demo videos: record demos in Arise Browser (editing slides in Google Slides, writing a blog post on Medium) and add them to the [Demo](#demo) section

## Contributing

Contributions to the design and documentation are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for the current scope and repository conventions. A standalone development setup will be documented when the implementation lands.

## Security

This repository is not a production service. Please do not include credentials, private browsing data, account identifiers, or unpublished page content in issues or contributions. See [SECURITY.md](SECURITY.md).

## License

A project license has not been selected yet. No license is granted for reuse until a license file is added. Some files in reference may have separate rights and must be reviewed independently.
