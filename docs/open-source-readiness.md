# Open-source readiness

This checklist tracks work required before presenting the repository as a licensed, reusable open-source project.

## Required before a public release

- [x] Select a project license and add its complete license text. Apache License 2.0, added on 2026-11-01 (see the decision note below).
- [ ] Audit every file under reference for source, commit, license, and redistribution rights. Remove or replace files that cannot be redistributed.
- [ ] Compare reference snapshots with their source revisions and review any uncommitted changes.
- [ ] Review tracked files for credentials, user data, private logs, and account-specific content.
- [ ] Confirm benchmark permissions and publish the sample count, setup, and limitations with every result.
- [ ] Confirm the security reporting contact and enable a private reporting path.
- [ ] Add verified build, setup, and test commands when a runnable implementation exists.
- [ ] Decide whether the first release includes only the open client and protocol, or also the hosted service and its operational configuration.

## License decision (2026-11-01)

The repository owner asked for the license and the code of conduct to be decided and added directly, so the Apache License 2.0 was chosen and `LICENSE`, `NOTICE` and `CODE_OF_CONDUCT.md` (Contributor Covenant 2.1) were added. Reasons: it is permissive and widely accepted for infrastructure software, it carries an express patent grant, it licenses contributions on the same terms (section 5), and it grants no trademark rights (section 6). The owner holds the copyright and can license later versions differently; people who already received a version under Apache-2.0 keep their rights for that version. The code of conduct names the owner's GitHub profile as its contact because no project mailbox exists; replace it if one is created.

`reference/` is excluded from the license in `NOTICE` and in the README because its source and redistribution rights are not verified. The repository is public (GitHub API, 2026-11-01), so those snapshots are already visible. The reference audit, the private security contact and the first release scope stay open above, and this decision does not complete gate REL-11.

## Source links before publication

On 2026-10-31 the authenticated GitHub API reported `heycodingkr/web-agentic-memory` as private; the same source URL returned 404 without authentication. The onboarding guide therefore offers no source download link by default. An operator-provided checkout remains usable, and remote MCP needs no source download.

Set `VITE_WAM_SOURCE_URL` only after the chosen repository passes the rights review above and returns HTTP 200 without authentication. This setting controls the link; it does not change repository visibility or complete any release gate. Making the existing repository public would also expose its Git history and reference snapshots, so that scope needs an explicit owner decision.

The repository has a local ignore rule for the attached hackathon PDF and MeshCode workspace state. Ignore rules protect new local files; they do not remove files already tracked by Git.

Do not publish or redistribute the reference snapshots until their rights review is complete.
