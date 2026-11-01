"""A scripted "agent" that drives a page over CDP with human-readable pacing.

It sends trusted mouse and keyboard events (the same kind a browser agent produces), draws a visible cursor,
a click ripple and a highlight around what it acts on, and logs named cues so the edit can follow the action.
The overlay lives in the recorded page only; the demo sites themselves stay untouched.
"""
import json
import math
import random
import time

OVERLAY_JS = r"""
(() => {
  if (window.__agent) return;
  const style = document.createElement('style');
  style.textContent = `
    #__agent_cursor { position: fixed; left: 0; top: 0; width: 30px; height: 30px; margin: -3px 0 0 -4px; z-index: 2147483647; pointer-events: none;
      transform: translate(-100px, -100px); filter: drop-shadow(0 3px 5px rgba(0,0,0,.35)); }
    .__agent_ripple { position: fixed; width: 18px; height: 18px; margin: -9px 0 0 -9px; border-radius: 50%; border: 3px solid #1b4338;
      background: rgba(216,239,120,.6); pointer-events: none; z-index: 2147483646; animation: __rip .65s ease-out forwards; }
    @keyframes __rip { from { transform: scale(.4); opacity: 1; } to { transform: scale(3.4); opacity: 0; } }
    .__agent_mark { position: fixed; pointer-events: none; z-index: 2147483645; border: 3px solid #7fb21a; border-radius: 12px;
      background: rgba(216,239,120,.14); box-shadow: 0 0 0 6px rgba(216,239,120,.38); animation: __mark 1.3s ease-out forwards; }
    @keyframes __mark { 0% { opacity: 0; transform: scale(1.03); } 12% { opacity: 1; transform: scale(1); } 72% { opacity: 1; } 100% { opacity: 0; } }
    #__agent_sync { position: fixed; left: 0; top: 0; width: 48px; height: 48px; background: #ff00ff; z-index: 2147483647; display: none; }
  `;
  document.documentElement.appendChild(style);
  const cursor = document.createElement('div');
  cursor.id = '__agent_cursor';
  cursor.innerHTML = '<svg viewBox="0 0 24 24" width="30" height="30"><path d="M4 2.5v16l4.4-3.9 2.8 6.4 2.8-1.2-2.8-6.2 5.8-.2z" fill="#111" stroke="#fff" stroke-width="1.6" stroke-linejoin="round"/></svg>';
  document.documentElement.appendChild(cursor);
  const sync = document.createElement('div');
  sync.id = '__agent_sync';
  document.documentElement.appendChild(sync);
  addEventListener('mousemove', (e) => { cursor.style.transform = `translate(${e.clientX}px, ${e.clientY}px)`; }, true);
  addEventListener('mousedown', (e) => {
    const r = document.createElement('div');
    r.className = '__agent_ripple'; r.style.left = e.clientX + 'px'; r.style.top = e.clientY + 'px';
    document.documentElement.appendChild(r); setTimeout(() => r.remove(), 800);
  }, true);
  window.__agent = {
    sync(on) { sync.style.display = on ? 'block' : 'none'; },
    mark(el) {
      if (!el) return;
      const b = el.getBoundingClientRect(), pad = 6;
      const m = document.createElement('div');
      m.className = '__agent_mark';
      m.style.cssText = `left:${b.left - pad}px;top:${b.top - pad}px;width:${b.width + pad * 2}px;height:${b.height + pad * 2}px`;
      document.documentElement.appendChild(m); setTimeout(() => m.remove(), 1500);
    }
  };
})();
"""

KEYS = {
    "Enter": (13, "\r"),
    "Backspace": (8, None),
    "Tab": (9, None),
    "Escape": (27, None),
}


