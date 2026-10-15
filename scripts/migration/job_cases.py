"""Observe original durable-job argument and state decisions with OS queries isolated."""
from __future__ import annotations
import contextlib
import io
import json
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch
from baseline_source import activate
activate()

from numera import jobs


def cases(root):
    root=Path(root)
    (root/'jobs').mkdir(parents=True,exist_ok=True)
    log=root/'jobs'/'sample.log'
    content=b'first\r\nsecond\rthird\n\n'+ 'four\u0085five six '.encode('utf-8') + b'bad\xff\xe2\x82\nlast'
    log.write_bytes(content)
    tails=[{'lines':n,'expected':jobs.tail({'log':str(log)},n)} for n in (1,3,20,0,-2)]
    arguments=[[],['list','--json'],['--password','synthetic','launch','a','--detach','--json'],
               ['--password=synthetic','launch','a','--detach'],['--data-dir','chosen','list'],
               ['--data-dir=chosen','list'],['pool','edit','a','--pass','synthetic-proxy'],
               ['list','--password']]
    records=[({},False,False),({'started':990},False,False),({'started':900},False,False),
             ({'pid':12,'pidToken':34,'started':900},True,False),
             ({'pid':12,'pidToken':34,'started':900},False,True),
             ({'pid':12,'pidToken':34,'state':'done','exit':0},False,True),
             ({'state':'failed','exit':1},False,False),({'state':'stopped'},True,False)]
    states=[]
    for record,leader,survivors in records:
        with patch.object(jobs,'alive',return_value=leader),patch('numera.proctree.owned',return_value=survivors), \
                patch('numera.proctree.alive_tree',return_value=survivors),patch('time.time',return_value=1000.0):
            expected=jobs._decorate(record)
        states.append({'record':record,'leader':leader,'survivors':survivors,'expected':expected})
    from numera.cli.parser import build_parser
    from numera.cli.out import set_json_mode
    record={'id':'sample','command':'check','pid':None,'started':900.0,'log':str(log)}
    (root/'jobs'/'sample.json').write_text(json.dumps(record),encoding='utf-8')
    with patch('time.time',return_value=1000.0):
        jobs.record_exit(str(root/'jobs'/'sample.json'),0)
    commands=[]
    for argv in (['jobs','show','sample','--lines','3','--json'],['jobs','list','--json'],
                 ['jobs','stop','sample','--json'],['jobs','show','missing','--json'],
                 ['jobs','clear','--json'],['jobs','list','--json']):
        args=build_parser().parse_args(argv)
        set_json_mode(True)
        output=io.StringIO()
        with contextlib.redirect_stdout(output),patch('time.time',return_value=1000.0):
            code=args.func(args,SimpleNamespace(root=root))
        payload=json.loads(output.getvalue())
        def normalize(value):
            if isinstance(value,list): return [normalize(v) for v in value]
            if isinstance(value,dict): return {k:(Path(v).name if k=='log' and isinstance(v,str) else normalize(v)) for k,v in value.items()}
            return value
        commands.append({'argv':argv,'exit':code,'payload':normalize(payload)})
    return {'arguments':[{'argv':argv,'expected':jobs.child_argv(argv,Path('fixture-root'))} for argv in arguments], 'states':states,'log':content.hex(),'tails':tails,'commands':commands}


def human_cases():
    from numera.cli import browser as handlers
    from numera.cli.parser import build_parser
    from numera.cli.out import set_json_mode
    class Stream(io.StringIO):
        def __init__(self,tty): super().__init__();self.tty=tty
        def isatty(self): return self.tty
    rows=[{'id':'fixture','state':'done','running':False,'exit':0,'argv':['check','fixture'],
           'command':'check','profile':'fixture','pid':None,'log':'fixture.log','verified':False},
          {'id':'orphan','state':'done','running':True,'exit':0,'command':'launch','profile':None,'orphaned':True}]
    result=[]
    for tty in [False,True]:
        for sub in ['list','empty','show','missing','stop','clear']:
            argv=['jobs','list'] if sub in ('list','empty') else ['jobs',sub,'fixture'] if sub in ('show','missing','stop') else ['jobs','clear']
            if sub=='missing': argv=['jobs','show','missing']
            args=build_parser().parse_args(argv);output=Stream(tty);payload=[];set_json_mode(False)
            def emit(value,code=0): payload.append(value);return code
            with patch('sys.stdout',output),patch.object(handlers,'emit',side_effect=emit), \
                    patch.object(jobs,'listing',return_value=[] if sub=='empty' else rows), \
                    patch.object(jobs,'get',return_value=None if sub=='missing' else rows[0]), \
                    patch.object(jobs,'stop',return_value={**rows[0],'state':'stopped'}), \
                    patch.object(jobs,'prune',return_value=2),patch.object(jobs,'tail',return_value=['first','مرحبا']):
                code=args.func(args,SimpleNamespace(root=Path('fixture-root')))
            result.append({'argv':argv,'tty':tty,'payload':payload[0],'stdout':output.getvalue(),'exit':code})
    return result
