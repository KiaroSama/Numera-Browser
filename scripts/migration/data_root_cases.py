"""Data-root precedence through the original filesystem probe, isolated paths."""
from __future__ import annotations

import os
import tempfile
from pathlib import Path
from unittest.mock import patch
from numera import paths


def cases(parent: Path) -> list[dict]:
    parent.mkdir(parents=True, exist_ok=True)
    found = []
    variants = [('chosen', 'environment', False, False, False),
                (None, 'environment', False, False, False),
                (None, None, False, False, False),
                (None, None, True, False, False),
                (None, None, True, False, True),
                (None, None, True, True, True),
                ('~/chosen', 'environment', True, False, True)]
    for explicit, environment, blocked, current, legacy in variants:
        with tempfile.TemporaryDirectory(prefix='root-oracle-', dir=parent) as directory:
            root = Path(directory)
            app, home = root / 'app', root / 'home'
            app.mkdir(); home.mkdir()
            if blocked: (app / 'data').write_text('fixture obstruction', encoding='utf-8')
            if current: (home / '.numera').mkdir()
            if legacy: (home / '.persona').mkdir()
            chosen = home / 'chosen' if explicit == '~/chosen' else root / 'chosen' if explicit else None
            env_path = root / 'environment' if environment else None
            env = {'NUMERA_DATA_DIR': str(env_path) if env_path else ''}
            # No host home/CWD/user store consulted by the source probe.
            with patch.object(paths, 'app_dir', return_value=app), patch.object(Path, 'home', return_value=home), patch.dict(os.environ, env):
                result = paths.resolve_data_dir(chosen)
            found.append({'explicit': 'home/chosen' if explicit == '~/chosen' else 'chosen' if explicit else None,
                          'environment': 'environment' if environment else None, 'blocked': blocked,
                          'current': current, 'legacy': legacy, 'result': result.relative_to(root).as_posix(),
                          'portable_created': (app / 'data').is_dir()})
    return found
