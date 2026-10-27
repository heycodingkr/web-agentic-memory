"""macOS helpers for the isolated Arise window: geometry, focus, one real mouse click and viewport calibration.

Needs the Quartz bindings (pip install pyobjc-framework-Quartz pillow) and the same Screen Recording and
Accessibility permission that the terminal or app running the recording already has.
"""
import subprocess
import tempfile
import time
from pathlib import Path

import Quartz
from PIL import Image, ImageChops


def osa(script):
    result = subprocess.run(["osascript", "-e", script], capture_output=True, text=True)
    if result.returncode:
        raise RuntimeError(f"osascript failed: {result.stderr.strip()}")
    return result.stdout.strip()


def _process(pid):
    return f"(first process whose unix id is {pid})"


def window_rect(pid):
    """(x, y, w, h) of the first window of that process, in points."""
    raw = osa(f'tell application "System Events" to tell {_process(pid)} to return {{position of window 1, size of window 1}}')
    x, y, w, h = [int(float(v)) for v in raw.replace("{", "").replace("}", "").split(",")]
    return x, y, w, h


def set_window_rect(pid, x, y, w, h):
    osa(
        'tell application "System Events"\n'
        f"  tell {_process(pid)}\n"
        f"    set position of window 1 to {{{x}, {y}}}\n"
        f"    set size of window 1 to {{{w}, {h}}}\n"
        "  end tell\n"
        "end tell"
    )


def focus(pid):
    osa(f'tell application "System Events" to set frontmost of {_process(pid)} to true')


def click(x, y):
    """A real mouse click (CGEvent) in screen points; the app's own overlays ignore synthetic accessibility clicks."""
    for kind in (Quartz.kCGEventMouseMoved, Quartz.kCGEventLeftMouseDown, Quartz.kCGEventLeftMouseUp):
        event = Quartz.CGEventCreateMouseEvent(None, kind, (x, y), Quartz.kCGMouseButtonLeft)
        Quartz.CGEventPost(Quartz.kCGHIDEventTap, event)
        time.sleep(0.06)


def dismiss_onboarding(pid):
    """A fresh profile opens a centred "Getting started" card; its "Maybe later" button sits near the bottom."""
    x, y, w, h = window_rect(pid)
    focus(pid)
    time.sleep(0.4)
    click(x + w // 2 - 212, y + h - 42)
    time.sleep(0.8)


def capture_screen(path):
    subprocess.run(["screencapture", "-x", "-t", "png", str(path)], check=True)


def magenta_bbox(image_path):
    """Bounding box (device pixels) of the magenta calibration fill, or None.

    Screenshots are in the display colour space, so sRGB magenta comes back near (234, 51, 247); the thresholds
    only need to separate it from the dark Arise chrome and the cream or white demo pages."""
    image = Image.open(image_path).convert("RGB")
    r, g, b = image.split()
    mask = ImageChops.multiply(
        ImageChops.multiply(r.point(lambda v: 255 if v > 200 else 0), b.point(lambda v: 255 if v > 200 else 0)),
        g.point(lambda v: 255 if v < 100 else 0),
    )
    return mask.getbbox()


def calibrate_viewport(page):
    """Fill the page with magenta, take a screenshot and read back where the viewport sits on screen (device px)."""
    page.eval(
        "(() => { const d = document.createElement('div'); d.id = '__cal';"
        "d.style.cssText = 'position:fixed;inset:0;background:#ff00ff;z-index:2147483647'; document.documentElement.appendChild(d); })()"
    )
    time.sleep(0.5)
    with tempfile.TemporaryDirectory() as tmp:
        shot = Path(tmp) / "cal.png"
        capture_screen(shot)
        box = magenta_bbox(shot)
    page.eval("document.getElementById('__cal')?.remove()")
    if not box:
        raise RuntimeError("calibration fill not visible: is the Arise window covered or the onboarding card still open?")
    return box  # (left, top, right, bottom)
