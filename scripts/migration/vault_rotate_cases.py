"""Original master-password rotation: isolated stores, source-generated outputs."""
from __future__ import annotations

import json
import tempfile
from pathlib import Path
from unittest.mock import patch
from cryptography.fernet import Fernet
from numera import crypto, vault
from numera.store import ProfileStore


def cases(parent: Path) -> list[dict]:
    old_password, password = 'fixture-old-master', 'fixture-new-master'
    old_salt, salt = 'AAECAwQFBgcICQoLDA0ODw==', 'EBESExQVFhcYGRobHB0eHw=='
    def seal(key, used_salt, value):
        cipher = Fernet(crypto._derive_key(key, used_salt))
        return 'enc:v2:' + cipher._encrypt_from_parts(value.encode('utf-8'), 1700000000, bytes(range(16))).decode('ascii')
    old = {'salt': old_salt, 'verifier': seal(old_password, old_salt, 'numera-vault-ok')}
    initial = {'proxies.json': [{'pass': seal(old_password, old_salt, 'fixture-secret')}]}
    variants = [(initial, {'vault': old, 'future': 7}, old_password, password),
                (initial, {'vault': old}, 'fixture-wrong-old', password),
                (initial, {'vault': old}, old_password, ''),
                ({}, {}, old_password, password),
                ({'proxies.json': [{'pass': 'fixture-plaintext'}]}, {'vault': old}, old_password, password)]
    parent.mkdir(parents=True, exist_ok=True)
    result = []
    with patch('numera.crypto.new_salt', return_value=salt), patch('numera.crypto.encrypt', side_effect=seal):
        for files, config, supplied_old, supplied_new in variants:
            with tempfile.TemporaryDirectory(prefix='rotate-oracle-', dir=parent) as directory:
                root = Path(directory)
                store = ProfileStore(root)
                (root / 'config.json').write_text(json.dumps(config), encoding='utf-8')
                for name, data in files.items():
                    path = root / name
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_text(json.dumps(data), encoding='utf-8')
                before = {name: (root / name).read_bytes() for name in (*files, 'config.json')}
                try:
                    count, error = store.change_vault_password(supplied_old, supplied_new), None
                except (RuntimeError, ValueError) as exc:
                    count, error = None, str(exc)
                persisted = json.loads((root / 'config.json').read_text(encoding='utf-8'))
                active = persisted.get('vault')
                active_password = password if error is None else old_password
                observed = {}
                for path, site in vault.secret_files(root):
                    data = json.loads(path.read_text(encoding='utf-8'))
                    for holder in vault.holders(data, site):
                        if crypto.is_supported_envelope(holder[site.field]):
                            holder[site.field] = crypto.decrypt(active_password, active['salt'], holder[site.field])
                    observed[path.relative_to(root).as_posix()] = data
                if active:
                    assert crypto.check_password(active_password, active['salt'], active['verifier'])
                    persisted['vault'] = {'salt': 'authenticated-fixture-salt', 'verifier': 'authenticated-fixture-verifier'}
                result.append({'files': files, 'config': config, 'old_password': supplied_old, 'password': supplied_new,
                               'count': count, 'error': error, 'observed': observed, 'persisted': persisted,
                               'journal_retained': (root / 'vault-migration.json').exists(),
                               'unchanged': {name: before[name] == (root / name).read_bytes() for name in before}})
    return result
