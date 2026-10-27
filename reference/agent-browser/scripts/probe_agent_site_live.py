#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Inspect or smoke an actual site through disposable Arise chat/profile state.

Explicit --profile-seed is copied read-only; the running user's profile is never opened
by the test browser. Default is inspection only. --run-chat submits a draft-only task.
A live-site result is distinct from the local fixture oracle in probe_agent_memory_live.
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import signal
import sqlite3
import subprocess
import tempfile
import time
from urllib.parse import quote
import probe_agent_memory_live as base

STATE="""(()=>{const texts=[],editables=[],blocks=[],frames=[];const walk=(doc,path,depth)=>{
 texts.push((doc.body?.innerText||'').slice(0,6000));
 for(const e of [...doc.querySelectorAll('[contenteditable=true],textarea,input[type=text]')].slice(0,20))
  editables.push({frame:path,tag:e.tagName,role:e.getAttribute('role'),label:e.getAttribute('aria-label'),field:e.getAttribute('data-field'),text:(e.innerText||e.value||'').slice(0,1200)});
 for(const e of [...doc.querySelectorAll('[contenteditable=true][role=textbox] h3,[contenteditable=true][role=textbox] p')].slice(0,50))blocks.push({frame:path,tag:e.tagName,text:e.innerText});
 if(depth<3)for(const [i,f]of [...doc.querySelectorAll('iframe')].entries()){
  const child=path?path+'.f'+i:'f'+i;let d=null;try{d=f.contentDocument}catch(_){}
  frames.push({frame:child,src:f.src,accessible:!!d});if(d)walk(d,child,depth+1);
 }};walk(document,'',0);return JSON.stringify({url:location.href,title:document.title,navigation_epoch:performance.timeOrigin,text:texts.join('\\n').slice(0,12000),blocks,editables,frames})})()"""

def page_state(bridge):
    return json.loads(bridge.tool('browser_eval',{'js':STATE})['reply']['result'])


