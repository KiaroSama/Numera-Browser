"""Interrupted source rotations, preserving both authenticated generations."""
from __future__ import annotations

import json
import tempfile
from pathlib import Path
from unittest.mock import patch
from cryptography.fernet import Fernet
from numera import crypto, vault


def cases(parent: Path) -> list[dict]:
    old_password, password = 'fixture-old-master', 'fixture-new-master'
    old_salt, salt = 'AAECAwQFBgcICQoLDA0ODw==', 'EBESExQVFhcYGRobHB0eHw=='
    def seal(key, used_salt, value):
        cipher = Fernet(crypto._derive_key(key, used_salt))
        return 'enc:v2:' + cipher._encrypt_from_parts(value.encode('utf-8'), 1700000000, bytes(range(16))).decode('ascii')
    old = {'salt': old_salt, 'verifier': seal(old_password, old_salt, 'numera-vault-ok')}
    new = {'salt': salt, 'verifier': seal(password, salt, 'numera-vault-ok')}
    before_token = seal(old_password, old_salt, 'fixture-before')
    after_token = seal(password, salt, 'fixture-after')
    journal = {'version': 1, 'op': 'rotate', 'old': old, 'new': new, 'pid': 0}
    variants = [([{'pass': before_token}, {'pass': after_token}], old, password, old_password),
                ([{'pass': before_token}], old, password, None),
                ([{'pass': after_token}], new, password, None),
                ([{'pass': before_token}, {'pass': 'fixture-plaintext'}], old, password, old_password),
                ([{'pass': before_token}], old, 'fixture-wrong-new', old_password),
                ([{'pass': before_token}], old, password, 'fixture-wrong-old'),
                ([42], old, password, old_password),
                ([{'pass': before_token}], None, password, old_password),
                ([{'pass': before_token}], {}, password, old_password),
                ([{'pass': before_token}], {'salt': old_salt}, password, old_password)]
    parent.mkdir(parents=True, exist_ok=True)
    found = []
    with patch('numera.crypto.encrypt', side_effect=seal):
        for pool, active, key, previous in variants:
            with tempfile.TemporaryDirectory(prefix='rotate-recovery-oracle-', dir=parent) as directory:
                root = Path(directory)
                config = {'vault': active, 'future': 7}
                for name, value in [('proxies.json', pool), ('config.json', config), ('vault-migration.json', journal)]:
                    (root / name).write_text(json.dumps(value), encoding='utf-8')
                before = {name: (root / name).read_bytes() for name in ('proxies.json', 'config.json', 'vault-migration.json')}
                def get(): return json.loads((root / 'config.json').read_text(encoding='utf-8'))
                def save(value):
                    from numera.paths import write_json
                    write_json(root / 'config.json', value)
                try:
                    result, error, codes, role = vault.recover(root, get, save, password=key, old_password=previous), None, [], None
                except (RuntimeError, ValueError) as exc:
                    result, error, codes, role = None, str(exc), sorted(getattr(exc, 'codes', [])), getattr(exc, 'role', None)
                observed = json.loads((root / 'proxies.json').read_text(encoding='utf-8'))
                for row in observed:
                    if isinstance(row, dict) and crypto.is_supported_envelope(row.get('pass')):
                        token = row['pass']
                        try: row['pass'] = crypto.decrypt(password, salt, token)
                        except (RuntimeError, ValueError): row['pass'] = crypto.decrypt(old_password, old_salt, token)
                found.append({'pool': pool, 'config': config, 'journal': journal, 'password': key, 'old_password': previous,
                              'result': result, 'error': error, 'codes': codes, 'role': role, 'observed': observed, 'persisted': get(),
                              'journal_retained': (root / 'vault-migration.json').exists(),
                              'unchanged': {name: (root / name).exists() and before[name] == (root / name).read_bytes() for name in before}})
    return found
