"""Run original CLI profile handlers with fixed clock/id and isolated real stores."""
from __future__ import annotations

import contextlib
import io
import json
from pathlib import Path
from unittest.mock import patch
from baseline_source import activate
activate()

from numera.cli import profiles as handlers
from numera.cli.parser import build_parser
from numera.cli.out import set_json_mode
from numera.models import Profile
from numera.store import ProfileStore


def cases(parent, human=False):
    parent = Path(parent)
    parent.mkdir(parents=True, exist_ok=True)
    result = []
    operations = [
        ['create','Fixture','--seed','42','--engine','playwright','--tag','one','--proxy','http://user:synthetic@proxy.test:8080','--country','US','--json'],
        ['create','Fixture','--seed','43','--engine','camoufox','--json'],
        ['list','--json'], ['list','--tag','absent','--json'],
        ['show','Fixture','--json'], ['check','Fixture','--json'],
        ['edit','Fixture','--notes','changed','--add-tag','one','--add-tag','two','--json'],
        ['edit','Fixture','--proxy','http://user:synthetic@proxy.test:8080','--country','US','--json'],
        ['list','--json'], ['show','missing','--json'], ['check','missing','--json'],
        ['edit','missing','--notes','no-write','--json'], ['collisions','--json'],
        ['apply','--all','--notes','bulk-change','--add-tag','three','--locale','de-DE','-y','--json'],
        ['apply','--all','--notes','bulk-change','-y','--json'],
        ['apply','--all','-y','--json'],
        ['apply','missing','--notes','no-write','-y','--json'],
        ['health','--json'], ['stop','missing','--json'], ['stop','Fixture','--json'],
        ['delete','missing','-y','--json'], ['delete','Fixture','missing','-y','--json'],
        ['delete','Fixture','-y','--json'], ['list','--json'],
    ]
    root = parent/'store'
    store = ProfileStore(root)
    parser = build_parser()
    ids = iter(('fixture', 'duplicatefixture'))
    def profile(**kwargs):
        kwargs.update(id=next(ids), created_at=1000.0, updated_at=1000.0)
        return Profile(**kwargs)
    for argv in operations:
        args = parser.parse_args(argv)
        set_json_mode(not human)
        output = io.StringIO();payload=[]
        def emit(value,code=0): payload.append(value);return code
        with contextlib.ExitStack() as stack:
            stack.enter_context(contextlib.redirect_stdout(output))
            stack.enter_context(patch.object(handlers,'Profile',side_effect=profile))
            stack.enter_context(patch('numera.drivers.is_installed',return_value=True))
            stack.enter_context(patch('time.time',return_value=1000.0))
            if human:
                stack.enter_context(patch.object(handlers,'emit',side_effect=emit))
                stack.enter_context(patch('numera.cli.out.emit',side_effect=emit))
                stack.enter_context(patch('numera.cli.browser.emit',side_effect=emit))
            code = args.func(args,store)
        persisted = {kind: json.loads((root/kind/'fixture.json').read_text(encoding='utf-8'))['proxy'] if (root/kind/'fixture.json').exists() else None for kind in ('profiles','dashboard')}
        result.append({'argv':argv,'exit':code,'payload':payload[0] if human else json.loads(output.getvalue()),'persisted_proxy':persisted,'stdout':output.getvalue() if human else None})
    return result


