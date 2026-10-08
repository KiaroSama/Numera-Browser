"""Original installed-extension HTTP list; no provider download/browser."""
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
    found = []
    for name, defaults in [('empty', []), ('manual', ['manual']), ('duplicate', ['store', 'manual', 'manual']),
                           ('missing-url', ['missing', 'https://fixture.test/ext'])]:
        with tempfile.TemporaryDirectory(prefix='extension-route-oracle-', dir=parent) as directory:
            root = Path(directory)
            with patch.dict(os.environ, {'NUMERA_API_TOKEN': 'fixture-api', 'NUMERA_ALLOWED_ORIGINS': ''}):
                app = create_app(data_dir=directory, start_scheduler=False)
                files = {} if name == 'empty' else {
                    'extensions/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/manifest.json': json.dumps({'name': 'Store α', 'icons': {'48': 'icon.png'}}),
                    'extensions/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/icon.png': 'fixture-icon',
                    'manual/manifest.json': json.dumps({'name': 'Manual α'}),
                }
                for relative, value in files.items():
                    path = root / relative
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_text(value, encoding='utf-8')
                relative_defaults = ['extensions/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa' if item == 'store' else item for item in defaults]
                paths = [item if '://' in item else str(root / item) for item in relative_defaults]
                (root / 'config.json').write_text(json.dumps({'default_extensions': paths}), encoding='utf-8')
                with TestClient(app) as client:
                    response = client.get('/api/extensions/installed', headers={'authorization': 'Bearer fixture-api'})
            body = response.json()
            for entry in body.get('extensions', []):
                entry['path'] = Path(entry['path']).relative_to(root).as_posix()
                if entry['icon']: entry['icon'] = Path(entry['icon']).relative_to(root).as_posix()
            found.append({'name': name, 'files': files, 'defaults': relative_defaults,
                          'status': response.status_code, 'body': body})
    return found
