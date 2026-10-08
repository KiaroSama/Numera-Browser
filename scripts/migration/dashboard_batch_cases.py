"""Retained all-or-nothing dashboard batch and transaction-time patch results."""
from __future__ import annotations

import json
import tempfile
from pathlib import Path
from unittest.mock import patch
from numera.fingerprint import generate
from numera.models import Profile
from numera.server.convert import to_dashboard_profile
from numera.server.stores import DashboardStore
from numera.store import ProfileStore


def cases(parent: Path) -> list[dict]:
    base=to_dashboard_profile(Profile(id='fixture-a',name='alpha',fingerprint=generate(seed=42,os='windows'),
                                     created_at=1700000000.0,updated_at=1700000000.0))
    parent.mkdir(parents=True,exist_ok=True)
    found=[]
    original_profile=Profile
    def converted_profile(**values):
        values.setdefault('created_at',1700000000.0);values.setdefault('updated_at',1700000000.0)
        return original_profile(**values)
    variants=('new','duplicate','invalid-later','empty-config','patch','same-id','same-id-stored','same-id-secret','invalid-first-duplicate')
    with patch('numera.convert.Profile',side_effect=converted_profile),patch('numera.server.stores.time.time',return_value=1700000000.0):
        for variant in variants:
            with tempfile.TemporaryDirectory(prefix='dashboard-batch-oracle-',dir=parent) as directory:
                root=Path(directory);engine=ProfileStore(root);dash=DashboardStore(root,engine)
                (root/'config.json').write_text(json.dumps({'future':7}),encoding='utf-8')
                inputs=[{**base,'id':f'fixture-{index}','name':name,'future':index} for index,name in enumerate(['alpha','alpha' if variant=='duplicate' else 'beta'])]
                if variant=='invalid-first-duplicate':
                    inputs[0]['cores']=1.5;inputs[1]['name']='alpha'
                if variant=='same-id-secret':
                    inputs[0]['proxy']={'type':'http','host':'127.0.0.1','port':'8080','user':'fixture','pass':'fixture-first','country':'US'}
                    inputs[1]['proxy']={**inputs[0]['proxy'],'pass':None}
                if variant in ('same-id','same-id-stored','same-id-secret'):
                    inputs[1]['id']=inputs[0]['id']
                    if variant=='same-id-stored':dash.save(json.loads(json.dumps(inputs[0])))
                if variant=='empty-config':inputs=[]
                if variant=='invalid-later':inputs[1]['cores']=1.5
                if variant=='patch':
                    dash.save_many(inputs)
                    for path in (root/'dashboard').glob('*.json'):
                        record=json.loads(path.read_text(encoding='utf-8'));record['notes']='concurrent committed notes';path.write_text(json.dumps(record),encoding='utf-8')
                initial={path.relative_to(root).as_posix():json.loads(path.read_text(encoding='utf-8'))
                         for kind in ('profiles','dashboard') for path in sorted((root/kind).glob('*.json'))}
                before={name:(root/name).read_bytes() for name in initial}
                config_before=(root/'config.json').read_bytes()
                request=json.loads(json.dumps(inputs))
                try:
                    if variant=='patch':
                        def apply(pid,record):
                            record['tags']=['applied'];return record
                        result=dash.patch_many(['fixture-0','missing','fixture-1'],apply,config_change={'default_engine':'patchright'},secret_intent='preserve')
                    else:result=dash.save_many(inputs,config_change={'default_engine':'patchright'})
                    error=None
                except (RuntimeError,ValueError) as exc:result,error=None,str(exc)
                written={path.relative_to(root).as_posix():json.loads(path.read_text(encoding='utf-8'))
                         for kind in ('profiles','dashboard') for path in sorted((root/kind).glob('*.json'))}
                found.append({'name':variant,'input':request,'initial':initial,'result':result,'error':error,'written':written,
                              'config':json.loads((root/'config.json').read_text(encoding='utf-8')),
                              'config_unchanged':config_before==(root/'config.json').read_bytes(),
                              'original_unchanged':all((root/name).read_bytes()==data for name,data in before.items())})
    return found
