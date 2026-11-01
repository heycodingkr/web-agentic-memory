#!/usr/bin/env python3
"""Record a scripted agent run in a clean, isolated Arise window.

    python3 demo/record/record.py blog-informed          # records demo/out/blog-informed/raw.mp4 + meta.json
    python3 demo/record/record.py slides-informed --dry   # runs the script without recording

What it does: starts the demo sites on loopback, launches a second Arise instance in a disposable profile (the
installed app is used in place; your running Arise, its profile and settings are never touched), dismisses the
first-run card, sizes the window, finds the page viewport on screen and records only that region with ffmpeg
while the scripted agent works. The page and the agent are scripted, so the footage shows a replay, not a
live model run. Nothing is logged in and no real account is used.

Needs: ffmpeg, Arise.app, pillow and pyobjc-framework-Quartz (see arise_window.py), and Screen Recording plus
Accessibility permission for the app or terminal that runs this.
"""
import argparse
import json
import re
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
sys.path.insert(0, str(ROOT / "tests" / "lib"))
sys.path.insert(0, str(HERE))

import arise_window as win  # noqa: E402
import scenarios  # noqa: E402
from agent import Agent  # noqa: E402
from wamtest.arise import LocalArise  # noqa: E402
from wamtest.procs import Isolation  # noqa: E402

PORT = 4890
URLS = {
    "blog": f"http://draftbook.localhost:{PORT}/",
    "slides": f"http://slidewise.localhost:{PORT}/",
    "docs": f"http://docwise.localhost:{PORT}/",
    "sheets": f"http://gridwise.localhost:{PORT}/",
    "email": f"http://mailwise.localhost:{PORT}/",
    "forms": f"http://formwise.localhost:{PORT}/",
}
WINDOW = (10, 40, 1420, 770)  # points; leaves the Dock clear at the bottom
TITLEBAR_PT = 32  # macOS title bar above Arise's own tab strip
FPS = 30


def screen_device_index():
    out = subprocess.run(
        ["ffmpeg", "-hide_banner", "-f", "avfoundation", "-list_devices", "true", "-i", ""],
        capture_output=True, text=True,
    ).stderr
    match = re.search(r"\[(\d+)\] Capture screen 0", out)
    if not match:
        raise RuntimeError("no screen capture device listed by ffmpeg")
    return match.group(1)


class Recorder:
    def __init__(self, out, crop):
        x, y, w, h = crop
        self.proc = subprocess.Popen(
            [
                "ffmpeg", "-y", "-loglevel", "error", "-f", "avfoundation", "-framerate", str(FPS),
                "-pixel_format", "uyvy422", "-i", f"{screen_device_index()}:none",
                "-vf", f"crop={w}:{h}:{x}:{y}", "-c:v", "libx264", "-preset", "ultrafast", "-crf", "12",
                "-pix_fmt", "yuv420p", str(out),
            ],
            stdin=subprocess.PIPE,
        )

    def stop(self):
        try:
            self.proc.stdin.write(b"q")
            self.proc.stdin.flush()
        except (BrokenPipeError, ValueError):
            pass
        self.proc.wait(timeout=30)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("scenario", choices=sorted(scenarios.SCENARIOS))
    parser.add_argument("--out", default=str(ROOT / "demo" / "out"))
    parser.add_argument("--dry", action="store_true", help="run the script without recording")
    args = parser.parse_args()
    site, scenario = scenarios.SCENARIOS[args.scenario]
    out = Path(args.out) / args.scenario
    out.mkdir(parents=True, exist_ok=True)

    server = subprocess.Popen([sys.executable, str(HERE / "serve.py"), "--port", str(PORT)], stdout=subprocess.DEVNULL)
    isolation = Isolation("arise-demo")
    arise = LocalArise(URLS[site], isolation=isolation)
    recorder = None
    try:
        time.sleep(0.8)
        arise.start()
        pid = arise.proc.popen.pid
        page = arise.page
        time.sleep(2.0)
        win.dismiss_onboarding(pid)
        win.set_window_rect(pid, *WINDOW)
        win.focus(pid)
        time.sleep(1.2)
        dpr = page.eval("devicePixelRatio")
        wx, wy, ww, wh = win.window_rect(pid)  # search for the calibration fill inside the Arise window only
        window_px = tuple(int(v * dpr) for v in (wx, wy, wx + ww, wy + wh))
        left, top, right, bottom = win.calibrate_viewport(page, window_px)
        crop_top = int((WINDOW[1] + TITLEBAR_PT) * dpr)
        crop = (left & ~1, crop_top & ~1, (right - left) & ~1, (bottom - crop_top) & ~1)
        meta = {
            "scenario": args.scenario, "site": site, "fps": FPS, "dpr": dpr,
            "crop": dict(zip(("x", "y", "w", "h"), crop)),
            "viewport_in_crop": {"x": 0, "y": top - crop_top, "w": right - left, "h": bottom - top},
            "viewport_css": page.eval("[innerWidth, innerHeight]"),
        }
        print("calibrated", meta["crop"], meta["viewport_in_crop"], meta["viewport_css"], flush=True)

        page.navigate(URLS[site])  # fresh page so the popup timer starts now
        time.sleep(0.4)
        agent = Agent(page)
        agent.install()
        if not args.dry:
            recorder = Recorder(out / "raw.mp4", crop)
            time.sleep(1.2)  # ffmpeg needs a moment before it delivers frames
        agent.sync_flash()
        scenario(agent)
        time.sleep(0.6)
        meta["cues"] = agent.cues
        meta["script_seconds"] = round(agent.now(), 2)
        (out / "meta.json").write_text(json.dumps(meta, indent=2))
        print("cues", json.dumps(agent.cues), flush=True)
    finally:
        if recorder:
            recorder.stop()
        arise.stop()
        isolation.cleanup()
        server.terminate()
    print("done", out)


if __name__ == "__main__":
    main()
