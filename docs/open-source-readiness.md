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

The repository has a local ignore rule for the attached hackathon PDF and MeshCode workspace state. Ignore rules protect new local files; they do not remove files already tracked by Git.

Do not publish or redistribute the reference snapshots until their rights review is complete.