class Agent:
    def __init__(self, page, seed=7):
        self.page = page
        self.rng = random.Random(seed)
        self.t0 = None
        self.cues = {}
        self.pos = (80.0, 80.0)

    # --- timing -----------------------------------------------------------------------------------------
    def now(self):
        return time.monotonic() - self.t0 if self.t0 else 0.0

    def cue(self, name):
        self.cues[name] = round(self.now(), 3)

    def pause(self, seconds):
        time.sleep(seconds)

    def install(self):
        self.page.eval(OVERLAY_JS)

    def sync_flash(self, hold=0.5):
        """Show a magenta square for a moment; the edit finds it in the footage to line cues up with video time."""
        self.page.eval("window.__agent.sync(true)")
        self.t0 = time.monotonic()
        time.sleep(hold)
        self.page.eval("window.__agent.sync(false)")

    # --- pointer ----------------------------------------------------------------------------------------
    def _mouse(self, kind, x, y, **extra):
        self.page.call("Input.dispatchMouseEvent", {"type": kind, "x": x, "y": y, **extra})

    def move(self, x, y, duration=0.6, buttons=0):
        x0, y0 = self.pos
        dist = math.hypot(x - x0, y - y0)
        if dist < 1:
            return
        duration = max(0.28, min(duration, 0.28 + dist / 1500))
        steps = max(10, int(duration * 60))
        arc = min(40.0, dist * 0.08) * self.rng.choice((-1, 1))
        nx, ny = -(y - y0) / dist, (x - x0) / dist
        for i in range(1, steps + 1):
            t = i / steps
            e = t * t * (3 - 2 * t)
            bend = arc * math.sin(math.pi * e)
            self._mouse("mouseMoved", x0 + (x - x0) * e + nx * bend, y0 + (y - y0) * e + ny * bend, buttons=buttons)
            time.sleep(duration / steps)
        self.pos = (x, y)

    def _point(self, selector, offset):
        rect = self.page.stable_rect(selector)
        if not rect["inView"]:
            self.page.scroll_into_view(selector)
            time.sleep(0.3)
            rect = self.page.stable_rect(selector)
        return rect["x"] + offset[0], rect["y"] + offset[1]

    def mark(self, selector):
        self.page.eval(f"window.__agent.mark(document.querySelector({json.dumps(selector)}))")

    def click(self, selector, duration=0.6, offset=(0, 0), hold=0.08, highlight=True):
        x, y = self._point(selector, offset)
        if highlight:
            self.mark(selector)
        self.move(x, y, duration)
        time.sleep(0.12)
        self._mouse("mousePressed", x, y, button="left", buttons=1, clickCount=1)
        time.sleep(hold)
        self._mouse("mouseReleased", x, y, button="left", buttons=0, clickCount=1)

    def drag(self, selector, dx, dy, duration=0.9):
        x, y = self._point(selector, (0, 0))
        self.mark(selector)
        self.move(x, y, 0.6)
        time.sleep(0.15)
        self._mouse("mousePressed", x, y, button="left", buttons=1, clickCount=1)
        steps = max(12, int(duration * 60))
        for i in range(1, steps + 1):
            t = i / steps
            e = t * t * (3 - 2 * t)
            self._mouse("mouseMoved", x + dx * e, y + dy * e, buttons=1)
            time.sleep(duration / steps)
        self._mouse("mouseReleased", x + dx, y + dy, button="left", buttons=0, clickCount=1)
        self.pos = (x + dx, y + dy)

    # --- keyboard ---------------------------------------------------------------------------------------
    def type(self, text, cps=44.0, mark="active"):
        """Type with human pacing. mark: "active" outlines the focused element, None skips the outline, any other
        string is a JS expression that returns the element to outline (a page-sized editor marks its caret block)."""
        if mark == "active":
            self.page.eval("window.__agent.mark(document.activeElement)")
        elif mark:
            self.page.eval(f"window.__agent.mark({mark})")
        for ch in text:
            self.page.call("Input.dispatchKeyEvent", {"type": "keyDown", "text": ch, "unmodifiedText": ch, "key": ch})
            self.page.call("Input.dispatchKeyEvent", {"type": "keyUp", "key": ch})
            delay = (1.0 / cps) * self.rng.uniform(0.6, 1.5)
            if ch in ".,;:—":
                delay += 0.14
            time.sleep(delay)

    def key(self, name, times=1, gap=0.18):
        code, text = KEYS[name]
        for _ in range(times):
            base = {"key": name, "code": name, "windowsVirtualKeyCode": code, "nativeVirtualKeyCode": code}
            down = {**base, "type": "keyDown"}
            if text:
                down["text"] = text
            self.page.call("Input.dispatchKeyEvent", down)
            self.page.call("Input.dispatchKeyEvent", {**base, "type": "keyUp"})
            time.sleep(gap)

    # --- checks -----------------------------------------------------------------------------------------
    def read(self, expression):
        return self.page.eval(expression)

    def wait_for(self, expression, timeout=8.0):
        return self.page.wait_for(expression, timeout=timeout)
