"""Original batch create and bulk patch HTTP outcomes with complete disk state."""
from __future__ import annotations

import copy
import json
import os
import tempfile
import uuid
from pathlib import Path
from unittest.mock import patch
from fastapi.testclient import TestClient
from numera.models import Profile
from numera.server import create_app
from numera.server.stores import DashboardStore
from numera.store import ProfileStore


def cases(parent: Path) -> list[dict]:
    parent.mkdir(parents=True, exist_ok=True)
    variants = [
        ('empty-batch','batch',{}),
        ('minimal-batch','batch',{'profiles':[{'name':'One','seed':42},{'name':'Two','seed':43}]}),
        ('mixed-engine','batch',{'profiles':[{'name':'One','seed':42,'engine':'patchright'},{'name':'Two','seed':43}]}),
        ('wrong-list','batch',{'profiles':'invalid'}),
        ('wrong-entry','batch',{'profiles':[{'name':'One'},17]}),
        ('bad-field','batch',{'profiles':[{'name':'One','seed':42},{'name':False}]}),
        ('duplicate-name','batch',{'profiles':[{'name':'One','seed':42},{'name':'One','seed':43}]}),
        ('bad-engine','batch',{'profiles':[{'name':'One','seed':42},{'name':'Two','engine':'unknown'}]}),
        ('owned-fields','batch',{'profiles':[{'id':'foreign','_rev':17,'status':'running','name':'One','seed':42}]}),
        ('empty-bulk','bulk',{}),
        ('rename','bulk',{'ids':['a'],'patch':{'name':'Renamed'}}),
        ('tags','bulk',{'ids':['a','b'],'patch':{'addTags':['new','old'],'removeTags':['drop']}}),
        ('locale','bulk',{'ids':['a','b'],'patch':{'locale':'de-DE'}}),
        ('engine','bulk',{'ids':['a','b'],'patch':{'engine':'patchright'}}),
        ('missing','bulk',{'ids':['absent','a'],'patch':{'notes':'Changed'}}),
        ('wrong-ids','bulk',{'ids':[17],'patch':{}}),
        ('wrong-patch','bulk',{'ids':['a'],'patch':'invalid'}),
        ('bad-tags','bulk',{'ids':['a'],'patch':{'addTags':[{}],'removeTags':None}}),
        ('bad-engine-bulk','bulk',{'ids':['a','b'],'patch':{'engine':'unknown'}}),
        ('name-conflict','bulk',{'ids':['a','b'],'patch':{'name':'Same'}}),
        ('owned-patch','bulk',{'ids':['a'],'patch':{'id':'foreign','_created':0,'status':'running','notes':'Changed'}}),
    ]
    def converted_profile(**values):
        values.setdefault('created_at',1700000000.0)
        values.setdefault('updated_at',1700000000.0)
        return Profile(**values)
    found=[]
    for name,route,body in variants:
        with tempfile.TemporaryDirectory(prefix='profile-batch-oracle-',dir=parent) as directory:
            root=Path(directory)
            identifiers=iter(uuid.UUID(hex=f'{number:012x}'+'0'*20) for number in range(1,20))
            with patch.dict(os.environ,{'NUMERA_API_TOKEN':'fixture-api','NUMERA_ALLOWED_ORIGINS':''}), patch('numera.convert.time.time',return_value=1700000000.0), patch('numera.convert.Profile',side_effect=converted_profile), patch('numera.server.routes_profiles.uuid.uuid4',side_effect=lambda:next(identifiers)):
                app=create_app(data_dir=directory,start_scheduler=False)
                if route=='bulk':
                    store=DashboardStore(root,ProfileStore(root))
                    for pid in ['a','b']:
                        store.save({'id':pid,'name':pid.upper(),'seed':42,'engine':'camoufox','notes':'Retained note','tags':['old','drop']})
                initial={f'{kind}/{path.name}':json.loads(path.read_text(encoding='utf-8')) for kind in ['dashboard','profiles'] for path in (root/kind).glob('*.json')}
                with TestClient(app,raise_server_exceptions=False) as client:
                    response=client.post(f'/api/profiles/{route}',json=copy.deepcopy(body),headers={'authorization':'Bearer fixture-api'})
            written={f'{kind}/{path.name}':json.loads(path.read_text(encoding='utf-8')) for kind in ['dashboard','profiles'] for path in (root/kind).glob('*.json')}
            found.append({'name':name,'route':route,'input':body,'initial':initial,'status':response.status_code,
                          'body':response.json() if response.headers.get('content-type','').startswith('application/json') else response.text,
                          'written':written,'after_config':json.loads((root/'config.json').read_text(encoding='utf-8')) if (root/'config.json').exists() else {}})
    return found
