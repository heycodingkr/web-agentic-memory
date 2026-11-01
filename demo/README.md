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
| `demo-docs` | `docs-informed` | An agent names a document, adds headings from the Styles menu, makes a bullet list and fills a table with Tab |
| `demo-docs-compare` | `docs-naive` next to `docs-informed` | The same brief without memory (title typed into the page, `#` and `-` expected to format, three detours) and with three hints. Scripted: the "3 detours" is not a measurement |
| `demo-sheets` | `sheets-informed` | An agent fills a small budget: Tab across, Enter to the next row, formulas that start with `=`, totals checked after Saved |
| `demo-email` | `email-informed` | An agent drafts an email: Enter turns each address into a chip, the body goes above the signature, Save draft (sending is off) |
| `demo-forms` | `forms-informed` | An agent fills a time-off form: custom list, read-only date set through the calendar, required box at the bottom, Save draft (submitting is off) |

Each clip also exists with Korean captions (`demo-*-ko.mp4`). The footage is the same English demo editor; only
the task, hints, captions and end card are translated.

## Layout

```
sites/blog      Draftbook, a stand-alone blog editor (popup, Enter twice leaves an empty paragraph, Preview, Save draft)
sites/slides    Slidewise, a stand-alone slide editor (placeholders, picture that spills past the slide, autosave status)
sites/docs      Docwise, a stand-alone document editor (rename in the top bar, no Markdown shortcuts, Styles menu, bullet button, table grid, Tab in a table, autosave status)
sites/sheets    Gridwise, a stand-alone spreadsheet (select then type, Tab right and Enter back to the start column, =SUM only with the equals sign, autosave status)
sites/email     Mailwise, a stand-alone mail composer (Enter or comma makes a recipient chip, signature below the first line, subject required, sending off)
sites/forms     Formwise, a stand-alone web form (custom list, read-only date with a calendar that opens on the current month, required checkbox, validation, submitting off)
record/         serve.py, record.py, scenarios.py, agent.py, arise_window.py, post.py, render.py, stage/stage.html
out/            raw recordings and frames (git-ignored)
```

The sites are single files with no network access. `record/serve.py` serves them on loopback as
`draftbook.localhost`, `slidewise.localhost`, `docwise.localhost`, `gridwise.localhost`, `mailwise.localhost` and
`formwise.localhost`, so the address bar reads like a real site.

The clips are look-alike demo editors, not Google Docs, Google Slides or Medium: recording the real products would
need a logged-in account in the footage, which this folder never contains. Name them by what they are ("demo
document editor") in anything a visitor reads.

## Re-record and re-render

Needs macOS with Screen Recording and Accessibility permission for the app or terminal running it, `ffmpeg`,
Arise.app, Google Chrome, and `pip install pillow pyobjc-framework-Quartz` (with a Homebrew Python, use a
throwaway venv: `python3 -m venv --system-site-packages /tmp/wam-demo-venv`, install Quartz into it and run the
recording scripts with `/tmp/wam-demo-venv/bin/python -I`). Fonts are SIL OFL (Manrope, DM Sans, DM Mono from
github.com/google/fonts, `ofl/manrope/Manrope[wght].ttf`, `ofl/dmsans/DMSans[opsz,wght].ttf`,
`ofl/dmmono/DMMono-Medium.ttf`); keep them outside the repo and pass the folder in.

```sh
# 1. record (uses a second, disposable Arise instance; your running Arise and its profile are not touched)
python3 demo/record/record.py blog-informed
python3 demo/record/record.py blog-naive
python3 demo/record/record.py slides-informed
python3 demo/record/record.py docs-informed
python3 demo/record/record.py docs-naive
python3 demo/record/record.py sheets-informed
python3 demo/record/record.py email-informed
python3 demo/record/record.py forms-informed

# 2. find the sync marker and cut frames
for s in blog-informed blog-naive slides-informed docs-informed docs-naive sheets-informed email-informed forms-informed; do python3 demo/record/post.py demo/out/$s; done

# 3. render the clips (H.264 MP4 + poster) into web/static/media; add --lang ko for the Korean captions (name gets a -ko suffix)
for c in blog-flow slides-flow docs-flow sheets-flow email-flow forms-flow compare docs-compare; do python3 demo/record/render.py $c --data demo/out --fonts-dir ~/fonts; done
for c in blog-flow slides-flow docs-flow sheets-flow email-flow forms-flow compare docs-compare; do python3 demo/record/render.py $c --lang ko --data demo/out --fonts-dir ~/fonts; done
```

`render.py <scene> [--lang ko] --at 3,9,14` writes still frames only, which is the quick way to check a change to
`record/stage/stage.html`. The stage is a pure function of time, so renders do not depend on machine speed.

How a recording stays clean: `record.py` launches Arise through `tests/lib/wamtest` in a throwaway HOME and
profile, dismisses its first-run card, sizes the window, finds the page viewport on screen with a calibration
fill (searched for inside the Arise window only, so a magenta pixel in another app cannot stretch the crop) and
records only that region, so no desktop, other tab or account detail can end up in the footage. Look
through the frames before publishing anyway: a system notification can still land on top of the window.

Brand assets (logo, favicon, social card) are generated separately by `docs/assets/build_brand_assets.py`.
