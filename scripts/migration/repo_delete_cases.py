"""Source deletion disposes both copies and a real isolated session directory."""
from __future__ import annotations

import json
import tempfile
from pathlib import Path
from numera.models import Profile
from numera.fingerprint import generate
from numera.store import ProfileStore


def cases(parent: Path) -> list[dict]:
    profile = Profile(id='fixture-delete', name='alpha', fingerprint=generate(seed=42, os='windows'),
                      created_at=1700000000.0, updated_at=1700000000.0).to_dict()
    parent.mkdir(parents=True, exist_ok=True)
    found = []
    for reference, corrupt in [('fixture-delete', False), ('alpha', False), ('missing', False), ('fixture-delete', True)]:
        with tempfile.TemporaryDirectory(prefix='delete-oracle-', dir=parent) as directory:
            root = Path(directory)
            store = ProfileStore(root)
            store.profiles_dir.joinpath('fixture-delete.json').write_text(json.dumps(profile), encoding='utf-8')
            session = root / 'user-data' / 'fixture-delete'
            session.mkdir()
            (session / 'fixture-state.txt').write_text('isolated browser state α', encoding='utf-8')
            if corrupt:
                (root / 'sessions').mkdir()
                (root / 'sessions' / 'fixture-delete.json').write_text('{broken', encoding='utf-8')
            try:
                result, error = store.delete(reference), None
            except (RuntimeError, ValueError) as exc:
                result, error = None, str(exc)
            found.append({'profile': profile, 'reference': reference, 'corrupt_claim': corrupt,
                          'result': result, 'error': error,
                          'engine_present': (root / 'profiles' / 'fixture-delete.json').exists(),
                          'dashboard_present': (root / 'dashboard' / 'fixture-delete.json').exists(),
                          'session_present': session.exists(),
                          'claim_retained': (root / 'sessions' / 'fixture-delete.json').exists()})
    return found
