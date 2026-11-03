"""Observe original stop HTTP refusals without any browser or user data."""
import json
import os
import tempfile
from pathlib import Path
from unittest.mock import patch

from fastapi.testclient import TestClient
from baseline_source import activate
activate()

from numera.server import create_app


def cases(parent):
    parent = Path(parent)
    parent.mkdir(parents=True, exist_ok=True)
    found = []
    for name, exists, claim, authorized in [
        ('stopped', True, None, True), ('missing', False, None, True),
        ('foreign-unreadable', True, '{', True), ('unauthorized', True, None, False),
    ]:
        with tempfile.TemporaryDirectory(prefix='stop-source-', dir=parent) as directory:
            root = Path(directory)
            with patch.dict(os.environ, {'NUMERA_API_TOKEN':'fixture-api', 'NUMERA_ALLOWED_ORIGINS':''}):
                app = create_app(data_dir=directory, start_scheduler=False)
                record = {'id':'a', 'name':'Fixture', 'custom':'preserved'}
                if exists:
                    (root/'dashboard').mkdir(exist_ok=True)
                    (root/'dashboard/a.json').write_text(json.dumps(record), encoding='utf-8')
                if claim is not None:
                    (root/'sessions').mkdir(exist_ok=True)
                    (root/'sessions/a.json').write_text(claim, encoding='utf-8')
                with TestClient(app, raise_server_exceptions=False) as client:
                    response = client.post('/api/profiles/a/stop', headers={'authorization':'Bearer fixture-api'} if authorized else {})
            found.append({'name':name, 'record':record if exists else None, 'claim':claim,
                          'authorized':authorized, 'status':response.status_code,
                          'body':response.json() if response.headers.get('content-type','').startswith('application/json') else response.text})
    return found
