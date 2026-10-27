# Documentation

This documentation describes the intended product and the current state of the repository. Most implementation documents are plans; the root README explains which parts are not yet runnable.

## Sections

- [Feature specifications](spec/README.md): requirements and feature-level acceptance criteria
- [Design](design/architecture.md): system boundaries, storage, API, MCP, CLI, and web app
- [System overview](design/system-overview.md): the architecture in one picture and two short paths (Korean: [system-overview.ko.md](design/system-overview.ko.md))
- [Browser host integration](design/browser-host-integration.md): pre-implementation decisions and Chrome/Arise integration acceptance tests
- [Module boundaries](design/module-boundaries.md): testable module split and adapter contracts
- [Memory gatekeeper](design/memory-gatekeeper.md): contribution review, redaction, deduplication, verification, and publishing
- [Plan](plan/benchmark-readiness.md): milestone and benchmark preparation notes
- [Test strategy](test/README.md): functional, host, security, and benchmark plans
- [Test environment](test/environment.md): confirmed local tools, test results, and remaining gaps
- [Host smoke guide index](test/hosts/README.md): planned host-specific setup and smoke reports
- [User-flow tests](test/user-flows.md): end-to-end journey scenarios
- [Project history](history.md): background and design principles
- [Hackathon notes](hackathon/README.md): event-specific notes and demo ideas
- [Open-source readiness](open-source-readiness.md): items to resolve before a public release

The [reference directory](../reference/README.md) contains read-only snapshots copied from related repositories. It is not a standalone implementation or a guarantee that those files may be redistributed under this project's eventual license.
