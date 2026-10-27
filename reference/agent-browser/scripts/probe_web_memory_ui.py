#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Native browser-coordinate smoke against the isolated administrator preview only."""
import argparse
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import tempfile
import time
import urllib.parse
import urllib.request
from probe_agent_memory_live import Bridge, wait_for, stop_owned_app

ROOT=Path(__file__).resolve().parents[1]
def main():
    parser=argparse.ArgumentParser()
    parser.add_argument("--url",default="http://127.0.0.1:5714/web-memory")
    parser.add_argument("--stage-info",type=Path,default=Path("/tmp/arise-web-memory-stage.json"))
    parser.add_argument("--out",type=Path,required=True)
    parser.add_argument("--binary",type=Path,default=ROOT/"target/debug/agent_browser")
    parser.add_argument("--app",type=Path,default=ROOT/"target/debug/agent_browser_app")
    parser.add_argument("--bundle",type=Path,default=ROOT/"dist/Arise.app")
    args=parser.parse_args();out=args.out.resolve();out.mkdir(parents=True,exist_ok=False)
    stage=json.loads(args.stage_info.read_text())
    assert stage["scope"]=="local in-memory fixture"
    assert urllib.parse.urlparse(args.url).hostname=="127.0.0.1"
    assert urllib.parse.urlparse(stage["url"]).hostname=="127.0.0.1"
    iso=Path(tempfile.mkdtemp(prefix="awm-ui-",dir="/tmp"))
    app=None;bridge=None;result={"passed":False,"scope":"isolated administrator UI"}
    def api(path,body=None):
        payload=None if body is None else json.dumps(body).encode()
        request=urllib.request.Request(stage["url"]+path,data=payload,headers={"Content-Type":"application/json"})
        with urllib.request.urlopen(request,timeout=10) as response:return json.load(response)
    def interrupted(*_):raise KeyboardInterrupt
    signal.signal(signal.SIGTERM,interrupted)
    try:
        data=iso/"config/agent-browser";data.mkdir(parents=True)
        (data/"settings.toml").write_text('language = "ko"\nauto_update_check = false\ntelemetry_enabled = false\nweb_agentic_memory_enabled = false\nweb_workflow_contribution = false\n')
        for name in ["workspace","T","cache","ui-state"]:(iso/name).mkdir()
        bundle=iso/"Arise.app";contents=bundle/"Contents";(contents/"MacOS").mkdir(parents=True)
        shutil.copy2(args.app,contents/"MacOS/Arise");shutil.copy2(args.bundle/"Contents/Info.plist",contents/"Info.plist")
        (contents/"Frameworks").symlink_to((args.bundle/"Contents/Frameworks").resolve(),target_is_directory=True)
        env=dict(os.environ,TMPDIR=str(iso/"T")+"/",XDG_CONFIG_HOME=str(iso/"config"),AGENT_BROWSER_SESSIONS_DIR=str(data/"sessions"),
            AGENT_BROWSER_UI_STATE=str(iso/"ui-state"),ARRISE_ENGINE="cef",ARRISE_CEF_URL=args.url,ARRISE_CEF_CACHE=str(iso/"cache"),
            ARRISE_CEF_FRAMEWORK=str(contents/"Frameworks/Chromium Embedded Framework.framework"),ARRISE_CEF_BUNDLE=str(bundle),
            ARRISE_CEF_HELPER=str(contents/"Frameworks/Arise Helper.app/Contents/MacOS/Arise Helper"),ARRISE_MCP_BIN=str(args.binary.resolve()),
            AGENT_BROWSER_SELFTEST_PROMPT="Isolated UI harness; wait for the start gate.",
            AGENT_BROWSER_SELFTEST_START_FILE=str(iso/"never-start"),ARISE_NO_UPDATE_CHECK="1",ARRISE_CHAT_AGENT_LOOP="0")
        for name in ["ARRISE_BROWSER_SESSION_ID","ARRISE_BROWSER_TAB_ID","ARRISE_BROWSER_LEASE_ID","AGENT_BROWSER_MCP_ALLOW_CDP"]:
            env.pop(name,None)
        log=(out/"app.log").open("w")
        app=subprocess.Popen([str(contents/"MacOS/Arise")],env=env,stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
        result["app_pid"]=app.pid
        marker=iso/"T/arise-control.sock.path"
        sock=wait_for(lambda:Path(marker.read_text().strip()) if marker.exists() else None,45)
        env["ARRISE_CONTROL_SOCK"]=str(sock);env["ARISE_APP_PATH"]="/nonexistent/Arise.app"
        bridge=Bridge(args.binary.resolve(),env,out)
        host=urllib.parse.urlparse(args.url).netloc
        tabs=wait_for(lambda:[tab for tab in bridge.tool("browser_tabs")["reply"]["tabs"] if tab.get("ready") and host in tab.get("url","")],30)
        bridge.tool("browser_acquire",{"tab_id":tabs[0]["id"]})
        def evaluate(code):
            return json.loads(bridge.tool("browser_eval",{"js":"JSON.stringify("+code+")"})["reply"]["result"])
        wait_for(lambda:evaluate("location.origin")==urllib.parse.urlparse(args.url).scheme+"://"+host,20)
        auth=api("/auth/login",stage["login"])
        assert evaluate("(()=>{localStorage.setItem('adminToken',"+json.dumps(auth["token"])+");localStorage.setItem('admin',"+
            json.dumps(json.dumps(auth["user"]))+");return true})()") is True
        bridge.tool("browser_navigate",{"url":args.url})
        wait_for(lambda:evaluate("document.body.innerText.includes('작업 방법 편집')"),30)
        bridge.capture(out/"before.png")
        before=api("/web-memory/v1/catalog")
        def target(label,kind):
            if kind=="button":
                return "Array.from(document.querySelectorAll('button')).find(el=>el.textContent.trim()==="+json.dumps(label)+")"
            return "Array.from(document.querySelectorAll('label')).find(el=>el.textContent.trim().startsWith("+json.dumps(label)+"))?.querySelector('input,textarea')"
        def click(label,kind="button"):
            expression=target(label,kind)
            def geometry():
                return evaluate("(()=>{const el="+expression+";if(!el)return null;const r=el.getBoundingClientRect(),x=r.left+r.width/2,y=r.top+r.height/2;return {x:x/innerWidth,y:y/innerHeight,cy:y,h:innerHeight,scroll:scrollY,disabled:!!el.disabled,hit:el.contains(document.elementFromPoint(x,y))}})()")
            for _ in range(16):
                bridge.tool("browser_snapshot")
                point=geometry()
                assert point is not None,label
                if not .03<point["y"]<.97:
                    bridge.tool("browser_scroll",{"dy":max(-800,min(800,round(point["cy"]-point["h"]/2)))})
                    time.sleep(.25)
                    continue
                time.sleep(.2)
                latest=geometry()
                if abs(latest["cy"]-point["cy"])>1 or abs(latest["scroll"]-point["scroll"])>1:
                    continue
                point=latest
                if point["hit"] and not point["disabled"]:break
                time.sleep(.1)
            assert .01<point["x"]<.99 and .01<point["y"]<.99,(label,point)
            assert not point["disabled"] and point["hit"],(label,point)
            bridge.tool("browser_click",{"x_ratio":point["x"],"y_ratio":point["y"]})
            with (out/"clicks.jsonl").open("a") as clicks:
                clicks.write(json.dumps({"label":label,"before":point,"after":geometry()})+"\n")
        def fill(label,value):
            click(label,"field");bridge.tool("browser_type",{"text":value})
            actual=evaluate("("+target(label,"field")+").value")
            assert actual==value,(label,actual,value)
        click("새 작업 방법")
        fill("사이트 도메인","example.test")
        fill("제목","Local native-input workflow")
        fill("짧은 요령","Focus the visible input and verify the result.")
        fill("실행 절차","Take a fresh screenshot, click the visible input, and type natively.")
        fill("확인한 동작과 한계","Isolated administrator UI smoke only; not a verified external-site workflow.")
        bridge.capture(out/"edited.png")
        assert api("/web-memory/v1/catalog")==before,"editing must not change the approved catalog"
        click("검토 후보 등록")
        wait_for(lambda:evaluate("document.body.innerText.includes('검증 대기')"),15)
        assert api("/web-memory/v1/catalog")==before,"unverified candidate must stay invisible"
        fill("실행한 테스트","Local native input and isolated API smoke")
        fill("결과 자료 위치","artifact://isolated/admin-ui")
        click("검증 결과 기록")
        wait_for(lambda:evaluate("document.body.innerText.includes('검증 기록됨')"),15)
        click("승인·배포")
        approved=wait_for(lambda:(value if len((value:=api('/web-memory/v1/catalog'))["entries"])==len(before["entries"])+1 else None),15)
        assert any(note["title"]=="Local native-input workflow" for note in approved["entries"])
        wait_for(lambda:evaluate("document.body.innerText.includes('승인 완료')"),15)
        bridge.capture(out/"approved.png")
        for _ in range(8):bridge.tool("browser_scroll",{"dy":-800})
        time.sleep(.3);bridge.capture(out/"after.png")
        (out/"visible-state.json").write_text(json.dumps(evaluate("({url:location.href,text:document.body.innerText})"),ensure_ascii=False,indent=2))
        result.update(passed=True,native_coordinates=True,native_typing=True,candidate_quarantined=True,
            verification_then_admin_publish=True,before_revision=before["revision"],approved_revision=approved["revision"],
            entries=len(approved["entries"]))
    except Exception as error:
        result["error"]=str(error)
        if bridge:
            try:bridge.capture(out/"failure.png")
            except Exception:pass
        raise
    finally:
        if bridge:bridge.close()
        if app:stop_owned_app(app)
        shutil.rmtree(iso)
        (out/"result.json").write_text(json.dumps(result,ensure_ascii=False,indent=2)+"\n")
        print(json.dumps(result,ensure_ascii=False))
if __name__=="__main__":main()
