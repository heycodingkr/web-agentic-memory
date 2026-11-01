# Google Flow — video workflow benchmark

Status: **not measured**. This session completed an access check on 2026-11-01 and did not reach the authenticated video editor.

## Access check

The available Flow QA browser profiles showed the public Flow landing page and a sign-in path. No authenticated creation UI was available. No project or video was created, no prompt was submitted, and no Flow credits were used.

Google's [Flow Help Center](https://support.google.com/flow/answer/16353333?hl=en) says current model costs are shown in the product and that credits may be deducted when generation begins. It also says failed or safety-filtered generations are refunded within an hour. The [Google Flow product page](https://flow.google.com/) lists text-to-video, frames-to-video and other creation features; this is feature discovery, not direct UI verification.

## Metrics

| Condition | n | Success | Time to submit | Generation wait | Tool calls | Credits |
|---|---:|---:|---:|---:|---:|---:|
| Tools only | 0 | — | — | — | — | 0 |
| Manual WAM recall | 0 | — | — | — | — | 0 |
| Automatic injection | 0 | — | — | — | — | 0 |

No performance comparison or speed claim can be made from this access check.

## Required before measurement

Google's [Terms of Service](https://policies.google.com/terms?hl=en) restrict automated access where it conflicts with a page's machine-readable instructions. This session did not verify Flow's current robots rules or service-specific automation permission, so neither is treated as approved.

- Use a disposable, dedicated Google test account in an isolated browser profile; do not use a personal or work account.
- Confirm the account can access Flow and the chosen model in its region. Record only eligibility and the displayed cost, never account identifiers.
- Verify current service automation terms and machine-readable access rules. If automation is not allowed or is unclear, use manual observation only.
- Set a no-top-up credit cap before any generation. Count the Phase 0 learning clip in the total budget. Stop if the available balance cannot cover the preregistered runs while preserving the documented daily headroom.
- Complete and commit a preregistration from [the benchmark template](../prereg/_template.md) before measuring. Compare tools-only, manual recall and automatic injection with a fixed benign prompt and the same model/settings; use at least three runs per condition or report an explicitly preliminary result.
- Measure agent activity through request submission separately from server generation wait. Independently confirm a completed clip in the project; a spinner or Pending state is not success.

A Flow-specific example memory is bundled locally as an **Example** and is based on public product/help information. It is not firsthand UI verification, a reviewed public memory, or benchmark evidence.
