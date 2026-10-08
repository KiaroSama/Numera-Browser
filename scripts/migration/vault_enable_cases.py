"""Ordinary source vault enable: real isolated files and deterministic source key."""
from __future__ import annotations

import json
import tempfile
from pathlib import Path
from unittest.mock import patch
from cryptography.fernet import Fernet
from numera import crypto, vault
from numera.store import ProfileStore


def cases(parent: Path) -> list[dict]:
    password = 'fixture-enable-master'
    salt = 'AAECAwQFBgcICQoLDA0ODw=='
    cipher = Fernet(crypto._derive_key(password, salt))
    def seal(key, used_salt, value):
        assert key == password and used_salt == salt
        return 'enc:v2:' + cipher._encrypt_from_parts(value.encode('utf-8'), 1700000000, bytes(range(16))).decode('ascii')
    descriptor = {'salt': salt, 'verifier': seal(password, salt, 'numera-vault-ok')}
    variants = [({}, {}),
                ({'profiles/a.json': {'proxy': {'password': 'fixture-a'}, 'future': 7},
                  'dashboard/a.json': {'proxy': {'pass': 'fixture-a'}, 'folder': 'keep'},
                  'proxies.json': [{'pass': 'fixture-b'}, {'pass': ''}]}, {'future': {'keep': True}}),
                ({'proxies.json': [42]}, {}),
                ({'proxies.json': [{'pass': 'enc:v9:fixture'}]}, {}),
                ({}, {'vault': descriptor})]
    parent.mkdir(parents=True, exist_ok=True)
    result = []
    with patch('numera.crypto.new_salt', return_value=salt), patch('numera.crypto.encrypt', side_effect=seal):
        for files, config in variants:
            with tempfile.TemporaryDirectory(prefix='enable-oracle-', dir=parent) as directory:
                root = Path(directory)
                store = ProfileStore(root)
                (root / 'config.json').write_text(json.dumps(config), encoding='utf-8')
                for name, data in files.items():
                    path = root / name
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_text(json.dumps(data), encoding='utf-8')
                before = {name: (root / name).read_bytes() for name in (*files, 'config.json')}
                try:
                    count, error = store.enable_vault(password), None
                except (RuntimeError, ValueError) as exc:
                    count, error = None, str(exc)
                observed = {}
                for path, site in vault.secret_files(root):
                    data = json.loads(path.read_text(encoding='utf-8'))
                    for holder in vault.holders(data, site):
                        if crypto.is_supported_envelope(holder[site.field]):
                            holder[site.field] = crypto.decrypt(password, salt, holder[site.field])
                    observed[path.relative_to(root).as_posix()] = data
                persisted = json.loads((root / 'config.json').read_text(encoding='utf-8'))
                # Compare authenticated descriptor meaning, not randomized IVs.
                if persisted.get('vault'):
                    assert crypto.check_password(password, persisted['vault']['salt'], persisted['vault']['verifier'])
                    persisted['vault'] = {'salt': 'authenticated-fixture-salt', 'verifier': 'authenticated-fixture-verifier'}
                result.append({'files': files, 'config': config, 'password': password, 'salt': salt,
                               'count': count, 'error': error, 'observed': observed, 'persisted': persisted,
                               'journal_retained': (root / 'vault-migration.json').exists(),
                               'unchanged': {name: before[name] == (root / name).read_bytes() for name in before}})
    return result
