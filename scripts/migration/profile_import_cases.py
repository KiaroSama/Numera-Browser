"""Run retained JSON import through the original dual-store coordinator."""
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
    base = Profile(id='fixture-import', name='import α', fingerprint=generate(seed=42, os='windows'),
                   proxy=Proxy(server='http://127.0.0.1:8080', password='fixture-import-secret'),
                   created_at=1700000000.0, updated_at=1700000000.0).to_dict()
    parent.mkdir(parents=True, exist_ok=True)
    found = []
    for variant in ('new', 'overwrite', 'taken-name', 'unknown-fields', 'bad-name'):
        with tempfile.TemporaryDirectory(prefix='import-oracle-', dir=parent) as directory:
            root = Path(directory)
            store = ProfileStore(root)
            repo = ProfileRepo(store)
            record = json.loads(json.dumps(base))
            if variant == 'unknown-fields':
                record['future'] = 7
                record['proxy']['future'] = True
            if variant == 'bad-name':
                record['name'] = []
            with patch('numera.repo.time.time', return_value=1700000000.0):
                if variant in ('overwrite', 'taken-name'):
                    existing = {**base, 'notes':'original notes'}
                    if variant == 'taken-name': existing['id'] = 'fixture-other'
                    repo.create(Profile.from_dict(existing))
                initial = {path.relative_to(root).as_posix():json.loads(path.read_text(encoding='utf-8'))
                           for kind in ('profiles','dashboard') for path in sorted((root / kind).glob('*.json'))}
                before = {name:(root / name).read_bytes() for name in initial}
                source = root / 'incoming.json'
                source.write_text(json.dumps(record), encoding='utf-8')
                try:
                    result,error = store.import_file(source).to_dict(),None
                except (RuntimeError,ValueError) as exc:
                    result,error = None,str(exc)
                after = {path.relative_to(root).as_posix():json.loads(path.read_text(encoding='utf-8'))
                         for kind in ('profiles','dashboard') for path in sorted((root / kind).glob('*.json'))}
            found.append({'name':variant,'input':record,'initial':initial,'result':result,'error':error,
                          'written':after,'original_unchanged':all((root / name).read_bytes() == value for name,value in before.items())})
    return found
