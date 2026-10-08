"""Public source lookup/coherent outcomes through real dual-store records."""
from __future__ import annotations

import json
import tempfile
from pathlib import Path
from unittest.mock import patch
from numera.fingerprint import generate
from numera.models import Profile
from numera.store import ProfileStore
from numera.repo import ProfileRepo
from numera.convert import to_dashboard_profile


def cases(parent: Path) -> list[dict]:
    base = Profile(id='fixture-a', name='alpha', fingerprint=generate(seed=42, os='windows'),
                   created_at=1700000000.0, updated_at=1700000000.0).to_dict()
    twin = to_dashboard_profile(Profile.from_dict(base))
    twin.update(id='fixture-a', _rev=1, _created=1700000000.0)
    variants = [('resolve', 'fixture-a', {'profiles/fixture-a.json': base}),
                ('resolve', 'alpha', {'dashboard/fixture-a.json': twin}),
                ('resolve', 'missing', {'profiles/fixture-a.json': base}),
                ('resolve', 'alpha', {'profiles/fixture-a.json': base, 'profiles/fixture-b.json': {**base, 'id':'fixture-b'}}),
                ('coherent', 'fixture-a', {'profiles/fixture-a.json': base, 'dashboard/fixture-a.json': {**twin, 'notes':'dashboard notes'}}),
                ('coherent', 'missing', {}),
                ('list', None, {'profiles/fixture-a.json': base, 'profiles/broken.json': 'broken-json'}),
                ('list', None, {'profiles/fixture-a.json': {**base, 'proxy': {'server': 'http://127.0.0.1:8080', 'password': 'fixture-engine'}},
                                'dashboard/fixture-a.json': {**twin, 'proxy': {'host': '127.0.0.1', 'port': 8080, 'type': 'http', 'pass': 'fixture-dashboard'}}}),
                ('resolve', 'fixture-a', {'profiles/fixture-a.json': {**base, 'proxy': {'server': 'http://127.0.0.1:8080', 'password': 'fixture-engine'}},
                                        'dashboard/fixture-a.json': {**twin, 'proxy': {'host': '127.0.0.1', 'port': 8080, 'type': 'http', 'pass': 'fixture-dashboard'}}}),
                ('list', None, {'profiles/fixture-a.json': base, 'dashboard/fixture-a.json': {'name': []}})]
    parent.mkdir(parents=True, exist_ok=True)
    found = []
    original = Profile
    def clocked(*args, **kwargs):
        kwargs.setdefault('created_at', 1700000000.0)
        kwargs.setdefault('updated_at', 1700000000.0)
        return original(*args, **kwargs)
    for operation, reference, files in variants:
        with tempfile.TemporaryDirectory(prefix='lookup-oracle-', dir=parent) as directory:
            root = Path(directory)
            store = ProfileStore(root)
            for name, data in files.items():
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text('{broken' if data == 'broken-json' else json.dumps(data), encoding='utf-8')
            with patch('numera.convert.Profile', side_effect=clocked), patch('numera.repo.time.time', return_value=1700000000.0), patch('numera.models.time.time', return_value=1700000000.0):
                try:
                    value = ProfileRepo(store).coherent(reference) if operation == 'coherent' else store.list() if operation == 'list' else store.resolve(reference)
                    result = [profile.to_dict() for profile in value] if isinstance(value, list) else value.to_dict() if value else None
                    error = None
                except (RuntimeError, ValueError) as exc:
                    result, error = None, str(exc)
            written = {}
            for kind in ('profiles', 'dashboard'):
                for path in sorted((root / kind).glob('*.json')):
                    try: data = json.loads(path.read_text(encoding='utf-8'))
                    except json.JSONDecodeError: data = 'broken-json'
                    written[path.relative_to(root).as_posix()] = data
            found.append({'operation':operation, 'reference':reference, 'files':files, 'result':result, 'error':error, 'written':written})
    return found
