"""Original launch HTTP body/refusal policy at the owned SDK startup boundary."""
from __future__ import annotations
import json
import os
import tempfile
from pathlib import Path
from unittest.mock import patch
from fastapi.testclient import TestClient
from numera.server import create_app


def cases(parent):
    parent=Path(parent);parent.mkdir(parents=True,exist_ok=True)
    variants=[('missing',None,False,True,False),('unauthorized',None,True,False,False),
              ('absent',None,True,True,False),('null','null',True,True,True),
              ('empty','{}',True,True,True),('false','{"headless":false}',True,True,True),
              ('truthy','{"headless":"yes"}',True,True,False),
              ('null-option','{"headless":null}',True,True,True),
              ('list','[]',True,True,False),('bad','{',True,True,False)]
    results=[]
    for name,body,exists,authorized,default in variants:
        with tempfile.TemporaryDirectory(prefix='launch-source-',dir=parent) as directory:
            root=Path(directory);seen=[]
            with patch.dict(os.environ,{'NUMERA_API_TOKEN':'fixture-api','NUMERA_ALLOWED_ORIGINS':''}):
                from numera.server import routes_browser
                original_register=routes_browser.register
                def start(pid,command,**options):
                    seen.append({'id':pid,'headless':'--headless' in command,'kind':options.get('kind')})
                    return None
                def register(app,context):
                    context.spawn_owner=start
                    return original_register(app,context)
                with patch.object(routes_browser,'register',side_effect=register):
                    app=create_app(data_dir=directory,start_scheduler=False)
                (root/'config.json').write_text(json.dumps({'headless_launch':default}),encoding='utf-8')
                if exists:
                    (root/'dashboard').mkdir(exist_ok=True)
                    (root/'dashboard/a.json').write_text(json.dumps({'id':'a','name':'Fixture','seed':42}),encoding='utf-8')
                with TestClient(app,raise_server_exceptions=False) as client:
                    headers={'content-type':'application/json'}
                    if authorized: headers['authorization']='Bearer fixture-api'
                    response=client.post('/api/profiles/a/launch',content=body,headers=headers)
                value=response.json() if response.headers.get('content-type','').startswith('application/json') else response.text
                results.append({'name':name,'text':body,'exists':exists,'authorized':authorized,'default':default,'status':response.status_code,'body':value,'calls':seen})
    return results


def attach_cases(parent):
    parent=Path(parent);parent.mkdir(parents=True,exist_ok=True)
    results=[]
    for name,exists,engine,claim,authorized in [
        ('missing',False,'playwright',None,True),
        ('unsupported',True,'camoufox',None,True),
        ('foreign',True,'playwright','{',True),
        ('unauthorized',True,'playwright',None,False),
    ]:
        with tempfile.TemporaryDirectory(prefix='attach-source-',dir=parent) as directory:
            root=Path(directory)
            with patch.dict(os.environ,{'NUMERA_API_TOKEN':'fixture-api','NUMERA_ALLOWED_ORIGINS':''}):
                app=create_app(data_dir=directory,start_scheduler=False)
                if exists:
                    (root/'dashboard').mkdir(exist_ok=True)
                    (root/'dashboard/a.json').write_text(json.dumps({'id':'a','name':'Fixture','seed':42,'engine':engine}),encoding='utf-8')
                if claim:
                    (root/'sessions').mkdir(exist_ok=True)
                    (root/'sessions/a.json').write_text(claim,encoding='utf-8')
                with TestClient(app,raise_server_exceptions=False) as client:
                    response=client.post('/api/profiles/a/attach',headers={'authorization':'Bearer fixture-api'} if authorized else {})
            results.append({'name':name,'exists':exists,'engine':engine,'claim':claim,'authorized':authorized,
                            'status':response.status_code,'body':response.json()})
    return results


