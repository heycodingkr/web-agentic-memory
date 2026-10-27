#!/usr/bin/env python3
"""Local editor fixture sites with an independent saved-oracle (tests/env/README.md, plan W4-1).

Two loopback servers:
  agent port   /editor/<run>, /frame/<run>, and the POST endpoints the page itself uses
  oracle port  /oracle/<run> (state and verdict), POST /oracle/new (create a run). No page links to it.

Styles: rich, iframe, popup, offscreen. Rich and popup autosave 900 ms after the last trusted input;
iframe and offscreen need an explicit Save click. A run may answer saves with 503 (save_error).
The verdict is computed from what the server recorded, never from what the agent says.

Run standalone: python3 fixture_server.py   (prints one JSON line with both ports, serves until SIGTERM)
"""
import json
import secrets
import signal
import sys
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

HERE = Path(__file__).resolve().parent
TEMPLATE = (HERE / "static" / "editor.html").read_text(encoding="utf-8")
STYLES = ("rich", "iframe", "popup", "offscreen")
AUTOSAVE = {"rich": True, "popup": True, "iframe": False, "offscreen": False}
LABELS = {
    "rich": "Rich editor",
    "iframe": "Iframe editor",
    "popup": "Resume popup editor",
    "offscreen": "Offscreen save editor",
}
TITLE = "초안 제목 Draft title"
PARAGRAPHS = ["첫 번째 문단입니다. First paragraph.", "두 번째 문단입니다. Second paragraph."]
BODY_HTML = (
    '<article id="body" contenteditable="true" role="textbox" aria-label="Article body">'
    '<p data-node="initial"><br></p></article>'
)
FRAME_HTML = (
    '<!doctype html><meta charset="utf-8"><style>body{margin:18px;font:20px/1.6 Georgia,serif}'
    "p{margin:0 0 16px}#body{min-height:240px;outline:0}</style>" + BODY_HTML
)
MAX_BODY = 64 * 1024


def new_state(style, save_error, title, paragraphs):
    return {
        "style": style,
        "save_error": bool(save_error),
        "autosave": AUTOSAVE[style],
        "expected": {"title": title, "paragraphs": list(paragraphs)},
        "save_attempts": 0,
        "publish_count": 0,
        "saved": None,
        "observed": None,
        "dialog": None,
        "events": 0,
    }


def evaluate(state, require_native=True):
    """Verdict for one run. Reasons are stable codes so tests can assert on why a run failed."""
    expected = state["expected"]
    saved = state.get("saved")
    reasons = []
    if saved is None:
        reasons.append("save_failed" if state["save_attempts"] and state["save_error"] else "not_saved")
    else:
        if saved.get("title") != expected["title"]:
            reasons.append("title_mismatch")
        paragraphs = saved.get("paragraphs")
        if paragraphs != expected["paragraphs"]:
            reasons.append("paragraph_mismatch")
            if isinstance(paragraphs, list):
                if any(p == "" for p in paragraphs):
                    reasons.append("empty_paragraph")
                if len(paragraphs) < len(expected["paragraphs"]):
                    reasons.append("missing_paragraph")
    if state["publish_count"] > 0:
        reasons.append("published")
    if require_native:
        native = (state.get("observed") or {}).get("native") or {}
        if native.get("title") != expected["title"] or native.get("body") != expected["paragraphs"]:
            reasons.append("native_input_mismatch")
    if state.get("dialog") == "resume":
        reasons.append("resume_chosen")
    return {"verdict": "pass" if not reasons else "fail", "reasons": reasons}


class Run:
    def __init__(self, server, key, url):
        self.server = server
        self.key = key
        self.url = url

    @property
    def state(self):
        with self.server.lock:
            return json.loads(json.dumps(self.server.runs[self.key]))

    def oracle(self, require_native=True):
        state = self.state
        return {"state": state, **evaluate(state, require_native)}


class FixtureServer:
    def __init__(self, host="127.0.0.1"):
        self.host = host
        self.runs = {}
        self.lock = threading.Lock()
        self._servers = []
        self.agent_port = None
        self.oracle_port = None

    def start(self):
        agent = ThreadingHTTPServer((self.host, 0), _agent_handler(self))
        oracle = ThreadingHTTPServer((self.host, 0), _oracle_handler(self))
        for server in (agent, oracle):
            threading.Thread(target=server.serve_forever, daemon=True).start()
            self._servers.append(server)
        self.agent_port = agent.server_address[1]
        self.oracle_port = oracle.server_address[1]
        return self

    def stop(self):
        for server in self._servers:
            server.shutdown()
            server.server_close()
        self._servers = []

    def new_run(self, style="rich", save_error=False, title=TITLE, paragraphs=PARAGRAPHS, alias="fixture"):
        if style not in STYLES:
            raise ValueError(f"unknown style {style!r}")
        key = secrets.token_hex(6)
        with self.lock:
            self.runs[key] = new_state(style, save_error, title, paragraphs)
        return Run(self, key, f"http://{alias}.localhost:{self.agent_port}/editor/{key}")

    def __enter__(self):
        return self.start()

    def __exit__(self, *exc):
        self.stop()
        return False