def human_cases(parent):
    from numera.fingerprint import generate
    from numera.cli import out
    class Stream(io.StringIO):
        def __init__(self,tty): super().__init__();self.tty=tty
        def isatty(self): return self.tty
    root=Path(parent)/'human';store=ProfileStore(root);parser=build_parser();result=[]
    for tty in [False,True]:
        for command in ['show','list','check']:
            profile=Profile(id='fixture',name='مرحبا Fixture',fingerprint=generate(seed=42),
                            tags=['one','two'],notes='Fixture notes',created_at=1000.0,updated_at=1000.0)
            store.save(profile)
            argv=[command] if command=='list' else [command,'fixture']
            args=parser.parse_args(argv);stdout=Stream(tty);set_json_mode(False)
            with patch('sys.stdout',stdout): code=args.func(args,store)
            result.append({'command':command,'tty':tty,'profile':profile.to_dict(),'stdout':stdout.getvalue(),'exit':code})
    for full in [False,True]:
        stdout=Stream(False)
        with patch('sys.stdout',stdout): out._print_summary(profile,full=full)
        result.append({'command':'summary','full':full,'tty':False,'profile':profile.to_dict(),'stdout':stdout.getvalue(),'exit':0})
    for tty in [False,True]:
        for variant in ['long-list','incoherent','empty','create','create-missing-engine','regen','edit','import','restore','export','backup']:
            isolated=ProfileStore(Path(parent)/f'human-{tty}-{variant}')
            profile=Profile(id='fixture',name='مرحبا Fixture with a deliberately long name',fingerprint=generate(seed=42),
                            tags=['one'],notes='Fixture notes',created_at=1000.0,updated_at=1000.0)
            if variant=='incoherent': profile.fingerprint.user_agent='Invalid fixture user agent'
            if variant not in ('empty','create','create-missing-engine'): isolated.save(profile)
            command={'long-list':'list','empty':'list','incoherent':'check','create-missing-engine':'create'}.get(variant,variant)
            argv=[command] if command=='list' else [command,'fixture']
            if command=='create': argv+=['--seed','42','--engine','playwright']
            if command in ('import','restore'): argv=[command,'fixture-file']
            if command in ('export','backup'): argv+=['fixture-file']
            args=parser.parse_args(argv);stdout=Stream(tty);payload=[];set_json_mode(False)
            def emit(value,code=0): payload.append(value);return code
            def constructed(**kwargs):
                kwargs.update(id='fixture',created_at=1000.0,updated_at=1000.0)
                return Profile(**kwargs)
            with patch('sys.stdout',stdout),patch.object(handlers,'emit',side_effect=emit), \
                    patch.object(handlers,'Profile',side_effect=constructed), \
                    patch.object(handlers,'generate',return_value=generate(seed=42)), \
                    patch('numera.drivers.is_installed',return_value=variant!='create-missing-engine'),patch('time.time',return_value=1000.0), \
                    patch.object(isolated,'import_file',return_value=profile),patch.object(isolated,'restore',return_value=profile), \
                    patch.object(isolated,'export',return_value=Path('fixture-file')),patch.object(isolated,'backup',return_value=Path('fixture-file')):
                code=args.func(args,isolated)
            result.append({'command':command,'tty':tty,'direct':True,'payload':payload[0],'stdout':stdout.getvalue(),'exit':code})
        for command in ['show','check','edit','regen','export','backup']:
            argv=[command,'missing']
            if command in ('export','backup'): argv+=['fixture-file']
            args=parser.parse_args(argv);stdout=Stream(tty);payload=[];set_json_mode(False)
            with patch('sys.stdout',stdout),patch('numera.cli.out.emit',side_effect=emit):
                code=args.func(args,isolated)
            result.append({'command':command,'tty':tty,'direct':True,'payload':payload[0],'stdout':stdout.getvalue(),'exit':code})
        for variant in ['health','health-empty','collisions','collisions-empty','bulk-create','bulk-clash']:
            isolated=ProfileStore(Path(parent)/f'fleet-{tty}-{variant}')
            profile=Profile(id='fixture',name='مرحبا Fixture',fingerprint=generate(seed=42),created_at=1000.0,updated_at=1000.0)
            if variant in ('health','collisions'): isolated.save(profile)
            if variant=='collisions': isolated.save(Profile(id='second',name='Second',fingerprint=generate(seed=42)))
            if variant=='bulk-clash': isolated.save(Profile(id='held',name='Profile0001',fingerprint=generate(seed=43)))
            command=variant.split('-empty')[0] if variant.endswith('-empty') else 'bulk-create' if variant=='bulk-clash' else variant
            argv=[command]
            if command=='bulk-create': argv+=['--count','1','--seed','42']
            args=parser.parse_args(argv);stdout=Stream(tty);payload=[];set_json_mode(False)
            with patch('sys.stdout',stdout),patch.object(handlers,'emit',side_effect=emit),patch('time.time',return_value=1000.0):
                code=args.func(args,isolated)
            result.append({'command':command,'tty':tty,'direct':True,'fleet':True,'payload':payload[0],'stdout':stdout.getvalue(),'exit':code})
    return result
