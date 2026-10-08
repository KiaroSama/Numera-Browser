"""Source pool mutation while locked/unlocked, deterministic synthetic envelopes."""
from __future__ import annotations

import json
import tempfile
from pathlib import Path
from unittest.mock import patch
from cryptography.fernet import Fernet
from numera import crypto
from numera.store import ProfileStore
from numera.server.stores import ProxyPool


def cases(parent: Path) -> list[dict]:
    password = 'fixture-pool-master'
    salt = 'AAECAwQFBgcICQoLDA0ODw=='
    cipher = Fernet(crypto._derive_key(password, salt))
    def seal(key, used_salt, value):
        assert key == password and used_salt == salt
        return 'enc:v2:' + cipher._encrypt_from_parts(value.encode('utf-8'), 1700000000, bytes(range(16))).decode('ascii')
    config = {'vault': {'salt':salt, 'verifier':seal(password, salt, 'numera-vault-ok')}}
    initial = [{'id':'fixture-a', 'type':'HTTP', 'host':'host.test', 'port':'8080',
                'pass':seal(password, salt, 'fixture-secret'), 'future':7}]
    parent.mkdir(parents=True, exist_ok=True)
    found = []
    with patch('numera.crypto.encrypt', side_effect=seal):
        for key, changes in [(None, {'status':'working'}), (None, {'pass':None}),
                             (None, {'pass':''}), (None, {'pass':'fixture-replacement'}),
                             (password, {'pass':''}), (password, {'pass':'fixture-replacement'})]:
            with tempfile.TemporaryDirectory(prefix='pool-vault-oracle-', dir=parent) as directory:
                root = Path(directory)
                (root / 'config.json').write_text(json.dumps(config), encoding='utf-8')
                path = root / 'proxies.json'
                path.write_text(json.dumps(initial), encoding='utf-8')
                pool = ProxyPool(root, ProfileStore(root, password=key))
                before = path.read_bytes()
                try:
                    result, error = pool.update('fixture-a', changes), None
                except (RuntimeError, ValueError) as exc:
                    result, error = None, str(exc)
                written = json.loads(path.read_text(encoding='utf-8'))
                observed = json.loads(json.dumps(written))
                for row in observed:
                    if crypto.is_supported_envelope(row.get('pass')):
                        row['pass'] = crypto.decrypt(password, salt, row['pass'])
                found.append({'initial':initial, 'config':config, 'password':key, 'changes':changes,
                              'result':result, 'error':error, 'listed':pool.list(), 'observed':observed,
                              'unchanged':before == path.read_bytes()})
    return found
