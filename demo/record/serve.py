#!/usr/bin/env python3
"""Serve the demo editors by host name on loopback.

    draftbook.localhost:4890  ->  demo/sites/blog
    slidewise.localhost:4890  ->  demo/sites/slides

Chromium resolves *.localhost to the loopback address, so the browser address bar shows a readable name
instead of 127.0.0.1. Nothing leaves the machine.
"""
import argparse
import http.server
import socketserver
from pathlib import Path

SITES = Path(__file__).resolve().parents[1] / "sites"
ROUTES = {"draftbook": "blog", "slidewise": "slides"}


class Handler(http.server.SimpleHTTPRequestHandler):
    def _pick(self):
        host = (self.headers.get("Host") or "").split(":")[0].split(".")[0]
        self.directory = str(SITES / ROUTES.get(host, "blog"))

    def do_GET(self):
        self._pick()
        super().do_GET()

    def do_HEAD(self):
        self._pick()
        super().do_HEAD()

    def end_headers(self):
        self.send_header("Cache-Control", "no-store")
        super().end_headers()

    def log_message(self, *args):
        pass


class Server(socketserver.ThreadingMixIn, http.server.HTTPServer):
    daemon_threads = True
    allow_reuse_address = True


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--port", type=int, default=4890)
    args = parser.parse_args()
    with Server(("127.0.0.1", args.port), Handler) as server:
        print(f"demo sites on http://draftbook.localhost:{args.port}/ and http://slidewise.localhost:{args.port}/", flush=True)
        server.serve_forever()


if __name__ == "__main__":
    main()
