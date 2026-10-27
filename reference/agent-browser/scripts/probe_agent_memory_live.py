#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Real Arise chat A/B/C benchmark; each run owns a disposable app/profile/session.

Build agent_browser + agent_browser_app first. --modes preflight validates the fixture
through native MCP input; normal modes send the identical Korean task through app chat.
Only localhost fixtures are edited. JSONL events and server-side draft oracles are retained.
"""
import argparse
import base64
from collections import Counter
import http.server
import json
import os
from pathlib import Path
import queue
import shutil
import signal
import socket
import subprocess
import tempfile
import threading
import time
import uuid

ROOT=Path(__file__).resolve().parents[1]
TITLE='팀과 함께 쓰는 AI'
PARAGRAPHS=['작은 결과를 팀 대화에 공유합니다.','맥락을 더하고 다음 행동을 함께 정합니다.']
PROMPT=f'지금 열린 편집기에 블로그 초안을 작성해줘. 제목은 "{TITLE}". 본문은 아래 두 문단 그대로 써줘.\n\n'+ '\n\n'.join(PARAGRAPHS)+'\n\n초안으로 저장하고 저장 완료인지 확인해줘. Publish나 발행은 누르지 마. 막히면 확인한 이유와 현재 상태를 알려줘.'
BODY='<article id="body" contenteditable="true" role="textbox" aria-label="Article body"><p data-node="initial"><br></p></article>'
FRAME='<!doctype html><meta charset="utf-8"><style>body{margin:18px;font:24px/1.65 Georgia}p{margin:0 0 16px}#body{min-height:260px;outline:0}</style>'+BODY

class Fixture(http.server.BaseHTTPRequestHandler):
    runs={}
    lock=threading.Lock()
    def log_message(self,*_):pass
    def reply(self,code,body,mime='application/json'):
        if not isinstance(body,bytes):body=body.encode()
        self.send_response(code);self.send_header('Content-Type',mime);self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
    def do_GET(self):
        parts=self.path.split('?',1)[0].strip('/').split('/')
        if len(parts)!=2 or parts[1] not in self.runs:return self.reply(404,'{}')
        kind,key=parts;run=self.runs[key]
        if kind=='frame':return self.reply(200,FRAME,'text/html; charset=utf-8')
        if kind!='editor':return self.reply(404,'{}')
        config={'id':key,'style':run['style']}
        body=BODY if run['style']=='medium' else '<iframe id="editor-frame" title="Article body frame" src="/frame/'+key+'"></iframe>'
        html=(ROOT/'assets/agent_editor_fixture.html').read_text().replace('@@BODY@@',body).replace('@@LABEL@@','Rich editor' if run['style']=='medium' else 'Iframe blog editor').replace('@@CONFIG@@',json.dumps(config))
        self.reply(200,html,'text/html; charset=utf-8')
    def do_POST(self):
        parts=self.path.strip('/').split('/')
        if len(parts)!=2 or parts[1] not in self.runs:return self.reply(404,'{}')
        kind,key=parts
        try:data=json.loads(self.rfile.read(int(self.headers.get('Content-Length','0'))))
        except ValueError:return self.reply(400,'{}')
        with self.lock:
            run=self.runs[key]
            if kind=='event':
                run['observed']=data
                if 'first_input_at' not in run:run['first_input_at']=time.time()
            elif kind=='save':
                run['save_attempts']+=1
                run['last_attempt']=data
                if run['save_error']:return self.reply(503,'{"saved":false}')
                run['saved']=data
            elif kind=='publish':run['publish_count']+=1
            elif kind in ('mouse','error'):run.setdefault(kind,[]).append(data)
            else:return self.reply(404,'{}')
        self.reply(200,'{"saved":true}')

class Bridge:
    def __init__(self,binary,env,out):
        self.log=(out/'mcp.log').open('w')
        self.operations=(out/'mcp-operations.jsonl').open('w')
        self.p=subprocess.Popen([str(binary),'--mcp-server'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=self.log,text=True,env=env)
        self.lines=queue.Queue();self.sequence=0
        threading.Thread(target=lambda:[self.lines.put(line) for line in self.p.stdout],daemon=True).start()
        self.rpc('initialize',{'protocolVersion':'2024-11-05','capabilities':{},'clientInfo':{'name':'agent-memory-eval','version':'1'}})
    def rpc(self,method,params):
        self.sequence+=1
        self.p.stdin.write(json.dumps({'jsonrpc':'2.0','id':self.sequence,'method':method,'params':params})+'\n');self.p.stdin.flush()
        response=json.loads(self.lines.get(timeout=30));assert response['id']==self.sequence,response
        if response.get('error'):raise RuntimeError(response['error'])
        return response['result']
    def tool(self,name,args=None):
        result=self.rpc('tools/call',{'name':name,'arguments':args or {}})
        logged=json.loads(json.dumps(result))
        for block in logged.get('content',[]):
            if block.get('type')=='image':block['data']='[image '+str(len(block.get('data','')))+' base64 characters]'
        self.operations.write(json.dumps({'at':time.time(),'name':name,'arguments':args or {},'result':logged},ensure_ascii=False)+'\n');self.operations.flush()
        if result.get('isError'):raise RuntimeError(str(result)[:1000])
        texts='\n'.join(block.get('text','') for block in result.get('content',[]) if block['type']=='text')
        if any(block['type']=='image' for block in result.get('content',[])):return result
        try:return json.loads(texts)
        except ValueError:return texts
    def capture(self,path):
        result=self.tool('browser_screenshot')
        block=next(block for block in result['content'] if block['type']=='image')
        path.write_bytes(base64.b64decode(block['data'],validate=True))
        assert path.read_bytes().startswith(b'\x89PNG\r\n\x1a\n')
    def close(self):
        if self.p.poll() is None:
            self.p.stdin.close()
            try:self.p.wait(timeout=8)
            except subprocess.TimeoutExpired:self.p.kill();self.p.wait()
        self.log.close();self.operations.close()

def ctl(path,request):
    with socket.socket(socket.AF_UNIX) as connection:
        connection.settimeout(15);connection.connect(str(path));connection.sendall((json.dumps(request)+'\n').encode())
        return json.loads(connection.makefile('rb').readline())

def wait_for(fn,seconds=30):
    deadline=time.monotonic()+seconds;last=None
    while time.monotonic()<deadline:
        try:
            value=fn()
            if value:return value
        except (OSError,ValueError,RuntimeError,queue.Empty) as error:last=error
        time.sleep(.2)
    raise TimeoutError(str(last) if last else 'condition timed out')

def rows(path):
    if not path.exists():return []
    result=[]
    for line in path.read_text(errors='replace').splitlines():
        try:result.append(json.loads(line))
        except ValueError:pass
    return result

def seed(bridge,host,style):
    summary=('Local fixture only: #title and #body are separate. Click each then browser_type without selector. Enter makes paragraph two. Autosave waits 900ms; verify Saved and full text. Never publish.' if style=='medium' else 'Local fixture only: #title is in the parent; #body is inside #editor-frame (frame f0). Click visible body with frame=f0, then native browser_type. Scroll parent until #save-draft is visible, click it, then verify Saved. Use browser_* here, not Naver-only blog_draft_input.')
    return bridge.tool('browser_memory_save',{'kind':'workflow','site':host,'title':'블로그 초안 작성 / draft writing '+style,'summary':summary,'detail':'Verified local fixture workflow. First take browser_screenshot standalone. Inspect current labels; stored selectors can become stale. Focus the title, type the title; focus body, type paragraphs with a newline. For the iframe case browser_click accepts frame=f0. After typing, inspect fresh screenshot and exact text, then wait for or click Save draft. A Save failed banner means the draft was NOT saved. Do not alter editor DOM/IDs with browser_eval. Do not repeat all text if saving fails.','evidence':'Local assets/agent_editor_fixture.html and preflight native-keyboard oracle; not a claim about real Medium or Naver.','tags':['블로그','초안','작성','draft','writing',style]})

def stop_owned_app(app):
    # Codex starts its own process group; killing only the app would leave that turn alive.
    table={}
    for line in subprocess.check_output(['ps','-axo','pid=,ppid=,pgid='],text=True).splitlines():
        pid,parent,group=map(int,line.split());table[pid]=(parent,group)
    owned={app.pid}
    while True:
        children={pid for pid,(parent,_) in table.items() if parent in owned}
        expanded=owned|children
        if expanded==owned:break
        owned=expanded
    groups={table[pid][1] for pid in owned if pid in table}-{os.getpgrp()}
    for group in groups:
        try:os.killpg(group,signal.SIGTERM)
        except ProcessLookupError:pass
    try:app.wait(timeout=8)
    except subprocess.TimeoutExpired:app.kill();app.wait()
    for group in groups:
        try:os.killpg(group,signal.SIGKILL)
        except ProcessLookupError:pass

def run_one(args,server,style,mode,index):
    label=f'{style}-{mode}-{index}';out=args.out/label;out.mkdir(parents=True,exist_ok=False)
    iso=Path(tempfile.mkdtemp(prefix='am-',dir='/tmp'))
    app=None;bridge=None;state={};result={'case':style,'mode':mode,'repeat':index,'driver':'mcp' if mode=='preflight' else 'chat','passed':False}
    key=uuid.uuid4().hex;host=('medium' if style=='medium' else 'naver')+'.localhost'
    url=f'http://{host}:{server.server_port}/editor/{key}'
    state={'style':style,'save_error':args.save_error,'save_attempts':0,'publish_count':0}
    Fixture.runs[key]=state
    try:
        config=iso/'config';data=config/'agent-browser';data.mkdir(parents=True)
        (data/'settings.toml').write_text('language = "ko"\nauto_update_check = false\ntelemetry_enabled = false\nweb_agentic_memory_enabled = false\nweb_workflow_contribution = false\nbrowser_memory_auto_recall = '+('true' if mode=='injected' else 'false')+'\nagent_workspace = '+json.dumps(str(iso/'workspace'))+'\n[model_effort]\n"codex:gpt-6-luna" = "max"\n')
        (iso/'workspace').mkdir();(iso/'T').mkdir();(iso/'cache').mkdir();(iso/'ui-state').mkdir()
        bundle=iso/'Arise.app';contents=bundle/'Contents';(contents/'MacOS').mkdir(parents=True)
        shutil.copy2(args.app,contents/'MacOS/Arise');shutil.copy2(args.bundle/'Contents/Info.plist',contents/'Info.plist')
        (contents/'Frameworks').symlink_to((args.bundle/'Contents/Frameworks').resolve(),target_is_directory=True)
        start=iso/'start';trace=out/'trace.jsonl'
        env=dict(os.environ,TMPDIR=str(iso/'T')+'/',XDG_CONFIG_HOME=str(config),AGENT_BROWSER_SESSIONS_DIR=str(data/'sessions'),
            AGENT_BROWSER_UI_STATE=str(iso/'ui-state'),ARRISE_ENGINE='cef',ARRISE_CEF_URL=url,ARRISE_CEF_CACHE=str(iso/'cache'),
            ARRISE_CEF_FRAMEWORK=str(contents/'Frameworks/Chromium Embedded Framework.framework'),ARRISE_CEF_BUNDLE=str(bundle),
            ARRISE_CEF_HELPER=str(contents/'Frameworks/Arise Helper.app/Contents/MacOS/Arise Helper'),ARRISE_MCP_BIN=str(args.binary),
            AGENT_BROWSER_SELFTEST_PROMPT=PROMPT,AGENT_BROWSER_SELFTEST_MODEL=args.model,
            AGENT_BROWSER_SELFTEST_START_FILE=str(start),AGENT_BROWSER_SELFTEST_TRACE=str(trace),ARRISE_CHAT_AGENT_LOOP='0',ARRISE_CLI_AUTO_APPROVE='1',ARISE_NO_UPDATE_CHECK='1')
        for name in ['ARRISE_BROWSER_SESSION_ID','ARRISE_BROWSER_TAB_ID','ARRISE_BROWSER_LEASE_ID','AGENT_BROWSER_MCP_ALLOW_CDP']:
            env.pop(name,None)
        with (out/'app.log').open('w') as log:app=subprocess.Popen([str(contents/'MacOS/Arise')],env=env,stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
        result['app_pid']=app.pid
        marker=iso/'T/arise-control.sock.path'
        sock=wait_for(lambda:Path(marker.read_text().strip()) if marker.exists() else None,45)
        env['ARRISE_CONTROL_SOCK']=str(sock);env['ARISE_APP_PATH']='/nonexistent/Arise.app'
        bridge=Bridge(args.binary,env,out)
        tabs=wait_for(lambda:[tab for tab in bridge.tool('browser_tabs')['reply']['tabs'] if tab.get('ready') and '/editor/' in tab.get('url','')],30)
        tab=tabs[0]['id'];bridge.tool('browser_acquire',{'tab_id':tab})
        bridge.tool('browser_wait_for',{'selector':'#title','timeout_ms':15000})
        bridge.capture(out/'before.png')
        if mode in ('manual','injected'):result['seed']=seed(bridge,host,style)
        if mode=='preflight':
            bridge.tool('browser_click',{'selector':'#title'});bridge.tool('browser_type',{'text':TITLE})
            target={'selector':'#body'}
            if style=='naver':target['frame']='f0'
            bridge.tool('browser_click',target);bridge.tool('browser_type',{'text':'\n'.join(PARAGRAPHS)})
            if style=='naver':
                try:bridge.tool('browser_click',{'selector':'#save-draft'})
                except RuntimeError as error:
                    assert 'outside the browser viewport' in str(error),str(error)
                    result['offscreen_click_rejected']=True
                else:raise AssertionError('offscreen target incorrectly reported a native click dispatch')
                bridge.tool('browser_scroll',{'dy':400})
                time.sleep(.3)
                bridge.capture(out/'scrolled.png')
            bridge.tool('browser_click',{'selector':'#save-draft'})
            wait_for(lambda:state.get('saved') or (args.save_error and state['save_attempts']),10)
            events=[];started=time.time()
        else:
            bridge.tool('browser_release');started=time.time();start.touch()
            def terminal_events():
                values=rows(trace)
                return values if any(row.get('type') in ('done','failed') for row in values) else None
            events=wait_for(terminal_events,args.timeout)
            result['terminal']=events[-1]
            result['wall_seconds']=round(time.time()-started,3)
            # The UI processes Done then releases its turn lease. Wait for that release.
            wait_for(lambda:bridge.tool('browser_acquire',{'tab_id':tab}),12)
        bridge.capture(out/'after.png')
        journal=ctl(sock,{'cmd':'action_journal'})
        (out/'journal.json').write_text(json.dumps(journal,ensure_ascii=False,indent=2))
        saved=state.get('saved',{});observed=state.get('observed',{})
        exact=saved.get('title')==TITLE and saved.get('paragraphs')==PARAGRAPHS
        observed_exact=observed.get('native',{}).get('title')==TITLE and observed.get('native',{}).get('body')==PARAGRAPHS
        tools=[row for row in events if row.get('type')=='tool_call'];counts=Counter(row['name'].rsplit('/',1)[-1].split('__')[-1] for row in tools)
        result.update({'exact_saved':exact,'exact_native_input':observed_exact,'save_attempts':state['save_attempts'],'publish_count':state['publish_count'],
            'tool_calls':len(tools),'tool_counts':dict(counts),'tool_errors':sum(row.get('type')=='tool_result' and bool(row.get('is_error')) for row in events),
            'first_input_seconds':round(state['first_input_at']-started,3) if state.get('first_input_at') and mode!='preflight' else None,
            'memory_in_model_prompt':any('[Browser know-how]' in row.get('model_prompt','') for row in events),
            'assistant':next((row['text'] for row in reversed(events) if row.get('type')=='done' and row.get('text')),None) or '\n'.join(row.get('text','') for row in events if row.get('type')=='text'),'fixture_errors':state.get('error',[])})
        expected=(not exact and state['save_attempts']>0) if args.save_error else exact
        result['blocked_as_expected']=bool(args.save_error and expected)
        result['passed']=expected and not state.get('error') and state['publish_count']==0 and (mode=='preflight' or result['terminal']['type']=='done')
        if mode in ('manual','injected') and result['memory_in_model_prompt']!=(mode=='injected'):result['passed']=False;result['context_mismatch']=True
        (out/'oracle.json').write_text(json.dumps(state,ensure_ascii=False,indent=2))
        if (data/'sessions').exists():shutil.copytree(data/'sessions',out/'sessions',dirs_exist_ok=True)
        result['before']=str(out/'before.png');result['after']=str(out/'after.png')
    except Exception as error:
        result['error']=str(error);result['trace_events']=len(rows(out/'trace.jsonl'))
        if bridge:
            try:
                bridge.capture(out/'failure.png')
                diagnostic=bridge.tool('browser_eval',{'js':"JSON.stringify({scrollY,viewport:[innerWidth,innerHeight],focus:document.activeElement.tagName,button:(()=>{const el=document.querySelector('#save-draft'),r=el.getBoundingClientRect();return {x:r.x,y:r.y,w:r.width,h:r.height,hit:document.elementFromPoint(r.x+r.width/2,r.y+r.height/2)?.outerHTML}})()})"})
                (out/'diagnostic.json').write_text(json.dumps(diagnostic,ensure_ascii=False,indent=2))
                (out/'journal.json').write_text(json.dumps(ctl(sock,{'cmd':'action_journal'}),ensure_ascii=False,indent=2))
            except Exception:pass
        (out/'oracle.json').write_text(json.dumps(state,ensure_ascii=False,indent=2))
    finally:
        if bridge:
            try:bridge.close()
            except Exception:pass
        if app:stop_owned_app(app)
        # Only this disposable app tree/profile is removed. Artifacts are outside it.
        shutil.rmtree(iso,ignore_errors=True)
        Fixture.runs.pop(key,None)
    (out/'result.json').write_text(json.dumps(result,ensure_ascii=False,indent=2))
    print(json.dumps(result,ensure_ascii=False),flush=True)
    return result

def main():
    def interrupted(*_):raise KeyboardInterrupt
    signal.signal(signal.SIGTERM,interrupted)
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out',type=Path,default=ROOT/'target/e2e-logs/agent-memory')
    parser.add_argument('--app',type=Path,default=ROOT/'target/debug/agent_browser_app')
    parser.add_argument('--binary',type=Path,default=ROOT/'target/debug/agent_browser')
    parser.add_argument('--bundle',type=Path,default=ROOT/'target/dev-bundle/Arise.app')
    parser.add_argument('--model',default='codex:gpt-6-luna')
    parser.add_argument('--modes',nargs='+',choices=['preflight','tools','manual','injected'],default=['tools','manual','injected'])
    parser.add_argument('--cases',nargs='+',choices=['medium','naver'],default=['medium','naver'])
    parser.add_argument('--repeats',type=int,default=1);parser.add_argument('--timeout',type=int,default=180)
    parser.add_argument('--save-error',action='store_true')
    args=parser.parse_args();args.out=args.out.resolve();args.out.mkdir(parents=True,exist_ok=True)
    assert args.repeats>=1
    for name in ('app','binary','bundle'):setattr(args,name,getattr(args,name).resolve())
    (args.out/'manifest.json').write_text(json.dumps({'prompt':PROMPT,'title':TITLE,'paragraphs':PARAGRAPHS,'model':args.model,'effort':'max','modes':args.modes,'cases':args.cases,'save_error':args.save_error},ensure_ascii=False,indent=2))
    server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Fixture)
    threading.Thread(target=server.serve_forever,daemon=True).start()
    results=[]
    try:
        for index in range(args.repeats):
            for style in args.cases:
                for mode in args.modes:results.append(run_one(args,server,style,mode,index))
    finally:server.shutdown();server.server_close()
    (args.out/'results.json').write_text(json.dumps(results,ensure_ascii=False,indent=2))
    print('PASS '+str(sum(result['passed'] for result in results))+'/'+str(len(results)),flush=True)
    return 0 if all(result['passed'] for result in results) else 1

if __name__=='__main__':raise SystemExit(main())
