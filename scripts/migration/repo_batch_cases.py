"""Original multi-profile writes, config coupling and name-clash policies."""
from __future__ import annotations

import json
import tempfile
from pathlib import Path
from unittest.mock import patch
from numera.fingerprint import generate
from numera.models import Profile
from numera.store import ProfileStore
from numera.repo import ProfileRepo


def cases(parent: Path) -> list[dict]:
    base = Profile(id='fixture-a', name='alpha', fingerprint=generate(seed=42, os='windows'),
                   created_at=1700000000.0, updated_at=1700000000.0).to_dict()
    variants = [('reject', ['alpha', 'beta']), ('reject', ['alpha', 'alpha']),
                ('skip', ['alpha', 'alpha', 'gamma']), ('reject', []), ('invalid', ['alpha']),
                ('patch', ['alpha', 'beta']), ('numeric-held', ['123']),
                ('missing-id-held', ['occupied']), ('boolean-held', ['true']),
                ('legacy-list-name', ['free'])]
    parent.mkdir(parents=True, exist_ok=True)
    found = []
    for policy, names in variants:
        with tempfile.TemporaryDirectory(prefix='repo-batch-oracle-', dir=parent) as directory:
            root = Path(directory)
            store = ProfileStore(root)
            repo = ProfileRepo(store)
            config = {'future': 7, 'default_engine': 'playwright'}
            (root / 'config.json').write_text(json.dumps(config), encoding='utf-8')
            pending = [Profile.from_dict({**base, 'id': f'fixture-{index}', 'name': name}) for index, name in enumerate(names)]
            inputs = [profile.to_dict() for profile in pending]
            initial = {}
            if policy == 'patch':
                with patch('numera.repo.time.time', return_value=1700000000.0):
                    repo.save_many(pending)
                for kind in ('profiles', 'dashboard'):
                    for path in sorted((root / kind).glob('*.json')):
                        data = json.loads(path.read_text(encoding='utf-8'))
                        data['notes'] = 'concurrent committed notes'
                        path.write_text(json.dumps(data), encoding='utf-8')
                        initial[path.relative_to(root).as_posix()] = data
            if policy in ('numeric-held', 'missing-id-held', 'boolean-held', 'legacy-list-name'):
                old = {**base, 'id':'fixture-existing', 'name':{'numeric-held':123,'missing-id-held':'occupied','boolean-held':True,'legacy-list-name':['old']}[policy]}
                if policy == 'missing-id-held': old.pop('id')
                path = root / 'profiles/fixture-existing.json'
                path.write_text(json.dumps(old),encoding='utf-8')
                initial['profiles/fixture-existing.json'] = old
            before = (root / 'config.json').read_bytes()
            with patch('numera.repo.time.time', return_value=1700000000.0):
                try:
                    if policy == 'patch':
                        def apply(profile):
                            profile.tags.append('applied')
                            return True
                        saved = repo.patch_many([profile.id for profile in pending] + ['missing'], apply, config_change={'default_engine': 'patchright'})
                    else:
                        saved = repo.save_many(pending, config_change={'default_engine': 'patchright'}, on_name_clash='reject' if policy.endswith('-held') or policy == 'legacy-list-name' else policy)
                    result, error = [profile.to_dict() for profile in saved], None
                except (RuntimeError, ValueError) as exc:
                    result, error = None, str(exc)
            files = {path.relative_to(root).as_posix(): json.loads(path.read_text(encoding='utf-8'))
                     for kind in ('profiles', 'dashboard') for path in sorted((root / kind).glob('*.json'))}
            found.append({'profiles': inputs, 'policy': policy, 'config': config, 'initial': initial,
                          'changes': {'default_engine': 'patchright'}, 'result': result, 'error': error,
                          'written': files, 'persisted': json.loads((root / 'config.json').read_text(encoding='utf-8')),
                          'config_unchanged': before == (root / 'config.json').read_bytes()})
    return found
