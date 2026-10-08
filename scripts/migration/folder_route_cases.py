"""Original folder HTTP transaction responses and persisted observations."""
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
        ('create', '/api/folders', {'name': ' source '}),
        ('create-missing', '/api/folders', {}),
        ('create-empty', '/api/folders', {'name': ' '}),
        ('create-null', '/api/folders', {'name': None}),
        ('create-false', '/api/folders', {'name': False}),
        ('rename', '/api/folders/rename', {'from': 'source', 'to': 'target'}),
        ('rename-missing', '/api/folders/rename', {'from': 'source'}),
        ('rename-same', '/api/folders/rename', {'from': 'source', 'to': 'source'}),
        ('delete', '/api/folders/delete', {'name': 'source'}),
        ('delete-reassign', '/api/folders/delete', {'name': 'source', 'reassignTo': 'other'}),
        ('delete-empty-target', '/api/folders/delete', {'name': 'source', 'reassignTo': ' '}),
        ('delete-missing', '/api/folders/delete', {}),
        ('delete-absent', '/api/folders/delete', {'name': 'absent'}),
        ('create-number', '/api/folders', {'name': 7}),
        ('rename-object', '/api/folders/rename', {'from': {}, 'to': 'target'}),
        ('delete-number-target', '/api/folders/delete', {'name': 'source', 'reassignTo': 7}),
        ('body-list', '/api/folders', []),
    ]
    found = []
    for name, path, body in variants:
        with tempfile.TemporaryDirectory(prefix='folder-route-oracle-', dir=parent) as directory:
            root = Path(directory)
            app_env = {'NUMERA_API_TOKEN': 'fixture-api', 'NUMERA_ALLOWED_ORIGINS': ''}
            with patch.dict(os.environ, app_env):
                app = create_app(data_dir=directory, start_scheduler=False)
                profiles = {'a.json': {'id': 'a', 'folder': 'source', 'future': [7]},
                            'b.json': {'id': 'b', 'folder': 'alpha'}}
                metadata = {'folders': ['source', 'alpha'], 'future': 7}
                (root / 'dashboard').mkdir(exist_ok=True)
                for filename, profile in profiles.items():
                    (root / 'dashboard' / filename).write_text(json.dumps(profile), encoding='utf-8')
                (root / 'dashboard-config.json').write_text(json.dumps(metadata), encoding='utf-8')
                before = {filename: (root / 'dashboard' / filename).read_bytes() for filename in profiles}
                with TestClient(app, raise_server_exceptions=False) as client:
                    response = client.post(path, headers={'authorization': 'Bearer fixture-api'}, json=body)
            found.append({'name': name, 'path': path, 'input': body, 'profiles': profiles,
                          'metadata': metadata, 'status': response.status_code,
                          'body': response.json() if response.headers.get('content-type', '').startswith('application/json') else response.text,
                          'written': {filename: json.loads((root / 'dashboard' / filename).read_text(encoding='utf-8')) for filename in profiles},
                          'written_meta': json.loads((root / 'dashboard-config.json').read_text(encoding='utf-8')),
                          'unchanged': {filename: before[filename] == (root / 'dashboard' / filename).read_bytes() for filename in profiles}})
    return found
