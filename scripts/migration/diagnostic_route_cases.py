"""Original readiness/health HTTP wires; no app browser work or listener."""
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
    variants = [('ready-open', 'GET', '/api/ready', '', {}, None),
                ('ready-protected', 'GET', '/api/ready', 'fixture-api', {}, None),
                ('ready-sticky', 'GET', '/api/ready', '', {}, {'protected': True}),
                ('health-open', 'GET', '/api/health', '', {}, None),
                ('health-denied', 'GET', '/api/health', 'fixture-api', {}, None),
                ('health-auth', 'GET', '/api/health', 'fixture-api', {'authorization': 'Bearer fixture-api'}, None),
                ('health-sticky-denied', 'GET', '/api/health', '', {}, {'protected': True}),
                ('ready-post', 'POST', '/api/ready', '', {}, None),
                ('ready-head', 'HEAD', '/api/ready', '', {}, None),
                ('ready-options', 'OPTIONS', '/api/ready', '', {}, None)]
    found = []
    for name, method, path, environment, headers, policy in variants:
        with tempfile.TemporaryDirectory(prefix='diagnostic-route-oracle-', dir=parent) as directory:
            root = Path(directory)
            if policy is not None:
                (root / 'auth-policy.json').write_text(json.dumps(policy), encoding='utf-8')
            with patch.dict(os.environ, {'NUMERA_API_TOKEN': environment, 'NUMERA_ALLOWED_ORIGINS': ''}):
                app = create_app(data_dir=directory, start_scheduler=False)
                with TestClient(app) as client:
                    response = client.request(method, path, headers=headers)
            found.append({'name': name, 'method': method, 'path': path, 'environment': environment,
                          'headers': headers, 'policy': policy, 'status': response.status_code,
                          'body': response.json() if response.content else None, 'content_type': response.headers.get('content-type')})
    return found
