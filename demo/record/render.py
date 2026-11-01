#!/usr/bin/env python3
"""Render an edit from stage/stage.html into an H.264 MP4 plus a poster frame.

    python3 demo/record/render.py blog-flow --data demo/out --fonts-dir ~/fonts --out web/static/media
    python3 demo/record/render.py compare --at 3,9,14 --data demo/out --fonts-dir ~/fonts   # stills only, to check the look

Scenes: blog-flow (uses blog-informed), slides-flow (slides-informed), docs-flow (docs-informed), sheets-flow, email-flow,
forms-flow (one informed run each), compare (blog-naive next to blog-informed) and docs-compare (docs-naive next to
docs-informed).
The footage must have been through post.py first. The stage is stepped frame by frame in headless Chrome, so the
result does not depend on how fast this machine is. Fonts are SIL OFL (Manrope, DM Sans, DM Mono).
"""
import argparse
import base64
import json
import math
import shutil
import socket
import subprocess
import sys
import tempfile
import time
import urllib.request
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
sys.path.insert(0, str(ROOT / "tests" / "lib"))

from wamtest.cdp import CDP  # noqa: E402

CHROME = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
FPS = 30
NEEDS = {
    "blog-flow": {"informed": "blog-informed"},
    "slides-flow": {"informed": "slides-informed"},
    "docs-flow": {"informed": "docs-informed"},
    "sheets-flow": {"informed": "sheets-informed"},
    "email-flow": {"informed": "email-informed"},
    "forms-flow": {"informed": "forms-informed"},
    "docs-compare": {"naive": "docs-naive", "informed": "docs-informed"},
    "compare": {"naive": "blog-naive", "informed": "blog-informed"},
}
OUT_NAMES = {"blog-flow": "demo-blog", "slides-flow": "demo-slides", "docs-flow": "demo-docs", "sheets-flow": "demo-sheets",
             "email-flow": "demo-email", "forms-flow": "demo-forms", "docs-compare": "demo-docs-compare", "compare": "demo-compare"}


def free_port():
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def launch_chrome(profile):
    port = free_port()
    proc = subprocess.Popen(
        [CHROME, "--headless=new", f"--remote-debugging-port={port}", f"--user-data-dir={profile}", "--allow-file-access-from-files",
         "--hide-scrollbars", "--force-device-scale-factor=1", "--window-size=1920,1080", "--mute-audio", "--no-first-run",
         "--disable-background-timer-throttling", "--disable-renderer-backgrounding", "about:blank"],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    )
    for _ in range(80):
        try:
            with urllib.request.urlopen(f"http://127.0.0.1:{port}/json/version", timeout=1) as response:
                return proc, json.loads(response.read())["webSocketDebuggerUrl"]
        except OSError:
            time.sleep(0.25)
    proc.terminate()
    raise RuntimeError("Chrome did not open its debugging port")


def load_footage(data, scene):
    footage = {}
    for key, folder in NEEDS[scene].items():
        info = json.loads((Path(data) / folder / "footage.json").read_text())
        footage[key] = info
    return footage


def screenshot(page, quality=93):
    shot = page.call("Page.captureScreenshot", {"format": "jpeg", "quality": quality}, timeout=30)
    return base64.b64decode(shot["data"])


def encode(frames, out_base, fps):
    mp4 = out_base.with_suffix(".mp4")
    subprocess.run(
        ["ffmpeg", "-v", "error", "-y", "-framerate", str(fps), "-i", str(frames / "%05d.jpg"), "-vf", "scale=1280:720:flags=lanczos", "-an",
         "-c:v", "libx264", "-preset", "slow", "-crf", "24", "-pix_fmt", "yuv420p", "-movflags", "+faststart", str(mp4)],
        check=True,
    )
    return mp4


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("scene", choices=sorted(NEEDS))
    parser.add_argument("--data", required=True, help="folder holding the recorded scenarios (each with footage.json)")
    parser.add_argument("--fonts-dir", required=True)
    parser.add_argument("--out", default=str(ROOT / "web" / "static" / "media"))
    parser.add_argument("--lang", choices=("en", "ko"), default="en", help="language of the words in the edit (the footage stays English)")
    parser.add_argument("--at", help="comma separated seconds: write PNG stills only (no video)")
    parser.add_argument("--poster-at", type=float, help="second to use as the poster frame (default: 60%% of the film)")
    args = parser.parse_args()

    footage = load_footage(args.data, args.scene)
    scene = {"name": args.scene, "lang": args.lang, "footage": footage, "fontsDir": str(Path(args.fonts_dir).resolve())}
    profile = tempfile.mkdtemp(prefix="wam-stage-")
    proc, ws = launch_chrome(profile)
    cdp = CDP(ws)
    try:
        page = cdp.new_page(f"file://{HERE / 'stage' / 'stage.html'}", size=(1920, 1080))
        duration = page.eval(f"window.__init({json.dumps(scene)})", await_promise=True, timeout=60)
        print(f"{args.scene}: {duration:.2f}s", flush=True)
        out = Path(args.out)
        out.mkdir(parents=True, exist_ok=True)
        if args.at:
            for sec in [float(x) for x in args.at.split(",")]:
                page.eval(f"window.__render({sec})", await_promise=True, timeout=60)
                target = out / f"stage-{args.scene}-{args.lang}-{sec:05.1f}.jpg"
                target.write_bytes(screenshot(page, 90))
                print("still", target, flush=True)
            return
        frames = Path(tempfile.mkdtemp(prefix=f"wam-frames-{args.scene}-"))
        count = int(math.ceil(duration * FPS))
        started = time.time()
        for i in range(count):
            page.eval(f"window.__render({i / FPS})", await_promise=True, timeout=60)
            (frames / f"{i + 1:05d}.jpg").write_bytes(screenshot(page))
            if i % 90 == 0:
                print(f"  frame {i}/{count} ({time.time() - started:.0f}s)", flush=True)
        base = out / (OUT_NAMES[args.scene] + ("-ko" if args.lang == "ko" else ""))
        mp4 = encode(frames, base, FPS)
        poster_t = args.poster_at if args.poster_at is not None else duration * 0.6
        poster = base.parent / f"{base.name}-poster.jpg"
        subprocess.run(["ffmpeg", "-v", "error", "-y", "-ss", f"{poster_t:.2f}", "-i", str(mp4), "-frames:v", "1", "-q:v", "3", str(poster)], check=True)
        for f in (mp4, poster):
            print(f"wrote {f} ({f.stat().st_size / 1024:.0f} KB)")
        shutil.rmtree(frames, ignore_errors=True)
    finally:
        cdp.close()
        proc.terminate()
        shutil.rmtree(profile, ignore_errors=True)


if __name__ == "__main__":
    main()
