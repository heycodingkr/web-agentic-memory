#!/usr/bin/env python3
"""Read-only Arise browser MCP acceptance through the persistent WAM stdio bridge.

Opens and closes only its own background example.com tab. No account, prompt,
DOM, browser cookie, private socket, model call, or real WAM server is used.
"""
import argparse
import json
import os
from pathlib import Path
import selectors
import subprocess
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

ROOT = Path(__file__).resolve().parents[2]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--browser-mcp", default="/Applications/Arise.app/Contents/MacOS/agent_browser")
    args = parser.parse_args()
    session = "wam-bridge-acceptance-" + str(time.time_ns())
    client = "wam-native-bridge-acceptance"
    captured = []

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_): pass
        def reply(self, value):
            body = json.dumps(value).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
        def do_GET(self):
            captured.append(("GET", self.path))
            self.reply({"schema_version":1,"version":"test","task_categories":["compose-draft"],"keywords":["blog"]})
        def do_POST(self):
            captured.append(("POST", json.loads(self.rfile.read(int(self.headers["Content-Length"])))))
            self.reply({"schema_version":1,"reason":"ok","hits":[],"receipt_id":None})

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    worker = threading.Thread(target=server.serve_forever, daemon=True)
    worker.start()
    with tempfile.TemporaryDirectory(prefix="wam-native-bridge-") as temporary:
        env = dict(os.environ, WAM_TELEMETRY="off", WAM_CACHE_PATH=str(Path(temporary)/"cache.sqlite3"), WAM_CONFIG_DIR=str(Path(temporary)/"config"))
        process = subprocess.Popen([str(ROOT/"target/debug/wam"),"--endpoint",f"http://127.0.0.1:{server.server_port}","--anonymous","browser-mcp","--browser-mcp",args.browser_mcp,"--browser-arg=--mcp-server"],env=env,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,text=True,bufsize=1)
        def rpc(identifier, method, params):
            process.stdin.write(json.dumps({"jsonrpc":"2.0","id":identifier,"method":method,"params":params})+"\n")
            process.stdin.flush()
            selector = selectors.DefaultSelector()
            selector.register(process.stdout, selectors.EVENT_READ)
            try:
                for _ in range(64):
                    if not selector.select(8): raise RuntimeError("browser bridge response timeout")
                    line = process.stdout.readline()
                    if not line: raise RuntimeError("browser bridge exited")
                    response = json.loads(line)
                    if response.get("id") == identifier:
                        if "error" in response: raise RuntimeError("browser bridge protocol error")
                        return response["result"]
                raise RuntimeError("too many notifications")
            finally: selector.close()
        def value(result):
            content = result.get("structuredContent")
            if content is None: content = json.loads(result["content"][0]["text"])
            return content.get("reply", content)
        opened = None
        try:
            rpc(1,"initialize",{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":client,"version":"1"}})
            process.stdin.write(json.dumps({"jsonrpc":"2.0","method":"notifications/initialized"})+"\n"); process.stdin.flush()
            tools = rpc(2,"tools/list",{})["tools"]
            assert any(tool["name"] == "web_memory_recall_current_tab" for tool in tools)
            rpc(3,"tools/call",{"name":"browser_open","arguments":{"session_id":session,"url":"https://example.com/guide?wam_probe=synthetic-sentinel#fragment"}})
            for identifier in range(4,44):
                context = value(rpc(identifier,"tools/call",{"name":"browser_tabs","arguments":{"session_id":session}}))
                opened = context.get("automation_target_tab")
                tab = next((tab for tab in context.get("tabs",[]) if tab.get("is_automation_target")), {})
                if tab.get("ready") and tab.get("url", "").startswith("https://example.com/guide"): break
                time.sleep(.1)
            owner = tab.get("owner") or {}
            checks = {"context_ok":context.get("ok") is True,"session":context.get("session_id")==session,"target":tab.get("tab_id")==opened,"tab_mine":tab.get("is_mine") is True,"ready":tab.get("ready") is True,"owner_mine":owner.get("is_mine") is True,"owner_session":owner.get("session_id")==session,"owner_client":owner.get("client_name")==client,"owner_window":owner.get("window_id")==tab.get("window_id"),"lease_live":owner.get("expires_at_ms",0)>time.time()*1000}
            result = rpc(20,"tools/call",{"name":"web_memory_recall_current_tab","arguments":{"session_id":session,"task_category":"compose-draft","keywords":["blog"]}})
            if result.get("isError"):
                raise RuntimeError("current-tab recall failed; public context checks="+json.dumps(checks)+"; requests="+str(len(captured))+"; error="+result["content"][0]["text"])
            requests = [body for method,body in captured if method == "POST"]
            assert len(requests)==1 and requests[0]["url"]=="https://example.com/guide"
            assert "synthetic-sentinel" not in json.dumps(requests)+json.dumps(result)
            print(json.dumps({"native_arise_browser_bridge":"passed","browser_tools_preserved":len(tools)-1,"owned_lease_retained":True,"raw_query_in_wam_request":False,"real_accounts_used":0}))
        finally:
            try:
                if opened is not None:
                    closed = value(rpc(100,"tools/call",{"name":"browser_close_tab","arguments":{"session_id":session,"tab_id":opened}}))
                    if not closed.get("ok"): raise RuntimeError("owned acceptance tab could not be closed")
            finally:
                process.stdin.close()
                try: process.wait(timeout=4)
                except subprocess.TimeoutExpired: process.kill(); process.wait(timeout=3)
                server.shutdown(); server.server_close(); worker.join(timeout=2)


if __name__ == "__main__": main()
