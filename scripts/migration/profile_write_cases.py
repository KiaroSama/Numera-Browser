"""Original HTTP profile writes, deterministic inputs and complete disk outcomes."""
from __future__ import annotations
import copy
import json
import os
import tempfile
import uuid
from pathlib import Path
from unittest.mock import patch
from fastapi.testclient import TestClient
from numera.server import create_app
from numera.server.stores import DashboardStore
from numera.store import ProfileStore


def cases(parent: Path) -> list[dict]:
    parent.mkdir(parents=True,exist_ok=True)
    variants = [
        ('create-minimal','POST','/api/profiles',{'name':'Fixture','seed':42},None),
        ('create-explicit','POST','/api/profiles',{'name':'Fixture','seed':42,'engine':'patchright'},None),
        ('create-default','POST','/api/profiles',{'name':'Fixture','seed':42},'camoufox'),
        ('invalid-name','POST','/api/profiles',{'name':False},None),
        ('unknown-field','POST','/api/profiles',{'future':1},None),
        ('invalid-number','POST','/api/profiles',{'cores':1.5},None),
        ('unknown-engine','POST','/api/profiles',{'engine':'unknown'},None),
        ('owned-fields','POST','/api/profiles',{'id':'foreign','name':'Fixture','seed':42,'_created':1,'status':'running'},None),
        ('rename','PUT','/api/profiles/existing',{'name':'Renamed'},None),
        ('missing-update','PUT','/api/profiles/absent',{'name':'Renamed'},None),
        ('stale-update','PUT','/api/profiles/existing',{'name':'Renamed','_rev':0},None),
        ('update-engine','PUT','/api/profiles/existing',{'engine':'patchright'},None),
        ('update-null-proxy','PUT','/api/profiles/existing',{'proxy':None},None),
        ('duplicate-name','POST','/api/profiles',{'name':'Existing','seed':42},None),
    ]
    found=[]
    for name,method,url,body,default in variants:
        with tempfile.TemporaryDirectory(prefix='profile-write-oracle-',dir=parent) as directory:
            root=Path(directory)
            with patch.dict(os.environ,{'NUMERA_API_TOKEN':'fixture-api','NUMERA_ALLOWED_ORIGINS':''}), patch('numera.convert.time.time',return_value=1700000000.0), patch('numera.server.routes_profiles.uuid.uuid4',return_value=uuid.UUID(hex='00000000004200000000000000000000')):
                app=create_app(data_dir=directory,start_scheduler=False)
                if default:
                    (root/'config.json').write_text(json.dumps({'default_engine':default}),encoding='utf-8')
                if method=='PUT' or name=='duplicate-name':
                    DashboardStore(root,ProfileStore(root)).save({'id':'existing','name':'Existing','seed':42,'engine':'camoufox','notes':'Retained note','tags':['retained']})
                initial={f'{kind}/{path.name}':json.loads(path.read_text(encoding='utf-8')) for kind in ['dashboard','profiles'] for path in (root/kind).glob('*.json')}
                config=json.loads((root/'config.json').read_text(encoding='utf-8')) if (root/'config.json').exists() else {}
                with TestClient(app,raise_server_exceptions=False) as client:
                    response=client.request(method,url,json=copy.deepcopy(body),headers={'authorization':'Bearer fixture-api'})
            written={f'{kind}/{path.name}':json.loads(path.read_text(encoding='utf-8')) for kind in ['dashboard','profiles'] for path in (root/kind).glob('*.json')}
            found.append({'name':name,'method':method,'url':url,'input':body,'initial':initial,'config':config,'status':response.status_code,
                          'body':response.json() if response.headers.get('content-type','').startswith('application/json') else response.text,
                          'written':written,'after_config':json.loads((root/'config.json').read_text(encoding='utf-8')) if (root/'config.json').exists() else {}})
    return found
