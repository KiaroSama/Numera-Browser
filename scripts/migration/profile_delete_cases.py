"""Retained single/bulk profile deletion responses and original disk outcomes."""
from __future__ import annotations
import json
import os
import tempfile
from pathlib import Path
from unittest.mock import patch
from fastapi.testclient import TestClient
from numera.server import create_app
from numera.server.stores import DashboardStore
from numera.store import ProfileStore


def cases(parent:Path)->list[dict]:
    parent.mkdir(parents=True,exist_ok=True)
    variants=[('ordinary','DELETE','/api/profiles/a',None,None),('missing','DELETE','/api/profiles/absent',None,None),
              ('claim','DELETE','/api/profiles/a',None,'{'),('bulk','POST','/api/profiles/delete',{'ids':['a','absent','a']},None),
              ('bulk-claim','POST','/api/profiles/delete',{'ids':['a','absent']},'{'),
              ('bad-ids','POST','/api/profiles/delete',{'ids':[17]},None),('empty','POST','/api/profiles/delete',{'ids':[]},None)]
    from numera.models import Profile
    def converted_profile(**values):
        values.setdefault('created_at',1700000000.0)
        values.setdefault('updated_at',1700000000.0)
        return Profile(**values)
    found=[]
    for name,method,url,body,claim in variants:
        with tempfile.TemporaryDirectory(prefix='delete-http-oracle-',dir=parent) as directory:
            root=Path(directory)
            with patch.dict(os.environ,{'NUMERA_API_TOKEN':'fixture-api','NUMERA_ALLOWED_ORIGINS':''}),patch('numera.convert.time.time',return_value=1700000000.0),patch('numera.convert.Profile',side_effect=converted_profile):
                app=create_app(data_dir=directory,start_scheduler=False)
                DashboardStore(root,ProfileStore(root)).save({'id':'a','name':'A','seed':42})
                initial={f'{kind}/{path.name}':json.loads(path.read_text(encoding='utf-8')) for kind in ['dashboard','profiles'] for path in (root/kind).glob('*.json')}
                (root/'user-data/a').mkdir(parents=True,exist_ok=True)
                (root/'user-data/a/state.txt').write_text('fixture state',encoding='utf-8')
                if claim is not None:
                    (root/'sessions').mkdir(exist_ok=True)
                    (root/'sessions/a.json').write_text(claim,encoding='utf-8')
                with TestClient(app,raise_server_exceptions=False) as client:
                    response=client.request(method,url,json=body,headers={'authorization':'Bearer fixture-api'})
            written={f'{kind}/{path.name}':json.loads(path.read_text(encoding='utf-8')) for kind in ['dashboard','profiles'] for path in (root/kind).glob('*.json')}
            found.append({'name':name,'method':method,'url':url,'input':body,'claim':claim,'initial':initial,'status':response.status_code,
                          'body':response.json() if response.headers.get('content-type','').startswith('application/json') else response.text,
                          'written':written,'session_present':(root/'user-data/a/state.txt').exists(),'claim_present':(root/'sessions/a.json').exists()})
    return found
