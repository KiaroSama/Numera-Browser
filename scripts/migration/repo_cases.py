"""Dual-store source reconciliation from isolated non-sensitive original files."""
from __future__ import annotations

import json
import tempfile
from pathlib import Path
from unittest.mock import patch
from numera import convert
from numera.fingerprint import generate
from numera.models import Profile, Proxy
from numera.store import ProfileStore
from numera.repo import ProfileRepo


def cases(parent: Path) -> list[dict]:
    profile = Profile(id='fixture00001', name='fixture', fingerprint=generate(seed=42, os='windows'),
                      created_at=1700000000.0, updated_at=1700000001.0,
                      proxy=Proxy(server='http://127.0.0.1:8080', password='fixture-secret'))
    engine = profile.to_dict()
    dashboard = {**convert.to_dashboard_profile(profile), 'id': profile.id,
                 '_created': 1700000000.0, '_rev': 1}
    scenarios = []
    for changes in ({}, {'notes':'new notes'}, {'proxy':None},
                    {'proxy':{**dashboard['proxy'],'pass':'different-fixture-secret'}},
                    {'screen':'1366x768'}, {'name':'renamed'}):
        scenarios.append((engine,{**dashboard,**changes},'materialize'))
    scenarios.extend([(engine,None,'adopt'), (engine,dashboard,'adopt'),
                      (None,dashboard,'materialize'),(None,None,'materialize')])
    from numera import crypto
    from cryptography.fernet import Fernet
    salt='AAECAwQFBgcICQoLDA0ODw=='
    password='fixture-master'
    cipher=Fernet(crypto._derive_key(password,salt))
    def sealed(value,iv):
        return 'enc:v2:'+cipher._encrypt_from_parts(value.encode('utf-8'),1700000000,bytes([iv])*16).decode('ascii')
    descriptor={'salt':salt,'verifier':sealed('numera-vault-ok',1)}
    encrypted_engine={**engine,'proxy':{**engine['proxy'],'password':sealed('fixture-secret',2)}}
    encrypted_dash={**dashboard,'proxy':{**dashboard['proxy'],'pass':sealed('fixture-secret',3)}}
    cases_with_vault=[(e,d,op,{},None) for e,d,op in scenarios]
    cases_with_vault.extend([
        (encrypted_engine,encrypted_dash,'materialize',{'vault':descriptor},None),
        (encrypted_engine,encrypted_dash,'materialize',{'vault':descriptor},password),
        (encrypted_engine,{**encrypted_dash,'proxy':{**encrypted_dash['proxy'],'pass':sealed('different-fixture-secret',4)}},'materialize',{'vault':descriptor},password),
        (encrypted_engine,None,'adopt',{'vault':descriptor},None),
    ])
    parent.mkdir(parents=True,exist_ok=True)
    found=[]
    original = convert.Profile
    def clocked(*args,**kwargs):
        kwargs.setdefault('created_at',1700000000.0)
        kwargs.setdefault('updated_at',1700000000.0)
        return original(*args,**kwargs)
    with patch('numera.convert.Profile',side_effect=clocked), patch('numera.crypto.encrypt',side_effect=lambda master,salt,value:sealed(value,5)):
        for initial_engine,initial_dash,operation,config,master in cases_with_vault:
            with tempfile.TemporaryDirectory(prefix='repo-oracle-',dir=parent) as directory:
                root=Path(directory)
                (root/'config.json').write_text(json.dumps(config),encoding='utf-8')
                store=ProfileStore(root,password=master)
                repo=ProfileRepo(store)
                repo.dash_dir.mkdir(exist_ok=True)
                paths=[root/'profiles'/f'{profile.id}.json',root/'dashboard'/f'{profile.id}.json']
                for path,data in zip(paths,(initial_engine,initial_dash)):
                    if data is not None:
                        path.write_text(json.dumps(data),encoding='utf-8')
                before=[p.read_bytes() if p.exists() else None for p in paths]
                try:
                    out=repo.materialize_current(profile.id) if operation=='materialize' else repo.adopt_engine_record(profile.id)
                    result=out.to_dict() if isinstance(out,Profile) else out
                    error=None
                except RuntimeError as exc:
                    result,error=None,str(exc)
                after=[p.read_bytes() if p.exists() else None for p in paths]
                found.append({'operation':operation,'config':config,'password':master,'engine':initial_engine,'dashboard':initial_dash,
                              'result':result,'error':error,
                              'written':[json.loads(b) if b else None for b in after],
                              'unchanged':[a==b for a,b in zip(before,after)]})
    return found
