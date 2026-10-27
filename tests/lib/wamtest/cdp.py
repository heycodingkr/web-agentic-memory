"""Minimal Chrome DevTools Protocol client (stdlib only): websocket framing and flattened sessions.

Used to drive a browser that the harness launched itself, on loopback, in a disposable profile.
"""
import base64
import hashlib
import json
import os
import socket
import struct
import threading
import time

GUID = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11"


class CDPError(RuntimeError):
    pass


class WebSocket:
    def __init__(self, url, timeout=10.0):
        if not url.startswith("ws://"):
            raise ValueError(f"only ws:// is supported: {url}")
        hostport, _, path = url[5:].partition("/")
        host, _, port = hostport.partition(":")
        self.sock = socket.create_connection((host, int(port or 80)), timeout=timeout)
        key = base64.b64encode(os.urandom(16)).decode()
        request = (
            f"GET /{path} HTTP/1.1\r\nHost: {hostport}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n"
            f"Sec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n"
        )
        self.sock.sendall(request.encode())
        buf = b""
        while b"\r\n\r\n" not in buf:
            chunk = self.sock.recv(4096)
            if not chunk:
                raise CDPError("websocket handshake closed")
            buf += chunk
        head, _, self._buf = buf.partition(b"\r\n\r\n")
        lines = head.decode("latin1").split("\r\n")
        if " 101 " not in lines[0]:
            raise CDPError(lines[0])
        accept = base64.b64encode(hashlib.sha1((key + GUID).encode()).digest()).decode()
        headers = {l.split(":", 1)[0].strip().lower(): l.split(":", 1)[1].strip() for l in lines[1:] if ":" in l}
        if headers.get("sec-websocket-accept") != accept:
            raise CDPError("bad Sec-WebSocket-Accept")
        self.sock.settimeout(None)
        self._send_lock = threading.Lock()

    def _recv_exact(self, n):
        while len(self._buf) < n:
            chunk = self.sock.recv(65536)
            if not chunk:
                raise ConnectionError("websocket closed")
            self._buf += chunk
        data, self._buf = self._buf[:n], self._buf[n:]
        return data

    def _send_frame(self, opcode, payload):
        header = bytearray([0x80 | opcode])
        n = len(payload)
        if n < 126:
            header.append(0x80 | n)
        elif n < 65536:
            header.append(0x80 | 126)
            header += struct.pack(">H", n)
        else:
            header.append(0x80 | 127)
            header += struct.pack(">Q", n)
        mask = os.urandom(4)
        masked = bytes(b ^ mask[i % 4] for i, b in enumerate(payload))
        with self._send_lock:
            self.sock.sendall(bytes(header) + mask + masked)

    def send_text(self, text):
        self._send_frame(0x1, text.encode("utf-8"))

    def recv_message(self):
        """Next text message, or None when the peer closed."""
        message = bytearray()
        while True:
            b1, b2 = self._recv_exact(2)
            fin, opcode, masked, n = b1 & 0x80, b1 & 0x0F, b2 & 0x80, b2 & 0x7F
            if n == 126:
                n = struct.unpack(">H", self._recv_exact(2))[0]
            elif n == 127:
                n = struct.unpack(">Q", self._recv_exact(8))[0]
            mask = self._recv_exact(4) if masked else None
            payload = self._recv_exact(n)
            if mask:
                payload = bytes(b ^ mask[i % 4] for i, b in enumerate(payload))
            if opcode == 0x8:
                return None
            if opcode == 0x9:
                self._send_frame(0xA, payload)
                continue
            if opcode == 0xA:
                continue
            if opcode in (0x1, 0x2):
                message = bytearray(payload)
            elif opcode == 0x0:
                message += payload
            if fin:
                return message.decode("utf-8")

    def close(self):
        try:
            self.sock.close()
        except OSError:
            pass


