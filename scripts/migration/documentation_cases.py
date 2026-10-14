"""Observe retained FastAPI documentation routes without SDK or user-store access."""
from __future__ import annotations
import argparse
import hashlib
import json
import os
import sys
import tempfile
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'engine'))
from run_logging import run_log

PATHS = {'openapi.json': '/openapi.json', 'swagger.html': '/docs',
         'oauth2-redirect.html': '/docs/oauth2-redirect', 'redoc.html': '/redoc'}


def cases(parent):
    from fastapi.testclient import TestClient
    from numera.server import create_app

    with tempfile.TemporaryDirectory(prefix='documentation-oracle-', dir=parent) as directory:
        with patch.dict(os.environ, {'NUMERA_API_TOKEN': 'documentation-fixture-only'}), \
             patch('numera.server.spa.dashboard_dir', return_value=None), \
             patch('numera.drivers.is_installed', return_value=False):
            app = create_app(data_dir=directory, start_scheduler=False)
            result = []
            with TestClient(app) as client:
                for name, path in PATHS.items():
                    for method in ('GET', 'HEAD', 'POST'):
                        response = client.request(method, path)
                        result.append({'name': name, 'path': path, 'method': method,
                                       'status': response.status_code,
                                       'content_type': response.headers.get('content-type'),
                                       'allow': response.headers.get('allow'),
                                       'content_length': response.headers.get('content-length'),
                                       'body': response.text})
            return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if not args.output.resolve().is_relative_to(ROOT.resolve()):
        raise ValueError('Documentation assets must stay in this project')
    from importlib.metadata import version
    if version('fastapi') != '0.142.3':
        raise RuntimeError('Documentation oracle requires FastAPI 0.142.3')
    temporary = ROOT / '.ci-work/migration-temp'
    temporary.mkdir(parents=True, exist_ok=True)
    observations = cases(temporary)
    args.output.mkdir(parents=True, exist_ok=True)
    hashes = {}
    for case in observations:
        if case['method'] != 'GET':
            continue
        assert case['status'] == 200, (case['path'], case['status'])
        path = args.output / case['name']
        path.write_text(case['body'], encoding='utf-8', newline='\n')
        hashes[case['name']] = hashlib.sha256(path.read_bytes()).hexdigest()
    (args.output / 'provenance.json').write_text(json.dumps({
        'fastapi': version('fastapi'), 'starlette': version('starlette'),
        'source': 'engine/numera/server', 'sha256': hashes}, indent=2) + '\n', encoding='utf-8')
    return len(observations)


if __name__ == '__main__':
    with run_log('documentation-oracle') as logger:
        logger.info('Generated documentation assets observations=%s', main())
