"""Original dashboard metadata reads/replacements and delegated profile deletion."""
from __future__ import annotations

import json
import tempfile
from pathlib import Path
from numera.server.stores import DashboardStore
from numera.store import ProfileStore


def cases(parent: Path) -> list[dict]:
    parent.mkdir(parents=True, exist_ok=True)
    found = []
    for name, raw in [('missing', None), ('broken', '{'),
                      ('legacy-null', 'null'), ('legacy-list', '[1,"α"]'),
                      ('object', '{"folders":["Work"],"future":7}')]:
        with tempfile.TemporaryDirectory(prefix='dashboard-meta-oracle-', dir=parent) as directory:
            root = Path(directory)
            dash = DashboardStore(root, ProfileStore(root))
            path = root / 'dashboard-config.json'
            if raw is not None:
                path.write_text(raw, encoding='utf-8')
            before = path.read_bytes() if path.exists() else None
            read = dash.get_meta()
            unchanged = (path.read_bytes() if path.exists() else None) == before
            request = {'folders': ['Current α'], 'future': {'retained': True}}
            result = dash.set_meta(request)
            found.append({'name': name, 'raw': raw, 'read': read,
                          'read_unchanged': unchanged, 'input': request,
                          'result': result, 'written': dash.get_meta(),
                          'profile_listing': dash.list()})
    for name, present, damaged in [('delete', True, False), ('missing-delete', False, False),
                                    ('uncertain-delete', True, True)]:
        with tempfile.TemporaryDirectory(prefix='dashboard-delete-oracle-', dir=parent) as directory:
            root = Path(directory)
            dash = DashboardStore(root, ProfileStore(root))
            if present:
                for kind in ('profiles', 'dashboard'):
                    (root / kind / 'fixture-delete.json').write_text(
                        json.dumps({'id': 'fixture-delete', 'name': 'Delete α'}), encoding='utf-8')
                (root / 'user-data' / 'fixture-delete').mkdir(parents=True)
                (root / 'user-data' / 'fixture-delete' / 'Cookies').write_bytes(b'fixture-cookies')
            if damaged:
                (root / 'sessions').mkdir(parents=True)
                (root / 'sessions' / 'fixture-delete.json').write_text('{', encoding='utf-8')
            try:
                result, error = dash.delete('fixture-delete'), None
            except (RuntimeError, ValueError) as exc:
                result, error = None, str(exc)
            found.append({'name': name, 'present': present, 'damaged_claim': damaged,
                          'result': result, 'error': error,
                          'engine_exists': (root / 'profiles' / 'fixture-delete.json').exists(),
                          'dashboard_exists': (root / 'dashboard' / 'fixture-delete.json').exists(),
                          'session_exists': (root / 'user-data' / 'fixture-delete').exists()})
    return found