class CDP:
    """One browser-level websocket; pages are attached as flattened sessions."""

    def __init__(self, ws_url):
        self.ws = WebSocket(ws_url)
        self._id = 0
        self._cv = threading.Condition()
        self._results = {}
        self._events = []
        self._closed = False
        self._reader = threading.Thread(target=self._read_loop, daemon=True)
        self._reader.start()

    def _read_loop(self):
        try:
            while True:
                text = self.ws.recv_message()
                if text is None:
                    break
                data = json.loads(text)
                with self._cv:
                    if "id" in data:
                        self._results[data["id"]] = data
                    else:
                        self._events.append(data)
                    self._cv.notify_all()
        except (OSError, ValueError):
            pass
        finally:
            with self._cv:
                self._closed = True
                self._cv.notify_all()

    def send(self, method, params=None, session=None, timeout=15.0):
        with self._cv:
            self._id += 1
            msg_id = self._id
        message = {"id": msg_id, "method": method, "params": params or {}}
        if session:
            message["sessionId"] = session
        self.ws.send_text(json.dumps(message))
        deadline = time.monotonic() + timeout
        with self._cv:
            while msg_id not in self._results:
                if self._closed:
                    raise CDPError(f"connection closed during {method}")
                left = deadline - time.monotonic()
                if left <= 0:
                    raise CDPError(f"timeout waiting for {method}")
                self._cv.wait(left)
            reply = self._results.pop(msg_id)
        if "error" in reply:
            raise CDPError(f"{method}: {reply['error']}")
        return reply.get("result", {})

    def clear_events(self, method=None, session=None):
        with self._cv:
            self._events = [e for e in self._events if not self._match(e, method, session)]

    @staticmethod
    def _match(event, method, session):
        return (method is None or event.get("method") == method) and (session is None or event.get("sessionId") == session)

    def wait_event(self, method, session=None, timeout=15.0):
        deadline = time.monotonic() + timeout
        with self._cv:
            while True:
                for i, event in enumerate(self._events):
                    if self._match(event, method, session):
                        return self._events.pop(i)
                if self._closed:
                    raise CDPError(f"connection closed while waiting for {method}")
                left = deadline - time.monotonic()
                if left <= 0:
                    raise CDPError(f"timeout waiting for event {method}")
                self._cv.wait(left)

    def new_page(self, url="about:blank", size=(1280, 900)):
        target = self.send("Target.createTarget", {"url": "about:blank"})["targetId"]
        session = self.send("Target.attachToTarget", {"targetId": target, "flatten": True})["sessionId"]
        page = Page(self, session, target)
        page.call("Page.enable")
        page.call(
            "Emulation.setDeviceMetricsOverride",
            {"width": size[0], "height": size[1], "deviceScaleFactor": 1, "mobile": False},
        )
        page.ensure_focus()
        if url != "about:blank":
            page.navigate(url)
        return page

    def close(self):
        self.ws.close()


