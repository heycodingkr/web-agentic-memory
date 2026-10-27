# Contributing

Thanks for helping make reusable browser workflows safer and easier to share.

## Current scope

This repository is in its early design and scaffolding stage. The standalone service, web app, MCP server, and CLI are not implemented here yet. Design reviews, documentation improvements, and focused implementation proposals are welcome.

For changes to the planned implementation, use the responsibilities described in the README files under crates, web, migrations, and guides. Keep the shared schema and safety rules in the design documents current as code is added.

## Before opening a pull request

1. For substantial changes, open an issue or discussion first so the scope and design can be agreed.
2. Keep changes focused and update the relevant documentation.
3. For implementation changes, link the relevant test cases in docs/test and report exactly what you ran.
4. Use synthetic data. Do not add credentials, session cookies, account identifiers, private browsing data, unpublished page content, or screenshots from private accounts.
5. Do not copy code or assets into reference or other folders unless their source and redistribution rights are clear.

There is no repository-wide build or test command yet. Do not describe a change as tested unless a matching command or manual procedure was actually run.

## Pull request contents

Include the problem, the change, relevant documentation, verification performed, and any security or privacy impact. For benchmark changes, include the run count, setup, raw-data location, and limitations.

## Licensing

The project license is still undecided. Before a license is added, discuss substantial code contributions with the maintainers and do not assume that submitting a change grants broad reuse rights. Existing files in reference may have separate terms.