def _read_json(handler):
    length = int(handler.headers.get("Content-Length") or 0)
    if length > MAX_BODY:
        return None
    try:
        return json.loads(handler.rfile.read(length) or b"{}")
    except ValueError:
        return None


class _Base(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def send(self, code, body, mime="application/json"):
        if not isinstance(body, bytes):
            body = body.encode("utf-8")
        self.send_response(code)
        self.send_header("Content-Type", mime + ("; charset=utf-8" if mime.startswith("text/") else ""))
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(body)

    def parts(self):
        return [p for p in self.path.split("?", 1)[0].split("/") if p]


def _agent_handler(fx):
    class Agent(_Base):
        def do_GET(self):
            parts = self.parts()
            if parts == ["healthz"]:
                return self.send(200, '{"ok":true}')
            if len(parts) == 2 and parts[0] in ("editor", "frame"):
                with fx.lock:
                    state = fx.runs.get(parts[1])
                if state is None:
                    return self.send(404, "{}")
                if parts[0] == "frame":
                    return self.send(200, FRAME_HTML, "text/html")
                style, key = state["style"], parts[1]
                body = (
                    f'<iframe id="editor-frame" title="Article body frame" src="/frame/{key}"></iframe>'
                    if style == "iframe"
                    else BODY_HTML
                )
                config = json.dumps({"id": key, "style": style, "autosave": AUTOSAVE[style]})
                html = (
                    TEMPLATE.replace("@@LABEL@@", LABELS[style])
                    .replace("@@CLASS@@", style)
                    .replace("@@BODY@@", body)
                    .replace("@@CONFIG@@", config)
                )
                return self.send(200, html, "text/html")
            return self.send(404, "{}")

        def do_POST(self):
            parts = self.parts()
            if len(parts) != 2 or parts[0] not in ("save", "event", "publish", "dialog"):
                return self.send(404, "{}")
            data = _read_json(self)
            if data is None:
                return self.send(400, "{}")
            kind, key = parts
            with fx.lock:
                state = fx.runs.get(key)
                if state is None:
                    return self.send(404, "{}")
                if kind == "save":
                    state["save_attempts"] += 1
                    if "native" in data:
                        state["observed"] = {"native": data["native"]}
                    if state["save_error"]:
                        return self.send(503, '{"saved":false}')
                    state["saved"] = {"title": data.get("title"), "paragraphs": data.get("paragraphs"), "via": data.get("via")}
                elif kind == "event":
                    state["observed"] = {"native": data.get("native"), "trusted_events": data.get("trusted_events")}
                    state["events"] += 1
                elif kind == "publish":
                    state["publish_count"] += 1
                elif kind == "dialog":
                    state["dialog"] = data.get("choice")
            return self.send(200, '{"ok":true}')

    return Agent


def _oracle_handler(fx):
    class Oracle(_Base):
        def do_GET(self):
            parts = self.parts()
            if len(parts) == 2 and parts[0] == "oracle":
                with fx.lock:
                    state = fx.runs.get(parts[1])
                    snapshot = json.loads(json.dumps(state)) if state else None
                if snapshot is None:
                    return self.send(404, "{}")
                return self.send(200, json.dumps({"state": snapshot, **evaluate(snapshot)}, ensure_ascii=False))
            return self.send(404, "{}")

        def do_POST(self):
            if self.parts() != ["oracle", "new"]:
                return self.send(404, "{}")
            data = _read_json(self) or {}
            try:
                run = fx.new_run(
                    style=data.get("style", "rich"),
                    save_error=data.get("save_error", False),
                    alias=data.get("alias", "fixture"),
                )
            except ValueError as error:
                return self.send(400, json.dumps({"error": str(error)}))
            return self.send(200, json.dumps({"key": run.key, "url": run.url}))

    return Oracle


def main():
    server = FixtureServer().start()
    print(json.dumps({"agent_port": server.agent_port, "oracle_port": server.oracle_port}), flush=True)
    stop = threading.Event()
    for sig in (signal.SIGINT, signal.SIGTERM):
        signal.signal(sig, lambda *_: stop.set())
    stop.wait()
    server.stop()
    return 0


if __name__ == "__main__":
    sys.exit(main())
