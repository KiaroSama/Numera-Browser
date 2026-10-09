"""Retained post-lease profile/credential gate; no SDK or real user stores."""
from __future__ import annotations
import json
from contextlib import ExitStack
from pathlib import Path
from tempfile import TemporaryDirectory
from unittest.mock import patch

from numera import drivers
from numera.fingerprint import generate
from numera.models import Profile
from numera.store import ProfileStore


def cases(parent: Path):
    parent.mkdir(parents=True, exist_ok=True)
    result = []
    export = json.loads((Path(__file__).resolve().parents[2]/'engine-rust/tests/fixtures/profile-export.json').read_text(encoding='utf-8'))
    for mode in ('current', 'missing', 'locked', 'unlocked', 'diverged'):
        stale = Profile(id='a', name='stale', fingerprint=generate(seed=1, os='windows'), created_at=1000.0, updated_at=1000.0)
        current = Profile(id='a', name='current', fingerprint=generate(seed=42, os='windows'), created_at=1000.0, updated_at=1000.0).to_dict()
        config = {}
        password = None
        if mode in ('locked', 'unlocked'):
            fixture = export['cases'][2]
            current['proxy'] = fixture['profile']['proxy']
            config = fixture['config']
            password = fixture['password'] if mode == 'unlocked' else None
        files = {} if mode == 'missing' else {'profiles/a.json': current}
        if mode == 'diverged':
            current['proxy'] = {'server':'http://127.0.0.1:8080','username':None,'password':'fixture-engine','country':None}
            from numera.convert import to_dashboard_profile
            dashboard = to_dashboard_profile(Profile.from_dict(current))
            dashboard['id'] = 'a'
            dashboard['proxy']['pass'] = 'fixture-dashboard'
            files['dashboard/a.json'] = dashboard
        if config:
            files['config.json'] = config
        snapshot = json.loads(json.dumps(files))
        with TemporaryDirectory(dir=parent) as tmp, ExitStack() as stack:
            root = Path(tmp)
            for name, value in files.items():
                path = root/name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(json.dumps(value), encoding='utf-8')
            stack.enter_context(patch.object(drivers.proctree, 'own_tree', lambda: object()))
            stack.enter_context(patch.object(drivers.sessions, 'take', lambda *_: {'session':'fixture'}))
            stack.enter_context(patch.object(drivers.sessions, 'release', lambda *_, **__: True))
            stack.enter_context(patch('numera.handshake.announce', lambda *_: None))
            stack.enter_context(patch('numera.handshake.reported', lambda: True))
            stack.enter_context(patch('time.time', lambda: 1000.0))
            store = ProfileStore(root, password=password)
            actual = error = kind = None
            try:
                with drivers.open_session(stale, store) as profile:
                    actual = profile.to_dict()
            except Exception as failure:
                error = str(failure)
                kind = drivers._startup_kind(failure)
            result.append({'name':mode,'stale':stale.to_dict(),'files':snapshot,'password':password,
                           'expected':actual,'error':error,'kind':kind})
    return result
