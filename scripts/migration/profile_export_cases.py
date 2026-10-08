"""Original profile export, including refusal to silently omit a locked secret."""
from __future__ import annotations

import json
import tempfile
from pathlib import Path
from cryptography.fernet import Fernet
from numera import crypto
from numera.fingerprint import generate
from numera.models import Profile, Proxy
from numera.store import ProfileStore


def cases(parent: Path) -> list[dict]:
    password, salt = 'fixture-export-master', 'AAECAwQFBgcICQoLDA0ODw=='
    cipher = Fernet(crypto._derive_key(password, salt))
    def seal(value):
        return 'enc:v2:' + cipher._encrypt_from_parts(value.encode('utf-8'), 1700000000, bytes(range(16))).decode('ascii')
    descriptor = {'salt':salt, 'verifier':seal('numera-vault-ok')}
    profile = Profile(id='fixture-export', name='fixture α', fingerprint=generate(seed=42, os='windows'),
                      proxy=Proxy(server='http://127.0.0.1:8080', password='fixture-secret'),
                      created_at=1700000000.0, updated_at=1700000000.0).to_dict()
    parent.mkdir(parents=True, exist_ok=True)
    found = []
    for enabled, key, missing in [(False,None,False),(True,None,False),(True,password,False),(False,None,True)]:
        with tempfile.TemporaryDirectory(prefix='export-oracle-', dir=parent) as directory:
            root = Path(directory)
            config = {'vault':descriptor} if enabled else {}
            (root / 'config.json').write_text(json.dumps(config),encoding='utf-8')
            store = ProfileStore(root,password=key)
            record = json.loads(json.dumps(profile))
            if enabled: record['proxy']['password'] = seal('fixture-secret')
            if not missing: (root / 'profiles/fixture-export.json').write_text(json.dumps(record),encoding='utf-8')
            dest = root / 'export.json'
            try:
                output = store.export('fixture-export',dest)
                result,error = output is not None,None
            except (RuntimeError,ValueError) as exc:
                result,error = None,str(exc)
            found.append({'profile':record,'config':config,'password':key,'missing':missing,
                          'result':result,'error':error,'written':json.loads(dest.read_text(encoding='utf-8')) if dest.exists() else None,
                          'destination_present':dest.exists()})
    return found
