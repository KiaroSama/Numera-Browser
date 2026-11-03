"""Original warmup policy and bounded control, with no browser/network/wait."""
from __future__ import annotations

import contextlib
from unittest.mock import patch
from baseline_source import activate
activate()

from numera import warmup
from numera.fingerprint import generate
from numera.models import Profile


def cases():
    result=[]
    for minutes,preset,sites in [(5,None,None),('２',None,None),('1_0','ADS',None),
            (True,None,[]),(0,None,None),(-1e-8,None,None),(481,None,None),
            ('nan',None,None),('inf',None,None),(None,None,None),('bad',None,None),
            (480.1256789,None,None),(5,'missing',None),(5,'',None),
            (5,None,'https://fixture.test/'),(5,None,['file:///fixture']),
            (5,'news',['https://fixture.test/'])]:
        try: value,error=warmup.validate_warmup(minutes,preset,sites),None
        except ValueError as exception: value,error=None,str(exception)
        result.append({'minutes':minutes,'preset':preset,'sites':sites,'value':value,'error':error})
    return {'validation':result,'presets':warmup.PRESETS,
            'fallback':warmup.preset_sites('missing'),
            'hostScript':'() => location.host',
            'linksScript':"""(host) => [...document.querySelectorAll('a[href]')]
                 .map(a => a.href)
                 .filter(h => h.startsWith('http') && new URL(h).host === host)
                 .slice(0, 40)"""}


def run_cases():
    result=[]
    for failed in [False,True]:
        now=[0.0];operations=[];closed=[]
        class Random:
            def shuffle(self,values): pass
            def uniform(self,lo,hi): return lo
            def random(self): return 0.5
            def randint(self,lo,hi): return lo
            def choice(self,values): return values[0]
        class Mouse:
            def wheel(self,x,y): operations.append({'operation':'wheel','x':x,'y':y})
            def move(self,x,y,steps): operations.append({'operation':'move','x':x,'y':y,'steps':steps})
        class Page:
            mouse=Mouse()
            def goto(self,url,wait_until,timeout):
                operations.append({'operation':'navigate','url':url,'timeout':timeout})
                now[0]+=0.01 if failed else 0.0
                if failed: raise RuntimeError('Synthetic navigation failure')
            def evaluate(self,script,argument=None):
                operations.append({'operation':'evaluate','expression':script,'argument':argument})
                return 'fixture.test' if argument is None else ['https://fixture.test/deeper']
        class Context:
            pages=[Page()]
        @contextlib.contextmanager
        def session(*args,**kwargs):
            try: yield Context()
            finally: closed.append(True)
        def wait(seconds): now[0]+=seconds
        profile=Profile(name='Fixture',fingerprint=generate(seed=42))
        with patch.object(warmup,'session',session),patch.object(warmup.random,'Random',Random),patch.object(warmup.time,'time',lambda:now[0]),patch.object(warmup.time,'sleep',wait):
            summary=warmup.warm_up(profile,None,minutes=0.001,sites=['https://fixture.test/'],headless=True)
        import io
        from types import SimpleNamespace
        from numera.cli.parser import build_parser
        from numera.cli.out import set_json_mode
        from numera.cli import browser as handlers
        class Stream(io.StringIO):
            def __init__(self,tty): super().__init__();self.tty=tty
            def isatty(self): return self.tty
        human=[]
        for tty in [False,True]:
            now[0]=0.0;operations.clear();closed.clear();stdout=Stream(tty);payload=[]
            args=build_parser().parse_args(['warmup','Fixture','--minutes','0.001','--site','https://fixture.test/','--headless'])
            set_json_mode(False)
            def emit(value,code=0): payload.append(value);return code
            with patch('sys.stdout',stdout),patch.object(handlers,'emit',side_effect=emit), \
                    patch.object(warmup,'session',session),patch.object(warmup.random,'Random',Random), \
                    patch.object(warmup.time,'time',lambda:now[0]),patch.object(warmup.time,'sleep',wait):
                code=args.func(args,SimpleNamespace(resolve=lambda ref:profile))
            human.append({'tty':tty,'stdout':stdout.getvalue(),'payload':payload[0],'exit':code})
        result.append({'failed':failed,'summary':summary,'operations':list(operations),'closed':list(closed),'human':human})
    return result


def schedule_cases(parent):
    import io,json
    from pathlib import Path
    from numera.cli.parser import build_parser
    from numera.cli.out import set_json_mode
    from numera.scheduler import ScheduleStore
    from numera.store import ProfileStore
    result=[]
    for mode in ['empty','mixed','failure']:
        root=Path(parent)/mode;store=ProfileStore(root);sched=ScheduleStore(root)
        profile=Profile(id='fixture',name='Fixture',fingerprint=generate(seed=42))
        store.save(profile);calls=[]
        rows=[] if mode=='empty' else [
            {'id':'run','profileId':'fixture','everyHours':1,'minutes':0.001,'preset':'news','enabled':True,'lastRun':None,'createdAt':900},
            {'id':'missing','profileId':'gone','everyHours':1,'minutes':1,'preset':'general','enabled':True,'lastRun':None,'createdAt':900},
            {'id':'held','profileId':'fixture','everyHours':1,'minutes':1,'preset':'general','enabled':True,'lastRun':None,'createdAt':900,'claimedBy':'other','claimedAt':999},
        ]
        sched._save(rows)
        def run(profile,store,**options):
            calls.append({'id':profile.id,**options})
            if mode=='failure': raise RuntimeError('Controlled warmup failure')
            return {'pages':1}
        args=build_parser().parse_args(['schedule','run-due','--json']);set_json_mode(True);output=io.StringIO()
        with contextlib.redirect_stdout(output),patch.object(warmup,'warm_up',run),patch('time.time',return_value=1000.0):
            try: code=args.func(args,store);error=None
            except RuntimeError as exception: code=None;error=str(exception)
        stored=sched.list();observed_calls=list(calls)
        human=[]
        from numera.cli import setup as handlers
        class Stream(io.StringIO):
            def __init__(self,tty): super().__init__();self.tty=tty
            def isatty(self): return self.tty
        for tty in [False,True]:
            sched._save(rows);calls.clear();stdout=Stream(tty);payload=[];set_json_mode(False)
            args=build_parser().parse_args(['schedule','run-due'])
            def emit(value,code=0): payload.append(value);return code
            with patch('sys.stdout',stdout),patch.object(handlers,'emit',side_effect=emit), \
                    patch.object(warmup,'warm_up',run),patch('time.time',return_value=1000.0):
                try: human_code=args.func(args,store);human_error=None
                except RuntimeError as exception: human_code=None;human_error=str(exception)
            human.append({'tty':tty,'stdout':stdout.getvalue(),'exit':human_code,'error':human_error,
                          'payload':payload[0] if payload else None})
        result.append({'mode':mode,'rows':rows,'stored':stored,'calls':observed_calls,'error':error,
                       'exit':code,'payload':json.loads(output.getvalue()) if output.getvalue() else None,'human':human})
    return result
