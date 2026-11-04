# Canva — Arise live workflow benchmark

Last verified: 2026-10-31 · Issue #103 · **R7 / Arise 0.1.60: T and M passed the full task and real-file checks; one observation showed lower time and call count for M**

## R7 — Full live task after the 0.1.60 deployment

Commit `3fe67c6`, which includes the focus fix `30bc7b3`, was deployed to all three operating systems, with the Mac GUI and bundled MCP updated together. Public downloads, feed signature checks, and path validation passed; the deployment record is `efb9277`. Both conditions used a new `gpt-6-luna/max` conversation with a 900-second limit. We disabled both automatic-memory switches in the actual inherited XDG settings and confirmed in the runner's user records that automatic Workspace history and Browser know-how were absent. We compared the input-field copyback with the runner's original user input (T: 1,276 characters; M: 7,544 characters). M received actual WAM recall results and three version-pinned L2 items.

| Condition | Result | External tool boundaries | Runtime | Observed errors |
|---|---|---:|---:|---|
| T | Passed the template, two-page editing, image replacement, PNG/PDF export, reopen, and independent file checks | 123 | 473.976 sec | 0 renderer/focus/stale markers |
| M | Passed the same full task and independent file checks | 106 | 423.193 sec | One stale-reference rejection and retry; 0 renderer/focus markers |

Both conditions had 0 explicit memory calls and 0 detected eval-change patterns. The actual tool sources contained only allowed browser actions, and the download lookup was actually called. T made no new upload call and used the same QA image already in Uploads; do not interpret T as validation of the new-upload path. In M, `browser_upload` was actually called with the specified QA PNG and replaced the existing center image. M closed the Pro prompt for PNG compression and exported with the free default settings. No purchase or trial was started.

An independent verifier reopened the design in a new tab and checked the text on both pages, the saved state, and replacement of the existing image on the right side of page two. We matched the actual download records' `complete`, `file_exists`, and path fields to the files, then copied them to the local evidence folder.

| Artifact | Actual file size | Independent check |
|---|---:|---|
| T PNG | 449,432 bytes | 1920×1080; PAGE TWO, Global workflow memory test, green diamond; no PAGE ONE |
| T PDF Standard | 431,803 bytes | `pdfinfo` confirmed one page; `pdftotext` confirmed the text; the rendered image showed the same second page as the PNG |
| M PNG | 210,963 bytes | 1920×1080; PAGE TWO, Global workflow memory test, centered green diamond; no PAGE ONE |
| M PDF Standard | 117,269 bytes | `pdfinfo` confirmed one page; `pdftotext` confirmed the text; the rendered image showed the same second page |

We opened and inspected the rendered PNG and PDF images from both conditions in an image tool. M was also reopened in a separate tab to confirm both saved pages and replacement of the existing center image. The full task, including downloads and reopening, finished within 900 seconds. We checked the card title and ID before moving T's new QA design to Trash, then confirmed there were no matching links for its ID in the newly opened Projects view. We did the same for M's QA design. We confirmed the exact filenames, 740-byte size, and green-diamond image of all four identical QA uploads, including the earlier preparation copies, before moving them to Trash. QA downloads and evidence copies were retained.

M took 50.783 seconds and 17 external tool boundaries fewer than T. However, each condition ran once, in fixed T→M order, with shared login and Uploads state, and the selected free templates differed. M uploaded a new image while T used the same image already present. The observed difference does not establish a causal memory effect or a general savings rate. This was not an automated Arise–WAM integration test.

Last `backend token_count / total_token_usage` values reported before completion:

| Condition | Cumulative input | Cached input | Output | Total |
|---|---:|---:|---:|---:|
| T | 14,234,122 | 13,865,472 | 27,574 | 14,261,696 |
| M | 17,571,418 | 17,303,552 | 33,919 | 17,605,337 |

These cumulative reported values include repeated context and are not billed costs. M used less time and fewer calls, but its reported token counts were higher, so this does not show an overall cost reduction. After the test, we confirmed that both automatic-memory switches were restored to their original `true/true` state in the actual XDG path and the default settings path.

Local evidence: `target/e2e-logs/bench/productivity-live103-r7/` contains `canva-{T,M}-result.json`, per-condition input validation, `canva-{T,M}-download-records.json`, `canva-{T,M}-artifact-oracle.json`, reopen JSON, `canva-{T,M}-cleanup.json`, `canva-{T,M}-artifacts/`, `canva-QA-upload-cleanup*.json`, `canva-retrieval.json`, `token-usage-and-source-audit.json`, and `settings-restored.json`. Original account screens and full run logs are not included in the public repository.

