#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Real stdio MCP smoke: one process, dynamic web setting, independent personal notes."""
import argparse
import http.server
import json
import os
from pathlib import Path
import re
import shutil
import tempfile
import threading
from probe_agent_memory_live import Bridge

ROOT=Path(__file__).resolve().parents[1]
CATALOG={"schema_version":1,"revision":"smoke-release","entries":[{
    "id":"smoke-editor","kind":"workflow","site":"medium.com","title":"Write draft",
    "summary":"SERVER_ONLY_HINT: focus the visible body.","detail":"SERVER_ONLY_DETAIL: use native input.",
    "evidence":"Isolated HTTP oracle, not a production workflow.","tags":["draft"],"status":"approved","verified_at":"2026-10-16"}]}

class Fixture(http.server.BaseHTTPRequestHandler):
    calls=[]
    def log_message(self,*_):pass
    def do_GET(self):
        type(self).calls.append(self.path)
        body=json.dumps(CATALOG).encode()
        self.send_response(200);self.send_header("Content-Type","application/json")
        self.send_header("Content-Length",str(len(body)));self.end_headers();self.wfile.write(body)

def main():
    parser=argparse.ArgumentParser()
    parser.add_argument("--binary",type=Path,default=ROOT/"target/debug/agent_browser")
    parser.add_argument("--out",type=Path,required=True)
    args=parser.parse_args();out=args.out.resolve();out.mkdir(parents=True,exist_ok=False)
    iso=Path(tempfile.mkdtemp(prefix="awm-",dir="/tmp"))
    config=iso/"config";data=config/"agent-browser";data.mkdir(parents=True)
    Fixture.calls=[]
    server=http.server.ThreadingHTTPServer(("127.0.0.1",0),Fixture)
    worker=threading.Thread(target=server.serve_forever,daemon=True);worker.start()
    endpoint=f"http://127.0.0.1:{server.server_port}"
    def settings(enabled):
        path=data/"settings.toml";tmp=path.with_suffix(".tmp")
        tmp.write_text('language = "ko"\nauto_update_check = false\ntelemetry_enabled = false\n'
            'web_workflow_contribution = false\nbrowser_memory_auto_recall = true\n'
            f'web_agentic_memory_enabled = {str(enabled).lower()}\nweb_agentic_memory_endpoint = "{endpoint}"\n')
        tmp.chmod(0o600);tmp.replace(path)
    bridge=None;result={"passed":False,"server_scope":"loopback fixture"}
    try:
        settings(False)
        env=dict(os.environ,XDG_CONFIG_HOME=str(config))
        for name in ["ARRISE_BROWSER_SESSION_ID","ARRISE_BROWSER_TAB_ID","ARRISE_BROWSER_LEASE_ID"]:
            env.pop(name,None)
        bridge=Bridge(args.binary.resolve(),env,out)
        result["mcp_pid"]=bridge.p.pid
        saved=bridge.tool("browser_memory_save",{"kind":"workflow","site":"medium.com","title":"Personal draft",
            "summary":"PERSONAL_ONLY_HINT","detail":"PERSONAL_ONLY_DETAIL"})
        personal=int(re.search(r"#(\d+)",str(saved)).group(1))
        query={"site":"medium.com","query":"draft"}
        off=bridge.tool("browser_memory_recall",query)
        assert "PERSONAL_ONLY_HINT" in off and "SERVER_ONLY_HINT" not in off and not Fixture.calls
        settings(True)
        on=bridge.tool("browser_memory_recall",query)
        assert "PERSONAL_ONLY_HINT" in on and "SERVER_ONLY_HINT" in on,on
        assert Fixture.calls==["/web-memory/v1/catalog"],Fixture.calls
        assert "SERVER_ONLY_DETAIL" in bridge.tool("browser_memory_expand",{"id":"web:smoke-editor","level":2})
        settings(False)
        off=bridge.tool("browser_memory_recall",query)
        assert "PERSONAL_ONLY_HINT" in off and "SERVER_ONLY_HINT" not in off
        assert "PERSONAL_ONLY_DETAIL" in bridge.tool("browser_memory_expand",{"id":personal,"level":2})
        try:bridge.tool("browser_memory_expand",{"id":"web:smoke-editor","level":2})
        except RuntimeError as error:assert "disabled" in str(error)
        else:raise AssertionError("disabled web expansion succeeded")
        settings(True)
        assert "SERVER_ONLY_HINT" in bridge.tool("browser_memory_recall",query)
        try:bridge.tool("browser_memory_forget",{"ids":["web:smoke-editor"]})
        except RuntimeError as error:assert "read-only" in str(error)
        else:raise AssertionError("web mutation succeeded")
        assert "PERSONAL_ONLY_DETAIL" in bridge.tool("browser_memory_expand",{"id":personal,"level":2})
        assert len(Fixture.calls)==1 and bridge.p.pid==result["mcp_pid"]
        result.update(passed=True,personal_id=personal,catalog_requests=Fixture.calls,
            same_mcp_process=True,off_blocks_web=True,personal_memory_preserved=True,web_records_read_only=True)
    except Exception as error:
        result["error"]=str(error)
        raise
    finally:
        if bridge:bridge.close()
        server.shutdown();server.server_close();worker.join(timeout=5)
        result["catalog_requests"]=list(Fixture.calls)
        (out/"result.json").write_text(json.dumps(result,ensure_ascii=False,indent=2)+"\n")
        shutil.rmtree(iso)
        print(json.dumps(result,ensure_ascii=False))

if __name__=="__main__":main()
