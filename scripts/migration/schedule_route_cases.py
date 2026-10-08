"""Original schedule HTTP contracts; no scheduler or browser starts."""
from __future__ import annotations

import json
import os
import tempfile
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch
from fastapi.testclient import TestClient
from numera.server import create_app


def cases(parent: Path) -> list[dict]:
    parent.mkdir(parents=True, exist_ok=True)
    variants = [
        ('list', 'GET', '/api/schedules', None),
        ('update', 'PUT', '/api/schedules/fixture-schedule', {'minutes': '7', 'enabled': False}),
        ('update-empty', 'PUT', '/api/schedules/fixture-schedule', {}),
        ('update-owned', 'PUT', '/api/schedules/fixture-schedule', {'lastRun': 7}),
        ('update-invalid-missing', 'PUT', '/api/schedules/missing', {'everyHours': -1}),
        ('update-missing', 'PUT', '/api/schedules/missing', {'minutes': 7}),
        ('delete', 'DELETE', '/api/schedules/fixture-schedule', None),
        ('delete-missing', 'DELETE', '/api/schedules/missing', None),
        ('add', 'POST', '/api/schedules', {'profileId': 'fixture-profile'}),
        ('add-null-preset', 'POST', '/api/schedules', {'profileId': 'fixture-profile', 'preset': None}),
        ('add-invalid', 'POST', '/api/schedules', {'profileId': 'fixture-profile', 'minutes': -1}),
        ('add-missing-profile', 'POST', '/api/schedules', {'profileId': 'missing', 'minutes': -1}),
        ('add-no-profile', 'POST', '/api/schedules', {}),
        ('list-invalid-number', 'GET', '/api/schedules', None),
        ('add-number-profile', 'POST', '/api/schedules', {'profileId': 7}),
        ('add-object-profile', 'POST', '/api/schedules', {'profileId': {'id': 'fixture-profile'}}),
        ('body-list', 'POST', '/api/schedules', []),
    ]
    found = []
    for name, method, path, body in variants:
        with tempfile.TemporaryDirectory(prefix='schedule-route-oracle-', dir=parent) as directory:
            root = Path(directory)
            with patch.dict(os.environ, {'NUMERA_API_TOKEN': 'fixture-api', 'NUMERA_ALLOWED_ORIGINS': ''}):
                app = create_app(data_dir=directory, start_scheduler=False)
                profiles = {'fixture-profile.json': {'id': 'fixture-profile', 'name': 'Fixture profile'}}
                rows = [{'id': 'fixture-schedule', 'profileId': 'fixture-profile', 'everyHours': 1.0,
                         'minutes': 5.0, 'preset': 'general', 'enabled': True,
                         'createdAt': 500.0, 'lastRun': None, 'future': 7},
                        {'id': 'missing-profile', 'profileId': 'gone', 'everyHours': 1.0,
                         'minutes': 5.0, 'preset': 'general', 'enabled': False,
                         'createdAt': 1500.0, 'lastRun': None}]
                if name == 'list-invalid-number': rows[0]['everyHours'] = 'invalid'
                (root / 'dashboard').mkdir(exist_ok=True)
                for filename, profile in profiles.items():
                    (root / 'dashboard' / filename).write_text(json.dumps(profile), encoding='utf-8')
                disk = root / 'schedules.json'
                disk.write_text(json.dumps(rows), encoding='utf-8')
                before = disk.read_bytes()
                with patch('numera.server.routes_schedules.time.time', return_value=1000.0), \
                     patch('numera.scheduler.uuid.uuid4', return_value=SimpleNamespace(hex='abcdef1234567890')), \
                     TestClient(app, raise_server_exceptions=False) as client:
                    response = client.request(method, path, headers={'authorization': 'Bearer fixture-api'}, json=body)
            found.append({'name': name, 'method': method, 'path': path, 'input': body, 'profiles': profiles,
                          'rows': rows, 'now': 1000.0, 'status': response.status_code,
                          'body': response.json() if response.headers.get('content-type', '').startswith('application/json') else response.text,
                          'written': json.loads(disk.read_text(encoding='utf-8')),
                          'unchanged': before == disk.read_bytes()})
    return found
