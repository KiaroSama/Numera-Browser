"""Original profile HTTP read view and uncertain ownership, no browser."""
from __future__ import annotations

import json
import os
import tempfile
from pathlib import Path
from unittest.mock import patch
from fastapi.testclient import TestClient
from numera.server import create_app


def cases(parent: Path) -> list[dict]:
    parent.mkdir(parents=True, exist_ok=True)
    variants = [
        ('empty', {}),
        ('ordinary', {'a.json': {'id':'a','name':'Fixture','_created':1.0,'notes':'Note'}}),
        ('legacy', {'a.json': {'id':'a','name':123,'tags':[{},'tag'],'notes':None,'folder':False}}),
        ('missing-name', {'a.json': {'id':'a','tags':'invalid'}}),
        ('damaged-json', {'a.json':'{'}),
        ('creation-order', {'a.json':{'id':'a','name':'Later','_created':2.0},'b.json':{'id':'b','name':'Earlier','_created':1.0}}),
        ('unreadable-claim', {'a.json':{'id':'a','name':'Fixture'}}),
        ('last-active', {'a.json':{'id':'a','name':'Fixture','_last_active':850.0}}),
    ]
    from numera.models import Profile
    from numera.fingerprint import generate
    original_profile = Profile(name='Engine only', id='engine-fixture', fingerprint=generate(seed=42), created_at=1000.0, updated_at=1000.0).to_dict()
    variants += [('engine-adopt', {}), ('engine-damaged', {})]
    found = []
    for name, profiles in variants:
        with tempfile.TemporaryDirectory(prefix='profile-read-oracle-', dir=parent) as directory:
            root = Path(directory)
            with patch.dict(os.environ, {'NUMERA_API_TOKEN': 'fixture-api', 'NUMERA_ALLOWED_ORIGINS': ''}):
                app = create_app(data_dir=directory, start_scheduler=False)
                engines = {'engine-fixture.json': original_profile} if name == 'engine-adopt' else {'damaged.json': '{'} if name == 'engine-damaged' else {}
                for filename, profile in engines.items():
                    (root / 'profiles' / filename).write_text(profile if isinstance(profile, str) else json.dumps(profile), encoding='utf-8')
                (root / 'dashboard').mkdir(exist_ok=True)
                for filename, profile in profiles.items():
                    (root / 'dashboard' / filename).write_text(profile if isinstance(profile, str) else json.dumps(profile), encoding='utf-8')
                if name == 'unreadable-claim':
                    (root / 'sessions').mkdir(exist_ok=True)
                    (root / 'sessions/a.json').write_text('{', encoding='utf-8')
                before = {filename:(root / 'dashboard' / filename).read_bytes() for filename in profiles}
                with patch('numera.convert.time.time', return_value=1000.0), TestClient(app, raise_server_exceptions=False) as client:
                    response = client.get('/api/profiles', headers={'authorization':'Bearer fixture-api'})
            found.append({'name':name,'profiles':profiles,'engines':engines,'claim': '{' if name == 'unreadable-claim' else None,
                          'now':1000.0,'status':response.status_code,
                          'body':response.json() if response.headers.get('content-type','').startswith('application/json') else response.text,
                          'unchanged':{filename:before[filename] == (root / 'dashboard' / filename).read_bytes() for filename in profiles},
                          'adopted': {path.name: json.loads(path.read_text(encoding='utf-8')) for path in (root / 'dashboard').glob('*.json') if path.name not in profiles}})
    return found
