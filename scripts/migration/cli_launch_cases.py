"""Observe original launch handler policy; replace only SDK launch execution."""
from __future__ import annotations
import contextlib
import io
import json
from pathlib import Path
from unittest.mock import patch
from numera.cli.parser import build_parser
from numera.cli.out import set_json_mode
from numera.models import Profile
from numera.fingerprint import generate
from numera.store import ProfileStore


def cases(parent, human=False):
    root=Path(parent)/'store';store=ProfileStore(root)
    with patch('time.time',return_value=1000.0):
        store.save(Profile(id='fixture',name='Fixture',fingerprint=generate(seed=42),created_at=1000.0,updated_at=1000.0))
    result=[]
    for argv in (['launch','missing','--headless','--json'],
                 ['launch','Fixture','--headless','--json'],
                 ['launch','Fixture','--engine','camoufox','--headless','--json']):
        args=build_parser().parse_args([value for value in argv if not human or value!='--json']);set_json_mode(not human);output=io.StringIO();seen=[];payload=[]
        def emit(value,code=0): payload.append(value);return code
        def launch(profile,store,**options):
            seen.append({'id':profile.id,'engine':options.get('engine'),'headless':options.get('headless'),'keep_open':options.get('keep_open')})
        with contextlib.ExitStack() as stack:
            stack.enter_context(contextlib.redirect_stdout(output))
            stack.enter_context(patch('numera.launcher.launch',side_effect=launch))
            if human:
                stack.enter_context(patch('numera.cli.browser.emit',side_effect=emit))
                stack.enter_context(patch('numera.cli.out.emit',side_effect=emit))
            code=args.func(args,store)
        result.append({'argv':argv,'exit':code,'payload':payload[0] if human else json.loads(output.getvalue()),'calls':seen,'stdout':output.getvalue() if human else None})
    return result


def attach_cases(parent, human=False):
    from types import SimpleNamespace
    from numera import automation, drivers
    root = Path(parent)/'attach-store'
    store = ProfileStore(root)
    store.save(Profile(id='fixture',name='Fixture',fingerprint=generate(seed=42)))
    results = []
    class Page:
        def wait_for_event(self,event,timeout):
            assert event == 'close' and timeout == 0
            raise KeyboardInterrupt()
    closed = []
    class Context:
        def close(self): closed.append('context')
    class Owner:
        def stop(self): closed.append('owner')
    @contextlib.contextmanager
    def admitted(profile,store): yield profile
    for reference in ['missing','Fixture']:
        argv = ['attach',reference,'--port','17777','--headless','--json']
        args=build_parser().parse_args([value for value in argv if not human or value!='--json']);set_json_mode(not human);output=io.StringIO();calls=[];closed.clear();payload=[]
        def emit(value,code=0): payload.append(value);return code
        def attach(profile,store,**options):
            calls.append(options)
            return (Owner(),Context(),Page()),{'Browser':'Fixture Chromium','webSocketDebuggerUrl':'ws://127.0.0.1:17777/devtools/browser/fixture'}
        with contextlib.ExitStack() as stack:
            stack.enter_context(contextlib.redirect_stdout(output))
            stack.enter_context(patch.object(drivers,'open_session',admitted))
            stack.enter_context(patch.object(drivers,'_closed'))
            stack.enter_context(patch.object(automation,'attach',attach))
            if human: stack.enter_context(patch('numera.cli.browser.emit',side_effect=emit))
            code=args.func(args,store)
        results.append({'argv':argv,'exit':code,'payload':payload[0] if human else json.loads(output.getvalue()),'calls':calls,'closed':list(closed),'stdout':output.getvalue() if human else None})
    headings=[]
    from numera.cli import browser as handlers
    for info in [{},{'Browser':''},{'Browser':None}]:
        output=io.StringIO();set_json_mode(False)
        with patch('sys.stdout',output),patch.object(drivers,'_closed'),patch.object(handlers,'emit',return_value=0):
            handlers._attached(args,store,store.get('fixture'),lambda *args,**kwargs:((Owner(),Context(),Page()),info),automation.snippets)
        headings.append({'info':info,'stdout':output.getvalue()})
    return {'cases':results,'headings':headings,'snippets':automation.snippets(17777,'ws://127.0.0.1:17777/devtools/browser/fixture')}
