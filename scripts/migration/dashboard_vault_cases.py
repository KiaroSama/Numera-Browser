"""Source dashboard proxy intent under locked/unlocked and conflicting stores."""
from __future__ import annotations

import json
import tempfile
from pathlib import Path
from unittest.mock import patch
from cryptography.fernet import Fernet
from numera import crypto
from numera.fingerprint import generate
from numera.models import Profile, Proxy
from numera.server.convert import to_dashboard_profile
from numera.server.stores import DashboardStore
from numera.store import ProfileStore


def cases(parent: Path) -> list[dict]:
    password,salt='fixture-dashboard-master','AAECAwQFBgcICQoLDA0ODw=='
    cipher=Fernet(crypto._derive_key(password,salt))
    def seal(key,used_salt,value):
        assert key==password and used_salt==salt
        return 'enc:v2:'+cipher._encrypt_from_parts(value.encode('utf-8'),1700000000,bytes(range(16))).decode('ascii')
    config={'vault':{'salt':salt,'verifier':seal(password,salt,'numera-vault-ok')}}
    profile=Profile(id='fixture-vault',name='Vault α',fingerprint=generate(seed=42,os='windows'),
                    proxy=Proxy(server='http://127.0.0.1:8080',password='fixture-original'),
                    created_at=1700000000.0,updated_at=1700000000.0).to_dict()
    dashboard=to_dashboard_profile(Profile.from_dict(profile))
    dashboard.update(id='fixture-vault',_rev=2,_created=1700000000.0,future=7)
    variants=[('locked-preserve',None,'preserve',False),('locked-clear',None,'clear',False),
              ('locked-replace',None,'replace',False),('locked-remove',None,'remove',False),
              ('unlocked-clear',password,'clear',False),('unlocked-replace',password,'replace',False),
              ('unlocked-preserve-conflict',password,'preserve',True),('locked-preserve-conflict',None,'preserve',True),
              ('unlocked-replace-conflict',password,'replace',True),('unlocked-remove',password,'remove',False)]
    parent.mkdir(parents=True,exist_ok=True)
    found=[]
    original_profile=Profile
    def converted_profile(**values):
        values.setdefault('created_at',1700000000.0);values.setdefault('updated_at',1700000000.0)
        return original_profile(**values)
    with patch('numera.crypto.encrypt',side_effect=seal),patch('numera.convert.Profile',side_effect=converted_profile),patch('numera.server.stores.time.time',return_value=1700000000.0):
        for name,key,intent,conflict in variants:
            with tempfile.TemporaryDirectory(prefix='dashboard-vault-oracle-',dir=parent) as directory:
                root=Path(directory);store=ProfileStore(root,password=key);dash=DashboardStore(root,store)
                engine=json.loads(json.dumps(profile));engine['proxy']['password']=seal(password,salt,'fixture-original')
                stored=json.loads(json.dumps(dashboard));stored['proxy']['pass']=seal(password,salt,'fixture-other' if conflict else 'fixture-original')
                (root/'config.json').write_text(json.dumps(config),encoding='utf-8')
                (root/'profiles/fixture-vault.json').write_text(json.dumps(engine),encoding='utf-8')
                (root/'dashboard/fixture-vault.json').write_text(json.dumps(stored),encoding='utf-8')
                request=dash.get('fixture-vault');request['notes']='changed notes'
                if intent=='clear':request['proxy']['pass']=''
                elif intent=='replace':request['proxy']['pass']='fixture-replacement'
                elif intent=='remove':request['proxy']=None
                before={path: (root/path).read_bytes() for path in ('profiles/fixture-vault.json','dashboard/fixture-vault.json')}
                incoming=json.loads(json.dumps(request))
                try:
                    result,error=dash.save(request,secret_intent=intent),None
                except (RuntimeError,ValueError) as exc:result,error=None,str(exc)
                written={path:json.loads((root/path).read_text(encoding='utf-8')) for path in before}
                observed=json.loads(json.dumps(written))
                for path,record in observed.items():
                    if isinstance(record.get('proxy'),dict):
                        field='password' if path.startswith('profiles/') else 'pass'
                        value=record['proxy'].get(field)
                        if crypto.is_supported_envelope(value):record['proxy'][field]=crypto.decrypt(password,salt,value)
                found.append({'name':name,'initial':{'profiles/fixture-vault.json':engine,'dashboard/fixture-vault.json':stored},
                              'input':incoming,'config':config,'password':key,'intent':intent,'result':result,'error':error,
                              'observed':observed,'listed':dash.get('fixture-vault'),
                              'unchanged':all((root/path).read_bytes()==data for path,data in before.items()),
                              'engine_secret_unchanged':written['profiles/fixture-vault.json'].get('proxy',{}).get('password')==engine['proxy']['password'] if written['profiles/fixture-vault.json'].get('proxy') else False,
                              'dashboard_secret_unchanged':written['dashboard/fixture-vault.json'].get('proxy',{}).get('pass')==stored['proxy']['pass'] if written['dashboard/fixture-vault.json'].get('proxy') else False})
    return found