def expected_text(state, title):
    # Medium uses NBSP while composing ordinary spaces; retain newlines for inspection.
    text = state.get('text', '').replace('\u00a0', ' ')
    return title in text and all(paragraph in text for paragraph in base.PARAGRAPHS)

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--url',required=True);parser.add_argument('--profile-seed',type=Path)
    parser.add_argument('--out',required=True,type=Path);parser.add_argument('--run-chat',action='store_true')
    parser.add_argument('--memory-note',type=Path);parser.add_argument('--timeout',type=int,default=240)
    parser.add_argument('--verify-reload',action='store_true')
    parser.add_argument('--prompt-suffix',default='',help='Extra constraints, recorded in the trace with the task')
    args=parser.parse_args();args.out=args.out.resolve();args.out.mkdir(parents=True,exist_ok=False)
    iso=Path(tempfile.mkdtemp(prefix='as-',dir='/tmp'));app=None;bridge=None
    result={'url':args.url,'ran_chat':args.run_chat,'profile_copied':bool(args.profile_seed),'memory_injected':bool(args.memory_note)}
    def interrupted(*_):raise KeyboardInterrupt
    signal.signal(signal.SIGTERM,interrupted)
    try:
        config=iso/'config';data=config/'agent-browser';data.mkdir(parents=True)
        (data/'settings.toml').write_text('language = "ko"\nauto_update_check = false\ntelemetry_enabled = false\nweb_agentic_memory_enabled = false\nweb_workflow_contribution = false\nbrowser_memory_auto_recall = '+('true' if args.memory_note else 'false')+'\nagent_workspace = '+json.dumps(str(iso/'workspace'))+'\n[model_effort]\n"codex:gpt-6-luna" = "max"\n')
        for name in ('workspace','T','cache','ui-state'):(iso/name).mkdir()
        cache=iso/'cache'
        if args.profile_seed:
            source=args.profile_seed.resolve();assert source.is_dir()
            subprocess.run(['cp','-Rc',str(source)+'/.',str(cache)],check=True,capture_output=True)
            for name in ('SingletonLock','SingletonSocket','SingletonCookie','DevToolsActivePort'):
                (cache/name).unlink(missing_ok=True)
            # Keep cookie SQLite/WAL snapshots consistent even when the original browser is running.
            for relative in ('Default/Cookies','Default/Network/Cookies'):
                original=source/relative
                if not original.exists():continue
                reader=sqlite3.connect('file:'+quote(str(original))+'?mode=ro',uri=True)
                writer=sqlite3.connect(cache/relative)
                reader.backup(writer);writer.close();reader.close()
        contents=iso/'Arise.app/Contents';(contents/'MacOS').mkdir(parents=True)
        shutil.copy2(base.ROOT/'target/debug/agent_browser_app',contents/'MacOS/Arise')
        bundle=base.ROOT/'target/dev-bundle/Arise.app';shutil.copy2(bundle/'Contents/Info.plist',contents/'Info.plist')
        (contents/'Frameworks').symlink_to((bundle/'Contents/Frameworks').resolve(),target_is_directory=True)
        trace=args.out/'trace.jsonl';start=iso/'start'
        title='[Arise QA] 팀과 함께 쓰는 AI'
        prompt=base.PROMPT.replace(base.TITLE,title)
        if args.prompt_suffix:prompt+='\n'+args.prompt_suffix
        env=dict(os.environ,TMPDIR=str(iso/'T')+'/',XDG_CONFIG_HOME=str(config),AGENT_BROWSER_SESSIONS_DIR=str(data/'sessions'),AGENT_BROWSER_UI_STATE=str(iso/'ui-state'),
            ARRISE_ENGINE='cef',ARRISE_CEF_URL=args.url,ARRISE_CEF_CACHE=str(cache),ARRISE_CEF_BUNDLE=str(iso/'Arise.app'),
            ARRISE_CEF_FRAMEWORK=str(contents/'Frameworks/Chromium Embedded Framework.framework'),ARRISE_CEF_HELPER=str(contents/'Frameworks/Arise Helper.app/Contents/MacOS/Arise Helper'),
            ARRISE_MCP_BIN=str(base.ROOT/'target/debug/agent_browser'),AGENT_BROWSER_SELFTEST_PROMPT=prompt,AGENT_BROWSER_SELFTEST_MODEL='codex:gpt-6-luna',
            AGENT_BROWSER_SELFTEST_START_FILE=str(start),AGENT_BROWSER_SELFTEST_TRACE=str(trace),ARRISE_CHAT_AGENT_LOOP='0',ARRISE_CLI_AUTO_APPROVE='1',ARISE_NO_UPDATE_CHECK='1')
        for key in ('ARRISE_BROWSER_SESSION_ID','ARRISE_BROWSER_TAB_ID','ARRISE_BROWSER_LEASE_ID','AGENT_BROWSER_MCP_ALLOW_CDP'):env.pop(key,None)
        with (args.out/'app.log').open('w') as log:app=subprocess.Popen([str(contents/'MacOS/Arise')],env=env,stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
        result['app_pid']=app.pid
        marker=iso/'T/arise-control.sock.path'
        sock=base.wait_for(lambda:Path(marker.read_text().strip()) if marker.exists() else None,45)
        env['ARRISE_CONTROL_SOCK']=str(sock);env['ARISE_APP_PATH']='/nonexistent/Arise.app'
        bridge=base.Bridge(base.ROOT/'target/debug/agent_browser',env,args.out)
        tabs=base.wait_for(lambda:[tab for tab in bridge.tool('browser_tabs')['reply']['tabs'] if tab.get('ready') and tab.get('url','').startswith('http')],45)
        tab=tabs[0]['id'];bridge.tool('browser_acquire',{'tab_id':tab})
        base.wait_for(lambda:state if (state:=page_state(bridge)).get('text','').strip() else None,30)
        time.sleep(5)
        initial=page_state(bridge);(args.out/'initial-state.json').write_text(json.dumps(initial,ensure_ascii=False,indent=2));bridge.capture(args.out/'before.png')
        result['initial_url']=initial['url'];result['initial_editables']=len(initial['editables'])
        if args.memory_note:
            result['seed']=bridge.tool('browser_memory_save',json.loads(args.memory_note.read_text()))
        final=initial
        if args.run_chat:
            bridge.tool('browser_release');start.touch()
            def terminal():
                events=base.rows(trace)
                return events if any(row.get('type') in ('done','failed') for row in events) else None
            events=base.wait_for(terminal,args.timeout)
            base.wait_for(lambda:bridge.tool('browser_acquire',{'tab_id':tab}),12)
            final=page_state(bridge);(args.out/'final-state.json').write_text(json.dumps(final,ensure_ascii=False,indent=2));bridge.capture(args.out/'after.png')
            result['terminal']=events[-1]['type'];result['elapsed_ms']=events[-1]['elapsed_ms'];result['final_url']=final['url']
            result['assistant']='\n'.join(row.get('text','') for row in events if row.get('type')=='text')
            result['tool_calls']=sum(row.get('type')=='tool_call' for row in events)
            result['tool_errors']=sum(row.get('type')=='tool_result' and bool(row.get('is_error')) for row in events)
            (args.out/'journal.json').write_text(json.dumps(base.ctl(sock,{'cmd':'action_journal'}),ensure_ascii=False,indent=2))
        result['expected_text_present']=expected_text(final,title)
        if args.verify_reload and result['expected_text_present'] and 'Saved' in final['text'] and '/p/' in final['url']:
            bridge.tool('browser_open',{'url':final['url']})
            base.wait_for(lambda:state if (state:=page_state(bridge)).get('navigation_epoch')!=final['navigation_epoch'] and expected_text(state,title) else None,30)
            reloaded=page_state(bridge)
            (args.out/'reloaded-state.json').write_text(json.dumps(reloaded,ensure_ascii=False,indent=2));bridge.capture(args.out/'reloaded.png')
            result['reload_verified']=expected_text(reloaded,title)
        result['inspection_completed']=True
    except Exception as error:
        result['error']=str(error)
        if bridge:
            try:
                (args.out/'failure-state.json').write_text(json.dumps(page_state(bridge),ensure_ascii=False,indent=2))
                bridge.capture(args.out/'failure.png')
            except Exception:pass
    finally:
        if bridge:
            try:bridge.close()
            except Exception:pass
        if app:base.stop_owned_app(app)
        shutil.rmtree(iso,ignore_errors=True)
    result['verification_scope']='medium_reload' if args.verify_reload else ('content_and_terminal' if args.run_chat else 'inspection')
    result['passed']=bool(result.get('inspection_completed'))
    if args.run_chat:
        result['passed']=result['passed'] and result.get('terminal')=='done' and result.get('expected_text_present',False)
        if args.verify_reload:result['passed']=result['passed'] and result.get('reload_verified',False)
    (args.out/'result.json').write_text(json.dumps(result,ensure_ascii=False,indent=2))
    print(json.dumps(result,ensure_ascii=False),flush=True)
    return 0 if result['passed'] else 1

if __name__=='__main__':raise SystemExit(main())
