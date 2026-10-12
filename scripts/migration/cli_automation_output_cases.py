"""Original token/schedule handlers observed without credential generation or browser work."""
from __future__ import annotations

import io
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch
from numera.cli.parser import build_parser
from numera.cli.out import set_json_mode
from numera.cli import setup as handlers
from numera.models import Profile
from numera.fingerprint import generate


def cases(parent):
    from numera.server import DuplicateToken
    class Stream(io.StringIO):
        def __init__(self,tty): super().__init__();self.tty=tty
        def isatty(self): return self.tty
    tokens=[{'id':'fixture','name':'Token ع 😀 with a deliberately long name','created':1000.9,'lastUsed':None,'hash':'fixture-hash'},
            {'id':'used','name':'Used','created':1000,'lastUsed':2000,'hash':'fixture-hash'}]
    row={'id':'scheduled','profileId':'fixture','everyHours':1.0,'minutes':5.0,'preset':'general','enabled':True,'lastRun':None,'createdAt':900}
    profile=Profile(id='fixture',name='Fixture',fingerprint=generate(seed=42))
    store=SimpleNamespace(root=Path(parent),list=lambda:[profile],resolve=lambda ref:profile if ref=='fixture' else None)
    variants=[('token-empty',['token','list']),('token-env',['token','list']),('token-list',['token','list']),
              ('token-create',['token','create','Fixture']),('token-duplicate',['token','create','Fixture']),
              ('token-revoke',['token','revoke','fixture']),('token-missing',['token','revoke','missing']),
              ('schedule-empty',['schedule','list']),('schedule-list',['schedule','list']),
              ('schedule-add',['schedule','add','fixture','--every','1']),('schedule-missing',['schedule','add','missing','--every','1']),
              ('schedule-update',['schedule','update','scheduled','--minutes','5']),('schedule-nochange',['schedule','update','scheduled']),
              ('schedule-missing-update',['schedule','update','missing','--minutes','5']),
              ('schedule-remove',['schedule','remove','scheduled']),('schedule-missing-remove',['schedule','remove','missing'])]
    parser=build_parser();result=[]
    for tty in [False,True]:
        for variant,argv in variants:
            payload=[];stdout=Stream(tty);set_json_mode(False)
            def emit(value,code=0): payload.append(value);return code
            def create(name):
                if variant=='token-duplicate': raise DuplicateToken("a token called 'Fixture' already exists")
                return {'id':'fixture','name':name,'token':'synthetic-output-fixture','created':1000}
            token_store=SimpleNamespace(list=lambda:tokens if variant=='token-list' else [],create=create,revoke=lambda id:id!='missing')
            schedules=[row,{**row,'id':'future','profileId':'gone','enabled':False,'lastRun':1000}] if variant=='schedule-list' else []
            schedule_store=SimpleNamespace(list=lambda:schedules,add=lambda **kwargs:row,update=lambda id,patch:None if id=='missing' else row,delete=lambda id:id!='missing')
            args=parser.parse_args(argv)
            with patch('sys.stdout',stdout),patch.object(handlers,'emit',side_effect=emit), \
                    patch('numera.server.TokenStore',return_value=token_store),patch('numera.scheduler.ScheduleStore',return_value=schedule_store), \
                    patch('time.time',return_value=1000.0),patch.dict('os.environ',{'NUMERA_API_TOKEN':'synthetic-fixture' if variant=='token-env' else ''}):
                code=args.func(args,store)
            result.append({'argv':argv,'tty':tty,'payload':payload[0],'stdout':stdout.getvalue(),'exit':code,
                           'resolvedName':profile.name if variant=='schedule-add' else None})
    return result
