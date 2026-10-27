# Browser visual editing

Users must be able to inspect the current embedded page as an image, click its visible editor, type through Chromium input, and distinguish text insertion from a saved draft.

## Dependency order and acceptance

1. CLI tool exposure: Codex's Arise MCP allowlist and instructions include browser_screenshot. Native desktop Computer Use remains disabled. Check the actual generated launch arguments, not only API-agent registration.
2. Screenshot output: the embedded browser captures its CEF surface and returns MCP image/png content. Preserve an explicit host error rather than replacing it with a PNG decode error. Images are called standalone, outside browser_script.
3. Visual click: browser_click advertises selector/ref, viewport CSS x/y, and normalized screenshot x_ratio/y_ratio. Exactly one form is accepted. Ratios use the latest page-only screenshot and are resolved against the live viewport; they are independent of image resolution/Retina scale.
4. Text entry: focus a visible editable target with a native click, retain the caret, and deliver text via Chromium keyboard input. Enter must be a keyboard Enter, not DOM paragraph cloning or execCommand. Reject non-editable focus. Await input completion; never repeat a whole insertion merely because saving is delayed.
5. Editing guidance: recall site know-how before substantial editor work; inspect a screenshot before input and after changes. Do not mutate contenteditable markup or internal IDs through browser_eval. Check visible text and explicit save status; a save-error banner prevents a saved claim. After repeated input failures, re-inspect focus and the screenshot before retrying.
6. Memory: save only verified steps with evidence; do not turn speculative site explanations into persistent facts. Generic editor mechanics live in shared instructions for API and CLI paths. Naver's blog_draft_input is Naver-only.

## Test inputs and expected results

- Codex launch with browser MCP: enabled_tools contains browser_screenshot; native desktop plugins remain off.
- Screenshot reply with ok=true and PNG base64: MCP content is an image with image/png and unchanged bytes. ok=false/error: retain the exact error.
- Click ratios (0.5,0.25), viewport 1000x800: CSS point (499.5,199.75); endpoints stay inside the viewport. Incomplete, mixed or out-of-range targets fail before dispatch.
- Rich-text fixture records key/input events and asserts that all keyboard events are trusted. Input title/body plus newline preserves the caret and produces separate paragraphs without duplicate internal identifiers.
- Input error / save-error banner: no claim of acceptance or saved status; no repeated full-text insertion.
- English and Korean API/CLI instructions require screenshots, native input, site recall, verified memory, and save verification.

## Live regression found during verification

The rebuilt CEF fixture accepted a screenshot-relative click and trusted English/Enter/Korean/emoji key events. The immediately following screenshot failed with `Could not submit browser screenshot`. CEF requires message IDs to increase across methods; independent feature counters (screenshot at 2,000,000 and keyboard at 3,000,000) caused CEF to reassign the next screenshot ID. Its observer then waited for the wrong ID. All internal CEF DevTools calls must share one sequence, allocated at dispatch time, including the second phase of file uploads. Re-run screenshot → input → screenshot → upload → screenshot to cover interleaving.

Reference: https://github.com/chromiumembedded/cef/blob/master/libcef/browser/devtools/devtools_controller.cc (ExecuteDevToolsMethod).

## Verification (2026-10-16)

- Test-first CLI regression reproduced the omitted screenshot tool; API guidance and normalized-click tests also failed before implementation.
- Browser MCP library: 174 passed, 2 ignored. Cadence: 30 passed. CEF: 140 passed. Chat: 687 passed, 3 ignored. LLM: 284 passed. Real MCP subprocess/socket integration: 11 passed.
- Rebuilt both application and MCP binaries. An isolated CEF app/profile ran `scripts/probe_visual_editor_live.py` against its explicit control socket. The script first reproduced the post-input screenshot failure above, then passed after the shared-ID fix.
- The live fixture returned actual PNG image blocks before/after native input and after file upload. Native keydown events were trusted, Enter created two distinct paragraphs, Korean/emoji were preserved, reselecting the focused editor retained its caret, and `browser_type_human` appended without replacing text. Both the after image and local fixture's explicit Saved status were inspected. Evidence: `/tmp/arise-visual-editor/result.json`, `before.png`, `typed.png`, `after.png`.
- This fixture verifies browser transport, image capture, native input and editor events. Its Saved label is local test state; it does not establish Medium server persistence. Live Medium verification still requires an authenticated draft in the rebuilt app. A screenshot of a past session cannot establish current save behavior.

Codex allowlist semantics: https://learn.chatgpt.com/docs/config-file/config-reference (mcp_servers.<id>.enabled_tools).
