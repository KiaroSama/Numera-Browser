"""Retained token list/revoke routes, never prints or generates a real token."""
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
    from numera.server.stores import TokenStore
    row = {'id': 'fixture-token', 'name': 'Fixture', 'hash': TokenStore.hash('fixture-named-api'),
           'created': 1000.0, 'lastUsed': None, 'future': 7}
    wire_bodies = {
        'create-empty-json': ('', 'application/json'),
        'create-empty-text': ('', 'text/plain'),
        'create-no-content-type': ('{}', None),
        'create-text-object': ('{}', 'text/plain'),
        'create-vendor-json': ('{}', 'application/vnd.fixture+json'),
        'create-json-charset': ('{}', 'application/json; charset=utf-8'),
        'create-json-case': ('{}', 'APPLICATION/JSON'),
        'create-json-space': (' \n\t', 'application/json'),
        'create-json-open-object': ('{', 'application/json'),
        'create-json-unquoted-key': ('{bad}', 'application/json'),
        'create-json-missing-value': ('{"name":}', 'application/json'),
        'create-json-missing-colon': ('{"name" ""}', 'application/json'),
        'create-json-trailing-comma': ('{"name":"",}', 'application/json'),
        'create-json-trailing-data': ('{} {}', 'application/json'),
        'create-json-open-string': ('"name', 'application/json'),
        'create-json-invalid-escape': ('"bad\\q"', 'application/json'),
        'create-json-control': ('"bad\ntext"', 'application/json'),
        'create-json-unicode-offset': ('{"é":1,}', 'application/json'),
        'create-json-array-comma': ('[1,]', 'application/json'),
        'create-json-array-eof': ('[1', 'application/json'),
        'create-json-object-eof': ('{"name":""', 'application/json'),
        'create-json-colon-eof': ('{"name"', 'application/json'),
        'create-json-value-eof': ('{"name":', 'application/json'),
        'create-json-invalid-literal': ('nulX', 'application/json'),
        'create-json-short-literal': ('tru', 'application/json'),
        'create-json-leading-zero': ('01', 'application/json'),
        'create-json-short-exponent': ('1e', 'application/json'),
        'create-json-short-hex': ('"\\u12"', 'application/json'),
        'create-json-invalid-hex': ('"\\u12xz"', 'application/json'),
    }
    encoded_bodies = {
        **{f'create-encoded-{encoding}': ('{}'.encode(encoding), encoding)
           for encoding in ('utf-8-sig', 'utf-16', 'utf-16-le', 'utf-16-be', 'utf-32', 'utf-32-le', 'utf-32-be')},
        'create-encoded-unicode-error': ('{"é":1,}'.encode('utf-16-be'), 'utf-16-be'),
        'create-encoded-invalid-utf8': (b'\xff', 'invalid-utf8'),
        'create-encoded-truncated-utf16': (b'\xff\xfe\x7b', 'truncated-utf16'),
        'create-encoded-truncated-utf32': (b'\xff\xfe\x00\x00\x7b', 'truncated-utf32'),
    }
    found = []
    for name, method, path, table, environment in [
        ('list-empty', 'GET', '/api/tokens', [], ''),
        ('list-filter', 'GET', '/api/tokens', [row], 'fixture-api'),
        ('revoke-missing', 'DELETE', '/api/tokens/missing', [row], 'fixture-api'),
        ('revoke-last-env', 'DELETE', '/api/tokens/fixture-token', [row], 'fixture-api'),
        ('revoke-damaged', 'DELETE', '/api/tokens/fixture-token', '{', 'fixture-api'),
        ('list-damaged', 'GET', '/api/tokens', '{', 'fixture-api'),
        ('create-empty', 'POST', '/api/tokens', [], 'fixture-api'),
        ('create-duplicate', 'POST', '/api/tokens', [row], 'fixture-api'),
        ('revoke-last-named', 'DELETE', '/api/tokens/fixture-token', [row], ''),
        ('revoke-named-missing', 'DELETE', '/api/tokens/missing', [row], ''),
        ('create-list', 'POST', '/api/tokens', [], 'fixture-api'),
        ('create-null', 'POST', '/api/tokens', [], 'fixture-api'),
        ('create-string', 'POST', '/api/tokens', [], 'fixture-api'),
        ('create-number', 'POST', '/api/tokens', [], 'fixture-api'),
        *((name, 'POST', '/api/tokens', [], 'fixture-api') for name in (*wire_bodies, *encoded_bodies)),
    ]:
        with tempfile.TemporaryDirectory(prefix='token-route-oracle-', dir=parent) as directory:
            root = Path(directory)
            path_on_disk = root / 'tokens.json'
            path_on_disk.write_text(table if isinstance(table, str) else json.dumps(table), encoding='utf-8')
            if name in ('revoke-last-named','revoke-named-missing'):
                table = [{**row, 'lastUsed': 9999999999.0}]
                path_on_disk.write_text(json.dumps(table), encoding='utf-8')
            before = path_on_disk.read_bytes()
            headers = {'authorization': 'Bearer '+environment} if environment else {'authorization':'Bearer fixture-named-api'} if name in ('revoke-last-named','revoke-named-missing') else {}
            with patch.dict(os.environ, {'NUMERA_API_TOKEN': environment, 'NUMERA_ALLOWED_ORIGINS': ''}):
                app = create_app(data_dir=directory, start_scheduler=False)
                with patch('numera.server.stores.time.time', return_value=1000.0), TestClient(app) as client:
                    request_body = {'create-list': [], 'create-null': None, 'create-string': 'invalid', 'create-number': 7}.get(name, {'name':'Fixture' if name == 'create-duplicate' else ''}) if method == 'POST' else None
                    if name in encoded_bodies:
                        headers['content-type'] = 'application/json'
                        response = client.request(method, path, headers=headers, content=encoded_bodies[name][0])
                    elif name in wire_bodies:
                        wire_body, media_type = wire_bodies[name]
                        if media_type is not None:
                            headers['content-type'] = media_type
                        response = client.request(method, path, headers=headers, content=wire_body.encode('utf-8'))
                    else:
                        response = client.request(method, path, headers=headers, json=request_body)
            raw = path_on_disk.read_text(encoding='utf-8')
            try:
                written = json.loads(raw)
            except ValueError:
                written = raw
            found.append({'name': name, 'method': method, 'path': path, 'table': table,
                          'environment': environment, 'headers': headers,
                          'input': list(encoded_bodies[name][0]) if name in encoded_bodies else wire_bodies[name][0] if name in wire_bodies else request_body,
                          'status': response.status_code,
                          'body': response.json(), 'written': written,
                          'unchanged': before == path_on_disk.read_bytes(),
                          **({'wire_body': wire_bodies[name][0]} if name in wire_bodies else {}),
                          **({'wire_bytes': list(encoded_bodies[name][0]), 'wire_encoding': encoded_bodies[name][1]} if name in encoded_bodies else {})})
    return found
