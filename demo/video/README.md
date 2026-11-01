# Demo day video (Meta Global AI Developer Hackathon)

A 2:50, 1080p, **silent** video with burned-in English captions, rendered from `stage.html`. The scene plan, the claims it may make
and the submission checklist are in [`docs/hackathon/demo-video.md`](../../docs/hackathon/demo-video.md).

```sh
node demo/video/build.mjs --at 3,65,104      # PNG stills in /tmp/wam-video/stills (check a layout in seconds)
node demo/video/build.mjs                    # full render, about 4 minutes
# -> demo/out/video/wam-demo-day.mp4, wam-demo-day.en.srt, wam-demo-day-poster.jpg   (demo/out is git-ignored)
```

Needs Google Chrome (Playwright's `chrome` channel, taken from `web/node_modules`) and `ffmpeg`. The stage is a pure function of time
(`window.__render(t)`), so a render does not depend on machine speed. Output is H.264, BT.709 limited range, 30 fps.

## What is real and what is a drawing

| Scene | Time | Source |
|---|---|---|
| 1 Hook | 0:00 | Numbers from [`google-calendar.md`](../../docs/test/benchmarks/google-calendar.md) (R8). The timers replay two measured totals; they are not footage |
| 2 Product | 0:10 | Screenshots of the real landing page from this repo's web app (`captures/landing-quick.png`) and the scripted demo-editor poster, labeled as such |
| 3 Workflows | 0:22 | Numbers from the Calendar, Canva and Notion reports. Brand names are text only, no logos |
| 4 Contribution | 0:50 | **A diagram** of the pipeline in [`docs/design/memory-gatekeeper.md`](../../docs/design/memory-gatekeeper.md), not a recording |
| 5 Muse Spark decides | 1:05 | **A real run**: `captures/curate-all.txt` is the unedited output of `wam-curate --scenario all -v` against the Meta Model API (`muse:muse-spark-1.3`) on 2026-11-01. Lines are clipped to fit, never rewritten. The stat tiles come from [`meta-model-usage.ko.md`](../../docs/design/meta-model-usage.ko.md) except the live "20 turns, $0.049", which the run printed |
| 6 Reuse and reward | 1:50 | **A real session**: `captures/muse-code-run.json` is the event log of Muse Code 1.4.14 calling the local WAM MCP, condensed. The points rules come from the README |
| 7 Economics | 2:10 | Numbers from the reports and the Meta usage doc, with the single-observation caveat on screen |
| 8 Close | 2:35 | A list of what was built in the event window and what was not |

Not in the video, on purpose: a logged-in contribution flow and the My Page dashboard. A real recording needs the local rig
(server, worker, sign-in) and the production catalog is still empty (issue #159). Do not replace scene 4 or 6 with a mock-up that
looks like a recording; label any added footage as a fixture.

## Changing it

- **Words:** edit `CAPTIONS` in `stage.html`; the SRT is generated from the same array. Keep each caption to one line (about 70 characters).
- **Numbers:** `D` at the top of the script, with the report each one comes from. Re-check them against the reports before rendering.
- **Voice or music:** the video has no audio track. Add your own voice over the SRT timing, for example
  `ffmpeg -i wam-demo-day.mp4 -i voice.wav -map 0:v -map 1:a -c:v copy -c:a aac -shortest out.mp4`.
- **A new capture:** rerun `wam-curate --scenario all -v` (about $0.05 on `reasoning.effort=low`), save the output to `captures/curate-all.txt`, and check
  it contains no key (`grep -c "$MODEL_API_KEY"` must print 0).
