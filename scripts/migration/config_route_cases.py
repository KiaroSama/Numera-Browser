"""Source settings HTTP contracts with explicit no-installed-backend discovery."""
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
        ('read', 'GET', None), ('empty', 'PUT', {}),
        ('engines-none', 'GET', None),
        ('engines-mixed', 'GET', None),
        ('write', 'PUT', {'default_engine': 'camoufox', 'headless_launch': True, 'default_extensions': []}),
        ('nulls', 'PUT', {'default_engine': None, 'headless_launch': None, 'default_extensions': None}),
        ('unknown', 'PUT', {'version': 'x', 'root': 'x'}),
        ('invalid-engine', 'PUT', {'default_engine': 'unknown'}),
        ('invalid-headless', 'PUT', {'default_engine': 'cloak', 'headless_launch': 'false'}),
        ('invalid-extensions', 'PUT', {'default_extensions': 7}),
        ('extension-url', 'PUT', {'default_extensions': ['https://fixture.test/extension']}),
        ('blank-extensions', 'PUT', {'default_extensions': ' \n\t'}),
        ('extension-list', 'GET', None),
        ('extension-text', 'GET', None),
        ('extension-numeric', 'GET', None),
        ('extension-object', 'GET', None),
        ('metadata-null', 'GET', None),
        ('metadata-folder-text', 'GET', None),
        ('metadata-folder-null', 'GET', None),
        ('metadata-folder-unhashable', 'GET', None),
        ('metadata-folder-object', 'GET', None),
        ('metadata-folder-numeric-bool', 'GET', None),
        ('metadata-folder-large-numbers', 'GET', None),
    ]
    found = []
    for name, method, body in variants:
        with tempfile.TemporaryDirectory(prefix='config-route-oracle-', dir=parent) as directory:
            root = Path(directory)
            installed = {engine: name == 'engines-mixed' and engine in ('camoufox', 'playwright')
                         for engine in ('camoufox', 'cloak', 'patchright', 'playwright')}
            with patch.dict(os.environ, {'NUMERA_API_TOKEN': 'fixture-api', 'NUMERA_ALLOWED_ORIGINS': ''}), \
                 patch('numera.drivers.is_installed', side_effect=lambda engine: installed[engine]):
                app = create_app(data_dir=directory, start_scheduler=False)
                config = {'future': 7, 'headless_launch': False}
                if name == 'extension-list': config['default_extensions'] = ['legacy-path']
                elif name == 'extension-text': config['default_extensions'] = 'ab'
                elif name == 'extension-numeric': config['default_extensions'] = 7
                elif name == 'extension-object': config['default_extensions'] = {'legacy-a': 1, 'legacy-b': 2}
                meta = {'folders': ['created', 'created'], 'future': 7}
                if name == 'metadata-null': meta = None
                elif name == 'metadata-folder-text': meta['folders'] = 'ab'
                elif name == 'metadata-folder-null': meta['folders'] = None
                elif name == 'metadata-folder-unhashable': meta['folders'] = [{}]
                elif name == 'metadata-folder-object': meta['folders'] = {'a': 7, 'b': 9}
                elif name == 'metadata-folder-numeric-bool': meta['folders'] = [1, True, 1.0, 0, False, None, '1']
                elif name == 'metadata-folder-large-numbers': meta['folders'] = [2**53, 2**53+1, float(2**53), 2**100, 2**100+1]
                profiles = {'b.json': {'id': 'b', 'folder': 'zeta'},
                            'a.json': {'id': 'a', 'folder': 'alpha'}}
                proxies = [{'id': 'p', 'server': 'http://127.0.0.1:8080', 'pass': ''}]
                (root / 'dashboard').mkdir(exist_ok=True)
                for filename, profile in profiles.items():
                    (root / 'dashboard' / filename).write_text(json.dumps(profile), encoding='utf-8')
                (root / 'dashboard-config.json').write_text(json.dumps(meta), encoding='utf-8')
                (root / 'proxies.json').write_text(json.dumps(proxies), encoding='utf-8')
                disk = root / 'config.json'
                disk.write_text(json.dumps(config), encoding='utf-8')
                before = disk.read_bytes()
                with TestClient(app, raise_server_exceptions=False) as client:
                    response = client.request(method, '/api/engines' if name.startswith('engines-') else '/api/config', headers={'authorization': 'Bearer fixture-api'}, json=body)
            response_body = response.json() if response.headers.get('content-type', '').startswith('application/json') else response.text
            if response.status_code == 200 and not name.startswith('engines-'):
                assert response_body['root'] == str(root)
                response_body['root'] = '<fixture-root>'
            found.append({'name': name, 'method': method, 'input': body, 'config': config, 'installed': installed,
                          'path': '/api/engines' if name.startswith('engines-') else '/api/config',
                          'metadata': meta, 'profiles': profiles, 'proxies': proxies,
                          'status': response.status_code, 'body': response_body,
                          'written': json.loads(disk.read_text(encoding='utf-8')),
                          'unchanged': before == disk.read_bytes()})
    return found
