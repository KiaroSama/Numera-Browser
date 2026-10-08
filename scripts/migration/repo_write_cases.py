"""Original atomic dual-store write decisions, fixed non-sensitive source clock."""
from __future__ import annotations

import json
import tempfile
from pathlib import Path
from unittest.mock import patch
from numera.fingerprint import generate
from numera.models import Profile, Proxy
from numera.store import ProfileStore
from numera.repo import ProfileRepo


def cases(parent: Path) -> list[dict]:
    base=Profile(id='fixture-write',name='fixture',fingerprint=generate(seed=42,os='windows'),
                 proxy=Proxy(server='http://127.0.0.1:8080',password='fixture-initial'),
                 created_at=1700000000.0,updated_at=1700000000.0).to_dict()
    parent.mkdir(parents=True,exist_ok=True)
    found=[]
    for mutation in ('notes','clear','replace','remove','rename-clash'):
        with tempfile.TemporaryDirectory(prefix='repo-write-oracle-',dir=parent) as directory:
            root=Path(directory)
            store=ProfileStore(root)
            repo=ProfileRepo(store)
            with patch('numera.repo.time.time',return_value=1700000000.0):
                repo.create(Profile.from_dict(base))
                dashpath=root/'dashboard/fixture-write.json'
                dash=json.loads(dashpath.read_text(encoding='utf-8'))
                dash.update(folder='User folder',future={'keep':[1,'α']},_rev=3)
                dashpath.write_text(json.dumps(dash),encoding='utf-8')
                record=Profile.from_dict(base)
                if mutation=='notes':record.notes='updated notes'
                elif mutation=='clear':record.proxy.password=''
                elif mutation=='replace':record.proxy.password='fixture-replacement'
                elif mutation=='remove':record.proxy=None
                else:
                    other=Profile.from_dict({**base,'id':'fixture-other','name':'occupied'})
                    repo.create(other)
                    record.name='occupied'
                inputs={kind:json.loads((root/kind/'fixture-write.json').read_text(encoding='utf-8')) for kind in ('profiles','dashboard')}
                before={kind:(root/kind/'fixture-write.json').read_bytes() for kind in inputs}
                try:
                    saved=repo.create(record)
                    result,error=saved.to_dict(),None
                except (RuntimeError,ValueError) as exc:
                    result,error=None,str(exc)
                found.append({'mutation':mutation,'initial':inputs,'profile':record.to_dict(),
                              'intent':record.proxy.password_intent if record.proxy else 'remove',
                              'other':other.to_dict() if mutation=='rename-clash' else None,
                              'result':result,'error':error,
                              'written':{kind:json.loads((root/kind/'fixture-write.json').read_text(encoding='utf-8')) for kind in inputs},
                              'unchanged':{kind:before[kind]==(root/kind/'fixture-write.json').read_bytes() for kind in inputs}})
    return found
