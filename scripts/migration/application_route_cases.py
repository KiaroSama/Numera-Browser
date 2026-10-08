"""Original cross-route authentication, CORS and method contracts."""
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
        ('ready', 'GET', '/api/ready', {}),
        ('health-auth', 'GET', '/api/health', {'authorization': 'Bearer fixture-api'}),
        ('health-denied', 'GET', '/api/health', {}),
        ('config-read', 'GET', '/api/config', {'authorization': 'Bearer fixture-api'}),
        ('extensions-read', 'GET', '/api/extensions/installed', {'authorization': 'Bearer fixture-api'}),
        ('proxies-read', 'GET', '/api/proxies', {'authorization': 'Bearer fixture-api'}),
        ('tokens-head', 'HEAD', '/api/tokens', {'authorization': 'Bearer fixture-api'}),
        ('schedules-head', 'HEAD', '/api/schedules', {'authorization': 'Bearer fixture-api'}),
        ('folders-method', 'GET', '/api/folders', {'authorization': 'Bearer fixture-api'}),
        ('cors-once', 'GET', '/api/tokens', {'authorization': 'Bearer fixture-api', 'origin': 'https://fixture.test'}),
        ('cors-denied', 'GET', '/api/tokens', {'origin': 'https://fixture.test'}),
        ('preflight', 'OPTIONS', '/api/folders', {'origin': 'https://fixture.test', 'access-control-request-method': 'POST'}),
        ('legacy-adopt', 'GET', '/api/ready', {}),
        ('legacy-loss', 'GET', '/api/tokens', {}),
    ]
    found = []
    for name, method, path, headers in variants:
        with tempfile.TemporaryDirectory(prefix='application-route-oracle-', dir=parent) as directory:
            root = Path(directory)
            legacy = name.startswith('legacy-')
            table = [{'id': 'fixture', 'name': 'Fixture', 'hash': 'fixture-hash',
                      'created': 1000.0, 'lastUsed': None}] if legacy else []
            token_path = root / 'tokens.json'
            token_path.write_text(json.dumps(table), encoding='utf-8')
            environment = '' if legacy else 'fixture-api'
            with patch.dict(os.environ, {'NUMERA_API_TOKEN': environment,
                                         'NUMERA_ALLOWED_ORIGINS': 'https://fixture.test'}), \
                 patch('numera.server.stores.time.time', return_value=1000.0), \
                 patch('numera.server.spa.dashboard_dir', return_value=None), \
                 patch('numera.drivers.is_installed', return_value=False):
                app = create_app(data_dir=directory, start_scheduler=False)
                if name == 'legacy-loss':
                    token_path.unlink()
                with TestClient(app) as client:
                    response = client.request(method, path, headers=headers)
            response_body = (response.json() if response.headers.get('content-type', '').startswith('application/json') else response.text) if response.content else None
            if name == 'config-read':
                assert response_body['root'] == str(root)
                response_body['root'] = '<fixture-root>'
            policy_path = root / 'auth-policy.json'
            found.append({'name': name, 'method': method, 'path': path, 'headers': headers,
                          'environment': environment, 'table': table,
                          'remove_tokens': name == 'legacy-loss', 'status': response.status_code,
                          'body': response_body,
                          'cors': {key: value for key, value in response.headers.items()
                                   if key.startswith('access-control-') or key == 'vary'},
                          'policy': json.loads(policy_path.read_text(encoding='utf-8')) if policy_path.exists() else None})
    return found
