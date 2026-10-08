"""Current retained API CORS status/body/headers; in-memory ASGI, no socket."""
from __future__ import annotations

import os
import tempfile
from pathlib import Path
from unittest.mock import patch
from fastapi.testclient import TestClient
from numera.server import create_app


def cases(parent: Path) -> list[dict]:
    parent.mkdir(parents=True, exist_ok=True)
    origin = 'https://fixture.example'
    variants = [('same-origin-default', '', 'GET', {}),
                ('default-origin', '', 'GET', {'origin': origin}),
                ('allowed-origin', origin, 'GET', {'origin': origin}),
                ('denied-origin', 'https://other.example', 'GET', {'origin': origin}),
                ('wildcard', '*', 'GET', {'origin': origin}),
                ('wildcard-cookie', '*', 'GET', {'origin': origin, 'cookie': 'fixture=value'}),
                ('preflight-allowed', origin, 'OPTIONS', {'origin': origin, 'access-control-request-method': 'PUT', 'access-control-request-headers': 'authorization, x-numera-token'}),
                ('preflight-denied', '', 'OPTIONS', {'origin': origin, 'access-control-request-method': 'GET'}),
                ('preflight-method', origin, 'OPTIONS', {'origin': origin, 'access-control-request-method': 'BOGUS'}),
                ('preflight-private', origin, 'OPTIONS', {'origin': origin, 'access-control-request-method': 'GET', 'access-control-request-private-network': 'true'}),
                ('preflight-wildcard', '*', 'OPTIONS', {'origin': origin, 'access-control-request-method': 'QUERY'}),
                ('options-ordinary', origin, 'OPTIONS', {'origin': origin}),
                ('denied-auth', origin, 'GET', {'origin': origin}),
                ('line-origins', ' https://other.example\n\n '+origin+' ', 'GET', {'origin': origin})]
    found = []
    for name, configured, method, headers in variants:
        with tempfile.TemporaryDirectory(prefix='cors-oracle-', dir=parent) as directory:
            token = 'fixture-token' if name == 'denied-auth' else ''
            with patch.dict(os.environ, {'NUMERA_ALLOWED_ORIGINS': configured, 'NUMERA_API_TOKEN': token}):
                app = create_app(data_dir=directory, start_scheduler=False)
                with TestClient(app) as client:
                    response = client.request(method, '/api/ready' if name != 'denied-auth' else '/api/config', headers=headers)
            observed = {key: value for key, value in response.headers.items()
                        if key.startswith('access-control-') or key in ('vary', 'content-type')}
            found.append({'name': name, 'origins': configured, 'method': method, 'headers': headers,
                          'token': token, 'path': '/api/ready' if name != 'denied-auth' else '/api/config',
                          'status': response.status_code, 'response_headers': observed,
                          'body': response.text if method == 'OPTIONS' or response.status_code == 401 else None})
    return found
