# Contributing

Thanks for helping make reusable browser workflows safer and easier to share.

## Current scope

Web Agentic Memory is an early-stage project (workspace version 0.1.0) built for the Meta Global AI Developer Hackathon. This repository contains the Rust workspace (`wam-core`, `wam-contracts`, `wam-client`, `wam-cli`, `wam-mcp`, `wam-server`, `wam-gatekeeper`), the SvelteKit web app, the stdio and remote MCP server, and the CLI. Production acceptance, signed binary releases and live-account journeys are not finished; see [release acceptance](docs/test/release-readiness.md). Bug reports, documentation fixes, test improvements and focused implementation proposals are welcome.

Each part's responsibilities are described in the README files under `crates/`, `web/`, `migrations/` and `guides/`. Keep the shared schema, contracts and safety rules in `docs/` current when you change behavior.

## Before opening a pull request

1. For substantial changes, open an issue or discussion first so the scope and design can be agreed.
2. Keep changes focused and update the relevant documentation.
3. For implementation changes, link the relevant test cases in docs/test and report exactly what you ran.
4. Use synthetic data. Do not add credentials, session cookies, account identifiers, private browsing data, unpublished page content, or screenshots from private accounts.
5. Do not copy code or assets into reference or other folders unless their source and redistribution rights are clear.

Run the checks that match your change from the repository root:

```sh
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
pnpm --dir web test
pnpm --dir web check
pnpm --dir web build
```

The web app needs Node 22.17 or newer and pnpm 10 or newer. Database, Redis, live-provider and native secure-store tests have their own environment requirements; see the [test index](docs/test/README.md) and [tests/env/README.md](tests/env/README.md). This repository has no CI workflows, so these checks run locally. Do not describe a change as tested unless a matching command or manual procedure was actually run.

## Pull request contents

Include the problem, the change, relevant documentation, verification performed, and any security or privacy impact. For benchmark changes, include the run count, setup, raw-data location, and limitations.

## Licensing

The project license is still undecided. Before a license is added, discuss substantial code contributions with the maintainers and do not assume that submitting a change grants broad reuse rights. Existing files in reference may have separate terms.
