"""Local MCP/HTTP subprocess contract: never send a raw URL or fall back to another tab."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

ROOT = Path(__file__).resolve().parents[3]
BINARY = ROOT / "target" / "debug" / ("wam.exe" if os.name == "nt" else "wam")

FAKE_BROWSER = r'''
import json, sys
context = json.loads(open(sys.argv[1], encoding="utf-8").read())
reads = 0
for line in sys.stdin:
    message = json.loads(line)
    if "id" not in message: continue
    if message["method"] == "initialize": result = {"protocolVersion":"2025-11-25","serverInfo":{"name":"browser-fixture","version":"1"}}
    elif message["method"] == "tools/list": result = {"tools":[{"name":"browser_tabs","inputSchema":{"type":"object"}}]}
    else:
        reads += 1
        selected = context[min(reads - 1, len(context) - 1)]
        result = {"content":[{"type":"text","text":json.dumps(selected)}],"isError":False}
    print(json.dumps({"jsonrpc":"2.0","id":message["id"],"result":result}), flush=True)
'''


def context(**changes):
    tab = {"tab_id": 7, "window_id": 2, "is_mine": True, "is_automation_target": True,
           "owner": {"is_mine": True, "session_id": "owned-session", "client_name": "wam-tab-adapter", "tab_id": 7, "window_id": 2, "expires_at_ms": 9223372036854775807},
           "ready": True, "url": "https://example.com/guide/editor?token=private-sentinel#draft"}
    tab.update(changes)
    return {"ok": True, "session_id": "owned-session", "automation_target_tab": tab["tab_id"], "tabs": [tab]}


@unittest.skipUnless(BINARY.exists(), "build wam-cli before native adapter acceptance")
class TabAdapterTest(unittest.TestCase):
    def setUp(self):
        self.requests = []
        captured = self.requests

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *_): pass
            def do_GET(self):
                captured.append(("GET", self.path))
                self.reply({"schema_version": 1, "version": "test", "task_categories": ["compose-draft"], "keywords": ["blog", "write"]})
            def do_POST(self):
                body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
                captured.append(("POST", body))
                self.reply({"schema_version": 1, "reason": "ok", "hits": [], "receipt_id": None})
            def reply(self, value):
                body = json.dumps(value).encode()
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.worker = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.worker.start()
        self.temporary = tempfile.TemporaryDirectory(prefix="wam-tab-adapter-")
        self.root = Path(self.temporary.name)
        self.browser = self.root / "browser.py"
        self.browser.write_text(FAKE_BROWSER)

    def tearDown(self):
        self.server.shutdown()
        self.server.server_close()
        self.worker.join(timeout=2)
        self.temporary.cleanup()

    def run_adapter(self, contexts, extra=()):
        source = self.root / "context.json"
        source.write_text(json.dumps(contexts))
        env = dict(os.environ, WAM_TELEMETRY="off", WAM_CONFIG_DIR=str(self.root / "config"), WAM_CACHE_PATH=str(self.root / "cache.sqlite3"))
        messages = [
            {"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"wam-tab-adapter","version":"test"}}},
            {"jsonrpc":"2.0","method":"notifications/initialized"},
            {"jsonrpc":"2.0","id":2,"method":"tools/list"},
            {"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"web_memory_recall_current_tab","arguments":{"session_id":"owned-session","task_category":"compose-draft","keywords":["blog"]}}},
        ]
        result = subprocess.run([str(BINARY), "--endpoint", f"http://127.0.0.1:{self.server.server_port}", "--anonymous", "browser-mcp", "--browser-mcp", sys.executable,
            "--browser-arg=" + str(self.browser), "--browser-arg=" + str(source), *extra],
            input="".join(json.dumps(message)+"\n" for message in messages), capture_output=True, text=True, env=env, timeout=15)
        self.assertNotIn("private-sentinel", result.stdout + result.stderr)
        self.assertEqual(result.returncode, 0, result.stderr)
        replies = {reply["id"]:reply for reply in map(json.loads,result.stdout.splitlines())}
        self.assertEqual([tool["name"] for tool in replies[2]["result"]["tools"]], ["browser_tabs","web_memory_recall_current_tab"])
        return replies[3]["result"]

    def test_selected_tab_is_canonicalized_before_any_recall_request(self):
        result = self.run_adapter([context()])
        self.assertFalse(result["isError"])
        self.assertEqual(self.requests[-1], ("POST", {"url": "https://example.com/guide/editor", "keywords": ["blog"], "task_category": "compose-draft", "kind": None, "limit": 3}))
        self.assertNotIn("private-sentinel", json.dumps(self.requests))
        self.assertIsNone(json.loads(result["content"][0]["text"]).get("receipt_id"))

    def test_foreign_unready_and_missing_lease_never_recall(self):
        for changed in (context(is_mine=False), context(ready=False), context(is_automation_target=False)):
            with self.subTest(context=changed):
                self.assertTrue(self.run_adapter([changed])["isError"])
                self.assertEqual(self.requests, [])
        missing = context()
        missing["automation_target_tab"] = None
        missing["tabs"][0]["active"] = True
        self.assertTrue(self.run_adapter([missing])["isError"])
        self.assertEqual(self.requests, [])

    def test_tab_or_document_change_during_check_never_recall(self):
        for second in (context(tab_id=8), context(window_id=3), context(url="https://example.com/other")):
            with self.subTest(second=second):
                self.assertTrue(self.run_adapter([context(), second])["isError"])
                self.assertFalse(any(method == "POST" for method, _ in self.requests))

    def test_nonpublic_urls_are_rejected_locally(self):
        for url in ("http://127.0.0.1/private-sentinel", "https://localhost/private-sentinel", "https://user:private-sentinel@example.com/guide"):
            with self.subTest(url=url):
                self.assertTrue(self.run_adapter([context(url=url)])["isError"])
                self.assertEqual(self.requests, [])


if __name__ == "__main__":
    unittest.main()
