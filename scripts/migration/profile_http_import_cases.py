"""Retained native/vendor HTTP import outcomes from isolated original stores."""
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


def cases(parent: Path) -> list[dict]:
    parent.mkdir(parents=True, exist_ok=True)
    variants = [
        ('empty',{}), ('bad-text',{'text':17}), ('bad-json',{'text':'{'}),
        ('native-empty',{'text':json.dumps({'numeraProfiles':1,'profiles':[]})}),
        ('native-minimal',{'text':json.dumps({'numeraProfiles':1,'profiles':[{'name':'Native','seed':42}]})}),
        ('native-future',{'text':json.dumps({'numeraProfiles':2,'profiles':[]})}),
        ('native-bad-version',{'text':json.dumps({'numeraProfiles':'1','profiles':[]})}),
        ('native-broken',{'text':json.dumps({'numeraProfiles':1,'profiles':[17]})}),
        ('native-bad-field',{'text':json.dumps({'numeraProfiles':1,'profiles':[{'name':False}]})}),
        ('native-proxy-redacted',{'text':json.dumps({'numeraProfiles':1,'profiles':[{'name':'Native','seed':42,'proxy':{'host':'fixture.invalid','type':'HTTP','pass':None}}]})}),
        ('vendor-empty',{'text':'[]'}),
        ('vendor-minimal',{'text':json.dumps({'name':'Vendor'})}),
        ('vendor-nested',{'text':json.dumps({'profiles':[{'name':'Nested','os':'Linux','navigator':{'language':'de-DE','hardwareConcurrency':'4','deviceMemory':'8'},'timezone':{'id':'Europe/Berlin'},'tags':'first, second','group':'group','engine':'patchright'}]})}),
        ('vendor-ios',{'text':json.dumps({'name':'Phone','os':'iOS','userAgent':'Mozilla/5.0 (iPhone; CPU iPhone OS 18_5 like Mac OS X)'})}),
        ('vendor-unusable',{'text':json.dumps({'name':'Odd','userAgent':17,'language':{},'tags':17,'cores':'8 cores','memory':'inf'})}),
        ('vendor-bad-name',{'text':json.dumps([{'name':'Good'},{'name':123}])}),
        ('vendor-proxy',{'text':json.dumps({'name':'Proxy','proxy':{'type':'socks4','host':'fixture.invalid','port':1080,'user':'fixture-user','pass':'fixture-value','country':'US'}})}),
        ('unversioned',{'text':json.dumps({'name':'Old','webglVendor':'x','webglRenderer':'x','clientRects':'Noise','webglImage':'Noise','mediaDevices':'Masked','mediaVideo':1})}),
    ]
    def converted_profile(**values):
        values.setdefault('created_at',1700000000.0)
        values.setdefault('updated_at',1700000000.0)
        values.setdefault('id','000000000042')
        return Profile(**values)
    found=[]
    for name,body in variants:
        with tempfile.TemporaryDirectory(prefix='profile-import-oracle-',dir=parent) as directory:
            root=Path(directory)
            identifiers=iter(uuid.UUID(hex=f'{number:012x}'+'0'*20) for number in range(1,20))
            with patch.dict(os.environ,{'NUMERA_API_TOKEN':'fixture-api','NUMERA_ALLOWED_ORIGINS':''}), patch('numera.convert.time.time',return_value=1700000000.0), patch('numera.convert.Profile',side_effect=converted_profile), patch('numera.importers.Profile',side_effect=converted_profile), patch('numera.fingerprint.random.randint',return_value=42), patch('numera.server.routes_profiles.uuid.uuid4',side_effect=lambda:next(identifiers)):
                app=create_app(data_dir=directory,start_scheduler=False)
                with TestClient(app,raise_server_exceptions=False) as client:
                    response=client.post('/api/profiles/import',json=copy.deepcopy(body),headers={'authorization':'Bearer fixture-api'})
            written={f'{kind}/{path.name}':json.loads(path.read_text(encoding='utf-8')) for kind in ['dashboard','profiles'] for path in (root/kind).glob('*.json')}
            found.append({'name':name,'input':body,'status':response.status_code,'body':response.json() if response.headers.get('content-type','').startswith('application/json') else response.text,'written':written,
                          'after_config':json.loads((root/'config.json').read_text(encoding='utf-8')) if (root/'config.json').exists() else {}})
    return found
