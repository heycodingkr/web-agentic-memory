# Demo

Sanitized demo editors, the scripts that record them in the Arise browser, and the edit that turns the footage
into the clips on the landing page (`web/static/media`).

No production credentials, private browser profiles, real user content or account-specific screenshots belong
here. Everything below runs against local pages with no login.

## What the clips are

Each clip is a **scripted replay**: a small script drives a demo editor in a clean Arise window the way a browser
agent would (trusted mouse and keyboard events, a visible cursor), and the edit adds the story around it: the
task, the memory hints, captions and a result. The hints are examples. Speeds in the clips are scripted for
readability and are **not measurements**; the only numbers shown come from the pilot in
[`docs/test/benchmarks.md`](../docs/test/benchmarks.md) and are labelled as such (one run per condition).

| Clip | Footage | What it shows |
|---|---|---|
| `demo-compare` | `blog-naive` next to `blog-informed` | The same blog task without memory (blocked click, empty paragraph) and with three hints |
| `demo-blog` | `blog-informed` | An agent writes a post and saves a clean draft |
| `demo-slides` | `slides-informed` | An agent builds a three-slide deck and fixes a picture that spills off the slide |

Each clip also exists with Korean captions (`demo-*-ko.mp4`). The footage is the same English demo editor; only
the task, hints, captions and end card are translated.

## Layout

```
sites/blog      Draftbook, a stand-alone blog editor (popup, Enter twice leaves an empty paragraph, Preview, Save draft)
sites/slides    Slidewise, a stand-alone slide editor (placeholders, picture that spills past the slide, autosave status)
record/         serve.py, record.py, scenarios.py, agent.py, arise_window.py, post.py, render.py, stage/stage.html
out/            raw recordings and frames (git-ignored)
```

The sites are single files with no network access. `record/serve.py` serves them on loopback as
`draftbook.localhost` and `slidewise.localhost`, so the address bar reads like a real site.

## Re-record and re-render

Needs macOS with Screen Recording and Accessibility permission for the app or terminal running it, `ffmpeg`,
Arise.app, Google Chrome, and `pip install pillow pyobjc-framework-Quartz`. Fonts are SIL OFL (Manrope, DM Sans,
DM Mono from github.com/google/fonts, `ofl/`); keep them outside the repo and pass the folder in.

```sh
# 1. record (uses a second, disposable Arise instance; your running Arise and its profile are not touched)
python3 demo/record/record.py blog-informed
python3 demo/record/record.py blog-naive
python3 demo/record/record.py slides-informed

# 2. find the sync marker and cut frames
for s in blog-informed blog-naive slides-informed; do python3 demo/record/post.py demo/out/$s; done

# 3. render the clips (H.264 MP4 + poster) into web/static/media; add --lang ko for the Korean captions (name gets a -ko suffix)
for c in blog-flow slides-flow compare; do python3 demo/record/render.py $c --data demo/out --fonts-dir ~/fonts; done
for c in blog-flow slides-flow compare; do python3 demo/record/render.py $c --lang ko --data demo/out --fonts-dir ~/fonts; done
```

`render.py <scene> [--lang ko] --at 3,9,14` writes still frames only, which is the quick way to check a change to
`record/stage/stage.html`. The stage is a pure function of time, so renders do not depend on machine speed.

How a recording stays clean: `record.py` launches Arise through `tests/lib/wamtest` in a throwaway HOME and
profile, dismisses its first-run card, sizes the window, finds the page viewport on screen with a calibration
fill and records only that region, so no desktop, other tab or account detail can end up in the footage. Look
through the frames before publishing anyway: a system notification can still land on top of the window.

Brand assets (logo, favicon, social card) are generated separately by `docs/assets/build_brand_assets.py`.
