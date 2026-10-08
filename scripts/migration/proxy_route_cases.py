"""Original proxy HTTP persistence contracts; no live proxy operations."""
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
    # Reuse only recorded synthetic INPUT vectors, never expected results.
    vault = json.loads((Path(__file__).resolve().parents[2] / 'engine-rust/tests/fixtures/pool-vault.json').read_text(encoding='utf-8'))['cases'][0]
    variants = [
        ('list', 'GET', '/api/proxies', None),
        ('edit-label', 'PUT', '/api/proxies/fixture-proxy', {'label': 'Updated'}),
        ('edit-host', 'PUT', '/api/proxies/fixture-proxy', {'host': ' new.fixture.test ', 'port': 9090}),
        ('edit-clear-password', 'PUT', '/api/proxies/fixture-proxy', {'pass': ''}),
        ('edit-preserve-password', 'PUT', '/api/proxies/fixture-proxy', {'pass': None}),
        ('edit-invalid-password', 'PUT', '/api/proxies/fixture-proxy', {'pass': 7}),
        ('edit-invalid-port', 'PUT', '/api/proxies/fixture-proxy', {'port': 70000}),
        ('edit-missing', 'PUT', '/api/proxies/missing', {'port': 70000}),
        ('edit-unknown', 'PUT', '/api/proxies/fixture-proxy', {'future': 8}),
        ('delete', 'POST', '/api/proxies/delete', {'ids': ['missing', 'fixture-proxy', 'fixture-proxy']}),
        ('delete-empty', 'POST', '/api/proxies/delete', {'ids': []}),
        ('delete-text', 'POST', '/api/proxies/delete', {'ids': 'fixture-proxy'}),
        ('export-all', 'POST', '/api/proxies/export', {}),
        ('export-one', 'POST', '/api/proxies/export', {'ids': ['fixture-proxy']}),
        ('export-empty', 'POST', '/api/proxies/export', {'ids': ['missing']}),
        ('export-text-selection', 'POST', '/api/proxies/export', {'ids': 'fixture-proxy'}),
        ('export-locked', 'POST', '/api/proxies/export', {}),
        ('export-locked-safe-selection', 'POST', '/api/proxies/export', {'ids': ['other-proxy']}),
        ('edit-locked-clear', 'PUT', '/api/proxies/fixture-proxy', {'pass': ''}),
        ('edit-locked-preserve', 'PUT', '/api/proxies/fixture-proxy', {'pass': None}),
        ('delete-single', 'DELETE', '/api/proxies/fixture-proxy', None),
        ('delete-single-missing', 'DELETE', '/api/proxies/missing', None),
        ('add-duplicate', 'POST', '/api/proxies', {'proxies': [{'type':'HTTP','host':'fixture.test','port':'8080','user':'fixture-user','pass':'fixture-secret','country':'US','label':'Fixture','rotateUrl':None}]}),
        ('add-empty', 'POST', '/api/proxies', {'text': ''}),
        ('add-invalid', 'POST', '/api/proxies', {'proxies': [{'host':'fixture.test','port':70000}]}),
        ('add-new', 'POST', '/api/proxies', {'text':'http://new.fixture.test:9090'}),
    ]
    found = []
    for name, method, path, body in variants:
        with tempfile.TemporaryDirectory(prefix='proxy-route-oracle-', dir=parent) as directory:
            root = Path(directory)
            with patch.dict(os.environ, {'NUMERA_API_TOKEN': 'fixture-api', 'NUMERA_ALLOWED_ORIGINS': ''}):
                app = create_app(data_dir=directory, start_scheduler=False)
                rows = [{'id': 'fixture-proxy', 'type': 'HTTP', 'host': 'fixture.test', 'port': '8080',
                         'user': 'fixture-user', 'pass': 'fixture-secret', 'country': 'US',
                         'label': 'Fixture', 'rotateUrl': None, 'status': 'ok', 'lastCheck': {'ok': True},
                         'checkedAt': 1000.0, 'future': 7},
                        {'id': 'other-proxy', 'type': 'HTTP', 'host': 'fixture.test', 'port': '8080',
                         'user': 'other-user', 'pass': None, 'country': None, 'label': None}]
                config = vault['config'] if 'locked' in name else {}
                if config: rows[0]['pass'] = vault['initial'][0]['pass']
                (root / 'config.json').write_text(json.dumps(config), encoding='utf-8')
                profiles = {'a.json': {'id': 'a', 'proxy': {key: rows[0][key] for key in ('type','host','port','user','pass')}},
                            'b.json': {'id': 'b', 'proxy': {key: rows[1][key] for key in ('type','host','port','user','pass')}}}
                (root / 'dashboard').mkdir(exist_ok=True)
                for filename, profile in profiles.items():
                    (root / 'dashboard' / filename).write_text(json.dumps(profile), encoding='utf-8')
                disk = root / 'proxies.json'
                disk.write_text(json.dumps(rows), encoding='utf-8')
                before = disk.read_bytes()
                with patch('numera.server.stores.uuid.uuid4', return_value=SimpleNamespace(hex='abcdef1234567890')), \
                     TestClient(app, raise_server_exceptions=False) as client:
                    response = client.request(method, path, headers={'authorization': 'Bearer fixture-api'}, json=body)
            found.append({'name': name, 'method': method, 'path': path, 'input': body, 'profiles': profiles,
                          'rows': rows, 'config': config, 'status': response.status_code,
                          'body': response.json() if response.headers.get('content-type', '').startswith('application/json') else response.text,
                          'written': json.loads(disk.read_text(encoding='utf-8')),
                          'unchanged': before == disk.read_bytes()})
    return found
