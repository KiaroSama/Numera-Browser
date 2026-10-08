"""Original stored revision coercion, increment and refusal without data writes."""
from __future__ import annotations

import json
import tempfile
from pathlib import Path
from unittest.mock import patch
from numera.fingerprint import generate
from numera.models import Profile
from numera.server.convert import to_dashboard_profile
from numera.server.stores import DashboardStore
from numera.store import ProfileStore


def cases(parent: Path) -> list[dict]:
    base = to_dashboard_profile(Profile(id='fixture-revision', name='Revision α',
        fingerprint=generate(seed=42, os='windows'), created_at=1700000000.0, updated_at=1700000000.0))
    base.update(id='fixture-revision', _created=1700000000.0)
    values = [None, 0, True, False, 3.9, -3.9, '  +004 ', '1_000', '١٢', '',
              2**100, -(2**100), -1, 'bad', '1__2', [1], {}, [], '3.0']
    parent.mkdir(parents=True, exist_ok=True)
    found = []
    original_profile = Profile
    def converted_profile(**values):
        values.setdefault('created_at', 1700000000.0)
        values.setdefault('updated_at', 1700000000.0)
        return original_profile(**values)
    with patch('numera.convert.Profile', side_effect=converted_profile), patch('numera.server.stores.time.time', return_value=1700000000.0):
        for index, revision in enumerate(values):
            with tempfile.TemporaryDirectory(prefix='dashboard-revision-oracle-', dir=parent) as directory:
                root = Path(directory)
                dash = DashboardStore(root, ProfileStore(root))
                stored = {**base, '_rev': revision}
                path = root / 'dashboard' / 'fixture-revision.json'
                path.write_text(json.dumps(stored), encoding='utf-8')
                before = path.read_bytes()
                incoming = {**base, 'notes': 'changed'}
                request = json.loads(json.dumps(incoming))
                try:
                    result, error = dash.save(incoming), None
                except (RuntimeError, ValueError, TypeError) as exc:
                    result, error = None, str(exc)
                found.append({'name': f'revision-{index}', 'initial': stored, 'input': request,
                              'result': result, 'error': error,
                              'written': json.loads(path.read_text(encoding='utf-8')),
                              'unchanged': path.read_bytes() == before,
                              'engine_exists': (root / 'profiles' / 'fixture-revision.json').exists()})
    return found
