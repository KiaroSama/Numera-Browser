"""Source folder operations, including rename chains and late prepared saves."""
from __future__ import annotations

import json
import tempfile
from pathlib import Path
from numera import folders


def cases(parent: Path) -> list[dict]:
    variants = [('create', [' source '], {'folders': ['alpha', 'alpha'], 'future': 7}),
                ('create', [' '], {}),
                ('rename', ['source', 'target'], {'folders': ['source', 'alpha'], 'renamed': {'before': 'source'}, 'future': 7}),
                ('rename', ['', 'target'], {}),
                ('delete', ['source'], {'folders': ['source', 'alpha'], 'future': 7}),
                ('delete', ['source', 'other'], {'folders': ['source'], 'future': 7})]
    parent.mkdir(parents=True, exist_ok=True)
    result = []
    for operation, args, meta in variants:
        with tempfile.TemporaryDirectory(prefix='folder-oracle-', dir=parent) as directory:
            root = Path(directory)
            (root / 'dashboard').mkdir()
            (root / 'dashboard-config.json').write_text(json.dumps(meta), encoding='utf-8')
            profiles = {'a.json': {'id': 'a', 'folder': 'source', 'future': [7]},
                        'b.json': {'id': 'b', 'folder': 'alpha'}}
            for name, data in profiles.items():
                (root / 'dashboard' / name).write_text(json.dumps(data), encoding='utf-8')
            before = {name: (root / 'dashboard' / name).read_bytes() for name in profiles}
            listed = folders.listing(root)
            try:
                value, error = getattr(folders, operation)(root, *args), None
            except ValueError as exc:
                value, error = None, str(exc)
            result.append({'operation': operation, 'args': args, 'meta': meta, 'profiles': profiles,
                           'listed': listed, 'result': value, 'error': error,
                           'written': {name: json.loads((root / 'dashboard' / name).read_text(encoding='utf-8')) for name in profiles},
                           'metadata': json.loads((root / 'dashboard-config.json').read_text(encoding='utf-8')),
                           'resolved': {name: folders.resolve_name(root, name) for name in ('before', 'source', 'target', 'alpha')},
                           'unchanged': {name: before[name] == (root / 'dashboard' / name).read_bytes() for name in profiles}})
    return result
