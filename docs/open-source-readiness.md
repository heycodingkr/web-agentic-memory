# Open-source readiness

This checklist tracks work required before presenting the repository as a licensed, reusable open-source project.

## Required before a public release

- [ ] Select a project license and add its complete license text.
- [ ] Audit every file under reference for source, commit, license, and redistribution rights. Remove or replace files that cannot be redistributed.
- [ ] Compare reference snapshots with their source revisions and review any uncommitted changes.
- [ ] Review tracked files for credentials, user data, private logs, and account-specific content.
- [ ] Confirm benchmark permissions and publish the sample count, setup, and limitations with every result.
- [ ] Confirm the security reporting contact and enable a private reporting path.
- [ ] Add verified build, setup, and test commands when a runnable implementation exists.
- [ ] Decide whether the first release includes only the open client and protocol, or also the hosted service and its operational configuration.

## Source links before publication

On 2026-10-31 the authenticated GitHub API reported `heycodingkr/web-agentic-memory` as private; the same source URL returned 404 without authentication. The onboarding guide therefore offers no source download link by default. An operator-provided checkout remains usable, and remote MCP needs no source download.

Set `VITE_WAM_SOURCE_URL` only after the chosen repository passes the rights review above and returns HTTP 200 without authentication. This setting controls the link; it does not change repository visibility or complete any release gate. Making the existing repository public would also expose its Git history and reference snapshots, so that scope needs an explicit owner decision.

The repository has a local ignore rule for the attached hackathon PDF and MeshCode workspace state. Ignore rules protect new local files; they do not remove files already tracked by Git.

Do not publish or redistribute the reference snapshots until their rights review is complete.
