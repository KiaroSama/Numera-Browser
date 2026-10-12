"""Original non-browser administration commands, one isolated source store."""
from __future__ import annotations

import contextlib
import io
import json
from pathlib import Path
from unittest.mock import patch
from numera.cli.parser import build_parser
from numera.cli.out import set_json_mode
from numera.store import ProfileStore
from numera.models import Profile
from numera.fingerprint import generate


def cases(parent):
    parent=Path(parent)
    parent.mkdir(parents=True,exist_ok=True)
    root=parent/'store'
    store=ProfileStore(root)
    profile=Profile(id='fixture',name='Fixture',fingerprint=generate(seed=42),created_at=1000.0,updated_at=1000.0)
    store.save(profile)
    operations=[
        ['engines','--json'], ['default-engine','--json'],
        ['default-engine','patchright','--json'],['default-engine','unknown','--json'],
        ['config','--json'],['config','--headless-launch','on','--default-engine','camoufox','--json'],
        ['config','--default-engine','invalid','--headless-launch','off','--json'],
        ['folder','--json','list'],['folder','--json','create','Team'],
        ['folder','--json','rename','Team','Renamed'],['folder','--json','delete','Renamed'],
        ['folder','--json','create','   '],
        ['token','list','--json'],['token','revoke','missing','--json'],
        ['schedule','list','--json'],['schedule','add','Fixture','--every','12','--json'],
        ['schedule','update','schedulecase','--minutes','10','--enabled','off','--json'],
        ['schedule','update','schedulecase','--json'],['schedule','remove','missing','--json'],
        ['schedule','remove','schedulecase','--json'],['schedule','list','--json'],
        ['ext','list','Fixture','--json'], ['ext','list','missing','--json'],
        ['ext','remove','Fixture','missing-extension','--json'],
        ['ext','default-list','--json'], ['ext','default-remove','missing-extension','--json'],
        ['ext','installed','--json'], ['ext','uninstall','abcdefghijklmnopabcdefghijklmnop','--json'],
        ['vault','status','--json'],
    ]
    result=[]
    parser=build_parser()
    for argv in operations:
        args=parser.parse_args(argv)
        output=io.StringIO();set_json_mode(True)
        with contextlib.redirect_stdout(output),patch('numera.drivers.available',return_value={'playwright':True,'patchright':False,'camoufox':True,'cloak':False}), \
                patch('numera.drivers.is_installed',side_effect=lambda name: name in ('playwright','camoufox')), \
                patch('uuid.uuid4',return_value=type('FixedId',(),{'hex':'schedulecase'})()), \
                patch('time.time',return_value=1000.0),patch.dict('os.environ',{'NUMERA_API_TOKEN':''}):
            code=args.func(args,store)
        payload=json.loads(output.getvalue())
        if isinstance(payload,dict) and 'root' in payload:
            assert payload['root']==str(root.resolve())
            payload['root']='fixture-root'
        result.append({'argv':argv,'exit':code,'payload':payload})
    return result


def human_cases(parent):
    from numera.cli import setup as handlers
    class Stream(io.StringIO):
        def __init__(self,tty): super().__init__();self.tty=tty
        def isatty(self): return self.tty
    result=[];parser=build_parser()
    operations=[['engines'],['default-engine'],['default-engine','patchright'],['default-engine','unknown'],
                ['config'],['config','--headless-launch','on'],
                ['config','--default-extension',''],
                ['config','--default-engine','camoufox','--headless-launch','off','--default-extension',''],
                ['config','--default-engine','invalid','--headless-launch','on'],
                ['config','--default-extension',str(Path(parent)/'missing-extension')],
                ['folder','list'],['folder','create','Team'],['folder','list'],
                ['folder','create','Team'],['folder','rename','Team','Renamed'],
                ['folder','create','Destination'],['folder','delete','Renamed','--reassign-to','Destination'],
                ['folder','delete','missing'],['folder','create','   '],['folder','delete','Destination']]
    for tty in [False,True]:
        store=ProfileStore(Path(parent)/f'human-{tty}')
        for argv in operations:
            args=parser.parse_args(argv);stdout=Stream(tty);payload=[];set_json_mode(False)
            def emit(value,code=0): payload.append(value);return code
            with patch('sys.stdout',stdout),patch.object(handlers,'emit',side_effect=emit), \
                    patch('numera.drivers.available',return_value={'camoufox':False,'cloak':False,'patchright':False,'playwright':True}), \
                    patch('numera.drivers.is_installed',side_effect=lambda name:name=='playwright'), \
                    patch('numera.drivers.names',return_value=['camoufox','cloak','patchright','playwright']):
                code=args.func(args,store)
            result.append({'argv':argv,'tty':tty,'payload':payload[0],'stdout':stdout.getvalue(),'exit':code})
        for sub,enabled,unlocked in [('status',False,False),('status',True,False),('status',True,True),('enable',True,True),('change',True,True),('resume',True,True)]:
            args=parser.parse_args(['vault',sub]);stdout=Stream(tty);payload=[];set_json_mode(False)
            from types import SimpleNamespace
            fake=SimpleNamespace(_password='fixture-master',vault_enabled=lambda:enabled,unlocked=lambda:unlocked,
                                 enable_vault=lambda password:2,change_vault_password=lambda old,new:2)
            args.new_password='fixture-new-master'
            with patch('sys.stdout',stdout),patch.object(handlers,'emit',side_effect=emit),patch('numera.crypto.available',return_value=True):
                code=args.func(args,fake)
            result.append({'argv':['vault',sub],'tty':tty,'payload':payload[0],'stdout':stdout.getvalue(),'exit':code})
    return result