def attach_success(parent):
    """Actual source route payload/idempotence; only SDK owner/CDP are inert."""
    parent=Path(parent);parent.mkdir(parents=True,exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='attach-success-source-',dir=parent) as directory:
        root=Path(directory);live=set();seen=[]
        with patch.dict(os.environ,{'NUMERA_API_TOKEN':'fixture-api','NUMERA_ALLOWED_ORIGINS':''}):
            from numera.server import routes_browser
            original_register=routes_browser.register
            def register(app,context):
                def spawn(pid,command,**options):
                    live.add(pid)
                    seen.append({'kind':options['kind'],'headless':'--headless' in command})
                    return object()
                def stop(pid):
                    context.attached.pop(pid,None)
                    existed=pid in live
                    live.discard(pid)
                    return existed
                context.spawn_owner=spawn
                context.is_running=lambda pid: pid in live
                context.stop_session=stop
                return original_register(app,context)
            with patch.object(routes_browser,'register',side_effect=register):
                app=create_app(data_dir=directory,start_scheduler=False)
            (root/'dashboard').mkdir(exist_ok=True)
            (root/'dashboard/a.json').write_text(json.dumps({'id':'a','name':'Fixture','seed':42,'engine':'playwright'}),encoding='utf-8')
            from numera import automation
            def info(port):
                return {'Browser':'Fixture Chromium','webSocketDebuggerUrl':f'ws://127.0.0.1:{port}/devtools/browser/fixture'}
            with patch.object(automation,'cdp_info',side_effect=info) as cdp:
                with TestClient(app,raise_server_exceptions=False) as client:
                    headers={'authorization':'Bearer fixture-api'}
                    first=client.post('/api/profiles/a/attach',json={'headless':True},headers=headers)
                    repeated=client.post('/api/profiles/a/attach',json={'headless':False},headers=headers)
                    stopped=client.post('/api/profiles/a/stop',headers=headers)
                return {'status':first.status_code,'body':first.json(),'repeatedStatus':repeated.status_code,
                        'repeated':repeated.json(),'stopStatus':stopped.status_code,'calls':seen,'queries':cdp.call_count}


def warmup_cases(parent):
    parent=Path(parent);parent.mkdir(parents=True,exist_ok=True);results=[]
    for name,exists,claim,body in [
        ('missing',False,None,{}),('invalid-before-missing',False,None,{'minutes':0}),
        ('nonfinite',True,None,{'minutes':'inf'}),('preset',True,None,{'preset':'missing'}),
        ('foreign',True,'{',{}),('default',True,None,{}),('news',True,None,{'minutes':1,'preset':'news'}),
    ]:
        with tempfile.TemporaryDirectory(prefix='warmup-http-source-',dir=parent) as directory:
            root=Path(directory);seen=[]
            with patch.dict(os.environ,{'NUMERA_API_TOKEN':'fixture-api','NUMERA_ALLOWED_ORIGINS':''}):
                from numera.server import routes_browser
                original_register=routes_browser.register
                def register(app,context):
                    def spawn(pid,command,**options):
                        seen.append({'kind':options['kind'],'headless':'--headless' in command,
                                     'minutes':command[command.index('--minutes')+1],
                                     'preset':command[command.index('--preset')+1] if '--preset' in command else None})
                        return object()
                    context.spawn_owner=spawn
                    return original_register(app,context)
                with patch.object(routes_browser,'register',side_effect=register):
                    app=create_app(data_dir=directory,start_scheduler=False)
                if exists:
                    (root/'dashboard').mkdir(exist_ok=True)
                    (root/'dashboard/a.json').write_text(json.dumps({'id':'a','name':'Fixture','seed':42}),encoding='utf-8')
                if claim:
                    (root/'sessions').mkdir(exist_ok=True)
                    (root/'sessions/a.json').write_text(claim,encoding='utf-8')
                with TestClient(app,raise_server_exceptions=False) as client:
                    response=client.post('/api/profiles/a/warmup',json=body,headers={'authorization':'Bearer fixture-api'})
                results.append({'name':name,'exists':exists,'claim':claim,'request':body,
                                'status':response.status_code,'body':response.json(),'calls':seen})
    return results


def scheduler_cases(parent):
    parent=Path(parent);parent.mkdir(parents=True,exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='scheduler-source-',dir=parent) as directory:
        root=Path(directory);seen=[]
        from numera.server import routes_schedules
        original_register=routes_schedules.register
        def register(app,context):
            def spawn(pid,command,**options):
                seen.append({'id':pid,'kind':options['kind'],'schedule':options['schedule_id']})
                return object()
            context.spawn_owner=spawn
            return original_register(app,context)
        with patch.dict(os.environ,{'NUMERA_API_TOKEN':'fixture-api','NUMERA_ALLOWED_ORIGINS':''}),patch.object(routes_schedules,'register',side_effect=register):
            app=create_app(data_dir=directory,start_scheduler=False)
        (root/'dashboard').mkdir(exist_ok=True)
        for pid,record in [('bad',{'proxy':{'type':'INVALID','host':'fixture.test','port':'80'}}),('good',{})]:
            (root/f'dashboard/{pid}.json').write_text(json.dumps({'id':pid,'name':pid,'seed':42,**record}),encoding='utf-8')
        from numera.scheduler import ScheduleStore
        schedules=ScheduleStore(root)
        rows=[{'id':sid,'profileId':pid,'everyHours':1,'minutes':1,'preset':'news','enabled':True,'lastRun':None,'createdAt':900}
              for sid,pid in [('broken','bad'),('healthy','good'),('missing','gone')]]
        schedules._save(rows)
        with patch('time.time',return_value=1000.0): app.state.scheduler_tick()
        return {'rows':rows,'stored':schedules.list(),'calls':seen}
