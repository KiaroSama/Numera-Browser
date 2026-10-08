"""Retained HTTP identity validation, no browser/network or writes."""
from __future__ import annotations

import os
import tempfile
from pathlib import Path
from unittest.mock import patch
from fastapi.testclient import TestClient
from numera.server import create_app


def cases(parent: Path) -> list[dict]:
    parent.mkdir(parents=True, exist_ok=True)
    found = []
    variants = [{}, {'timezone': 'unknown'}, {'screen': '9000x9000'},
                {'os': 'iOS', 'memory': 8}, {'webglRenderer': 'alien'},
                {'fontList': 'alien'}, {'maxTouchPoints': -1}, {'memory': 3.5},
                {'proxy': {'type': 'INVALID', 'host': 'fixture.test'}},
                {'cores': 'invalid'}, [], None]
    with tempfile.TemporaryDirectory(prefix='fingerprint-route-oracle-', dir=parent) as directory:
        with patch.dict(os.environ, {'NUMERA_API_TOKEN': 'fixture-api', 'NUMERA_ALLOWED_ORIGINS': ''}):
            app = create_app(data_dir=directory, start_scheduler=False)
            with TestClient(app) as client:
                for index, body in enumerate(variants):
                    response = client.post('/api/fingerprint/validate', headers={'authorization': 'Bearer fixture-api'}, json=body)
                    found.append({'name': f'fingerprint-{index}', 'input': body,
                                  'status': response.status_code, 'body': response.json()})
    return found