## Earlier R5 run conditions

The task was performed in a new conversation in the Arise app using a logged-in Canva account. The model was `gpt-6-luna/max`, with a 900-second limit per condition. We prepared to disable automatic personal/WAM recall, but the actual app used a temporary XDG settings path where automatic memory was enabled. Workspace history appeared in the actual user input for both runs, so **the R5 effectiveness comparison is invalid**. Explicit memory tools were prohibited. T received only the task; M also received three hints from actual local WAM recall and a version-pinned L2 lookup. This was not an automated Arise–WAM integration test.

The host's Unicode input dropped text in 20-character chunks, so we discarded the R4 comparison. For R5, we copied the local textarea with native copy and pasted it into chat, then compared the input-field copyback and the runner's actual user record against the original before sending. The full request was delivered accurately in both R5 conditions. Input accuracy alone does not establish a memory-free control, so R6 separately verifies the actual app switches and absence of automatic injection.

## Fixed task

1. Prepare a new QA design with two pages using a free template.
2. Change the text on page one to `WAM QA PAGE ONE`; change page two to `WAM QA PAGE TWO` and `Global workflow memory test`.
3. Replace the existing image on page two with the provided green-diamond QA PNG.
4. Export page two only as PNG and PDF Standard, and check download completion and the actual file paths.
5. Reopen the design and check the edits. An independent verifier directly checks the PNG and one-page PDF. Clean up only the design created by this run. Do not share, publish, or purchase anything.

## R5 observations

| Condition | Result | Actual external tool boundaries | Limits and errors |
|---|---|---:|---|
| T | Timed out; PNG completed within the limit, PDF completed after it | 220 (219 responses; 197 within 900 sec) | 900-second limit; stop action delayed by about 79 sec; 0 stale markers, 5 editable-focus markers |
| M | Timed out; editing incomplete, 0 downloads | 147 (145 within 900 sec) | Stop action delayed by about 20 sec; 0 stale markers, 5 editable-focus markers |

T had 0 additional memory calls and 0 detected DOM-change patterns. The PNG was recorded 887.725 seconds after the run started; the PDF was recorded after 950.166 seconds. The operator requested a stop at 978.859 seconds. File timestamps may include download-recording delays, so they are not treated as model completion times. Before the stop, the runner did not complete its own reopen check or final report.

An independent verifier opened the actual PNG and rendered PDF and confirmed the page-two text, green diamond, and absence of page-one content. `pdfinfo` confirmed that the PDF had one page. The artifacts were correct, but **the run does not count as full-task success within 900 seconds**. We moved the new QA design to Trash and confirmed there were no matching links in the new Projects view. Test files were retained in the local evidence folder.

In the actual 0.1.58 app, the Codex worker also could not find `browser_dialog` or `browser_downloads`. Issue #124 tracked their omission from the CLI allowlist, despite their presence in the MCP implementation. We checked fix `3ed8215` and 70 regression tests, then prepared 0.1.59. M's new QA design was also moved to Trash, and we confirmed there were no matching links in the new Projects view. Both T and M failed the full task, and automatic-memory isolation failed, so the call-count difference is not counted as an improvement.

Error counts were recalculated after excluding catalog-only tool-description output. The next run should follow the fix for the native input-focus regression found in 0.1.59.

## Retrieved memory and interpretation limits

- `web:example-canva-template`
- `web:example-canva-replace-content`
- `web:example-canva-export-pages`

This was a pilot with one run per condition, fixed T→M order, and the same logged-in account. We do not claim call reductions or general speed improvements by selecting only successful steps. Full logs, account screens, and original downloads are not included in the public repository.

## Earlier runs and evidence

R3 (0.1.57) reached only the login screen and was excluded from editing-performance measurement. R4 T was an invalid run stopped because of input loss; the QA design it created was cleaned up.

Local evidence: `target/e2e-logs/bench/productivity-live103-r5/` contains `canva-retrieval.json`, per-condition input validation and aggregation JSON, and `canva-T-artifacts/`. Deployment source `e2497ef`, tag `v0.1.58`, and deployment record `4918312` include confirmation-dialog fix `f525a6d`.