class Page:
    def __init__(self, cdp, session, target_id):
        self.cdp = cdp
        self.session = session
        self.target_id = target_id

    def call(self, method, params=None, timeout=15.0):
        return self.cdp.send(method, params, session=self.session, timeout=timeout)

    def ensure_focus(self):
        """A browser window that is hidden or in the background reports hasFocus()=false and drops native input
        (observed in the isolated Arise window). Focus emulation makes input reach the page regardless."""
        self.call("Emulation.setFocusEmulationEnabled", {"enabled": True})
        try:
            self.call("Page.bringToFront")
        except CDPError:
            pass

    def navigate(self, url, timeout=20.0):
        self.cdp.clear_events("Page.loadEventFired", self.session)
        result = self.call("Page.navigate", {"url": url})
        if result.get("errorText"):
            raise CDPError(f"navigate {url}: {result['errorText']}")
        self.cdp.wait_event("Page.loadEventFired", self.session, timeout)

    def eval(self, expression, await_promise=False, timeout=15.0):
        result = self.call(
            "Runtime.evaluate",
            {"expression": expression, "returnByValue": True, "awaitPromise": await_promise},
            timeout=timeout,
        )
        if "exceptionDetails" in result:
            details = result["exceptionDetails"]
            raise CDPError(details.get("exception", {}).get("description") or details.get("text", "evaluation failed"))
        return result["result"].get("value")

    def wait_for(self, expression, timeout=10.0, interval=0.1):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if self.eval(expression):
                return True
            time.sleep(interval)
        raise CDPError(f"condition not met within {timeout}s: {expression}")

    def rect(self, selector, frame=None):
        """Center of an element (optionally inside a same-origin iframe) in page coordinates, with a hit test."""
        js = """(() => {
          const frameSel = %s, sel = %s;
          let doc = document, ox = 0, oy = 0;
          if (frameSel) {
            const f = document.querySelector(frameSel);
            if (!f || !f.contentDocument) return null;
            doc = f.contentDocument;
            const fr = f.getBoundingClientRect(); ox = fr.left; oy = fr.top;
          }
          const el = doc.querySelector(sel);
          if (!el) return null;
          const r = el.getBoundingClientRect();
          const x = r.left + r.width / 2 + ox, y = r.top + r.height / 2 + oy;
          const inView = x >= 0 && y >= 0 && x <= innerWidth && y <= innerHeight;
          const top = inView ? document.elementFromPoint(x, y) : null;
          let hit = false;
          if (top) hit = frameSel ? top === document.querySelector(frameSel) : (top === el || el.contains(top) || top.contains(el));
          return {x, y, w: r.width, h: r.height, inView, hit};
        })()""" % (json.dumps(frame), json.dumps(selector))
        return self.eval(js)

    def stable_rect(self, selector, frame=None, settle=0.2, tries=5):
        """Read the geometry twice and require the center to move by at most 1px (scrolling/animation guard)."""
        for _ in range(tries):
            first = self.rect(selector, frame)
            time.sleep(settle)
            second = self.rect(selector, frame)
            if not first or not second:
                raise CDPError(f"element not found: {selector}")
            if abs(first["x"] - second["x"]) <= 1 and abs(first["y"] - second["y"]) <= 1:
                return second
        raise CDPError(f"geometry never settled: {selector}")

    def mouse_click(self, x, y):
        self.call("Input.dispatchMouseEvent", {"type": "mouseMoved", "x": x, "y": y})
        for kind in ("mousePressed", "mouseReleased"):
            self.call(
                "Input.dispatchMouseEvent",
                {"type": kind, "x": x, "y": y, "button": "left", "clickCount": 1},
            )

    def click(self, selector, frame=None):
        """Native click on a visible, uncovered element; refuses to click anything outside the viewport."""
        rect = self.stable_rect(selector, frame)
        if not rect["inView"]:
            raise CDPError(f"outside the viewport: {selector}")
        if not rect["hit"]:
            raise CDPError(f"covered by another element: {selector}")
        self.mouse_click(rect["x"], rect["y"])

    def scroll_into_view(self, selector):
        self.eval("document.querySelector(%s).scrollIntoView({block: 'center'})" % json.dumps(selector))

    def type_text(self, text):
        self.call("Input.insertText", {"text": text})

    def press_enter(self):
        base = {"key": "Enter", "code": "Enter", "windowsVirtualKeyCode": 13, "nativeVirtualKeyCode": 13}
        self.call("Input.dispatchKeyEvent", {**base, "type": "keyDown", "text": "\r"})
        self.call("Input.dispatchKeyEvent", {**base, "type": "keyUp"})

    def screenshot(self):
        data = self.call("Page.captureScreenshot", {"format": "png"})["data"]
        return base64.b64decode(data)

    def close(self):
        try:
            self.cdp.send("Target.closeTarget", {"targetId": self.target_id}, timeout=5)
        except CDPError:
            pass
