# Demo day video (Meta Global AI Developer Hackathon)

A 2:50, 1080p, **silent** video with burned-in English captions, rendered from `stage.html`. The scene plan, the claims it may make
and the submission checklist are in [`docs/hackathon/demo-video.md`](../../docs/hackathon/demo-video.md).

**v2 story (2026-11-02):** people work at URLs, and what they do at each URL is a short list. A URL is the best index for work
memory, so WAM lets people share work memory per URL. v1 (2026-11-01) was Muse-first; its scenes are listed in the guide for reference.

```sh
node demo/video/build.mjs --at 3,65,104      # PNG stills in /tmp/wam-video/stills (check a layout in seconds; no real footage)
node demo/video/build.mjs --from 44 --to 62  # render one range, real clips included (checks the overlays quickly)
node demo/video/build.mjs --out demo/out/video/wam-demo-day-v2.mp4   # full render, about 4 minutes
# -> the mp4, a .en.srt and a -poster.jpg next to it   (demo/out is git-ignored)
```

Needs Google Chrome (Playwright's `chrome` channel, taken from `web/node_modules`) and `ffmpeg`. The stage is a pure function of time
(`window.__render(t)`), so a render does not depend on machine speed. Output is H.264, BT.709 limited range, 30 fps. Real footage is
composited in a second ffmpeg pass: `stage.html` lists each clip in `window.__overlays` (source window, speed, crop, privacy mask,
destination box, time range) and draws a dark hole where it goes. The poster is extracted from the final composited MP4; a partial
render uses the preferred poster time when it falls inside the range, otherwise the range midpoint. The clips live in
`demo/out/slides-real/` (not committed).

Run the renderer option regressions with `node --test demo/video/render-options.test.mjs`.

## What is real and what is a drawing

| Scene | Time | Source |
|---|---|---|
| 1 People work at URLs | 0:00 | **A drawing.** Three URL cards (Gmail, Instagram, Google Slides) with simplified task lists, labeled "Illustration". Then the "every agent relearns, then forgets" rows, also an illustration |
| 2 A URL is the index | 0:22 | **Real:** `captures/recall-slides.txt` is the output of `wam recall` against the public service (ids shortened, labeled). The right-hand clip is the Arise chat of a real run calling WAM over MCP (`M-raw.mp4`, 2.5× speed, chat panel only) |
| 3 A real run | 0:45 | **Real footage:** an Arise 0.1.64 + `gpt-6-luna` (max effort) run of one prompt, no memory (`T-raw.mp4`, 12×). The clock and tool-call count come from the agent log (`captures/run-slides.json`); milestone times are read off the recording and marked ≈ |
| 4 Measured | 1:36 | **Headline: the earlier 2026-10-16 pilot, exactly as the landing page states it** (mean of each row's relative change over the three English rows, Medium, rich-text editor, iframe editor: −46% time, −34% tool calls, tool errors 3 → 0). Labeled **BEFORE**. `stage.html` recomputes it from the raw rows and fails the render if it drifts from 46/34. Below it the two live-site observations of 2026-10-31, labeled LIVE: Calendar (R8) and Canva (R7), one run per condition |
| 5 Share by CLI or MCP | 1:52 | `captures/propose-help.txt` is the real `wam propose --help` (unneeded lines trimmed, labeled). The Muse Spark panel is the unedited `captures/curate-all.txt` run from 2026-11-01 plus the numbers in [`meta-model-usage.ko.md`](../../docs/design/meta-model-usage.ko.md) |
| 6 Leaderboard | 2:12 | **Real:** `captures/leaderboard.png` is the live page `webagenticmemory.com/leaderboard` and `captures/leaderboard.txt` the output of `wam leaderboard`. Today the board lists one handle, the official account that publishes the built-in examples; the scene says so |
| 7 Ripple effect | 2:28 | An **illustrative multiplication** of one measured run per task × 1,000 uses (Canva ≈ 14 h, Calendar ≈ 54 h), labeled on the same screen. Dollars and tokens were not measured |
| 8 Close | 2:40 | A list of what was built in the event window and what was not |

## What the real runs say, and what the video does not claim

Three Slides runs on 2026-11-02 (Arise 0.1.64, `gpt-6-luna` max, durations and outer tool calls from the Codex logs):

| Run | Condition | Time | Calls |
|---|---|---:|---:|
| T | no hints, memory tools forbidden | 8:21 | 96 |
| T2 | the prompt asked for a web-memory lookup; nothing came back | 10:33 | 137 |
| M | WAM recalled over MCP (only an unrelated Slides image example exists) | 10:40 | 121 |

The two no-hint runs differ by +26% in time and +43% in calls, as much as anything M shows, so **the video makes no Slides speed claim**.
Run M is used only for the clip of the agent calling WAM. A Slides *proposal* memory does not exist yet; the next step is to write one from
what T and T2 had to work out (insert a photo, resize it from the Format options before it covers the text) and measure again, three runs per
condition. Tokens were not recorded per run beyond the log totals.

## Not in the video, on purpose

A logged-in contribution flow and the My Page dashboard. A real recording needs the local rig (server, worker, sign-in) and the production
catalog has no verified memories yet (issue #159). Do not replace a scene with a mock-up that looks like a recording; label any added footage as a fixture.

## Privacy rules for real footage

- The recordings were made in a logged-in browser profile. The Slides start dialog greets the account by its first name, so the lapse starts after it closes (`lapse.src_from`).
- The overlay crops away the tab strip, URL bar and the project-name row, and masks the account avatar. Check every new clip frame by frame before publishing.
- `demo/out/slides-real/` holds raw recordings: do not commit or upload them.

## Changing it

- **Words:** edit `CAPTIONS` in `stage.html`; the SRT is generated from the same array. Keep each caption to one line (about 70 characters).
- **Numbers:** `D` at the top of the script and `captures/run-slides.json`, with the report each one comes from. Re-check them before rendering.
- **Voice or music:** the video has no audio track. Add your own voice over the SRT timing, for example
  `ffmpeg -i wam-demo-day-v2.mp4 -i voice.wav -map 0:v -map 1:a -c:v copy -c:a aac -shortest out.mp4`.
- **A new capture:** rerun `wam-curate --scenario all -v` (about $0.05 on `reasoning.effort=low`), save the output to `captures/curate-all.txt`, and check
  it contains no key (`grep -c "$MODEL_API_KEY"` must print 0).
