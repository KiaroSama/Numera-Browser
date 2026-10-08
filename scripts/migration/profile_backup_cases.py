"""Original backup ZIP contents and admission refusals in isolated stores."""
from __future__ import annotations

import base64
import json
import tempfile
import zipfile
from pathlib import Path
from unittest.mock import patch
from cryptography.fernet import Fernet
from numera import crypto
from numera.fingerprint import generate
from numera.models import Profile, Proxy
from numera.store import ProfileStore


def cases(parent: Path) -> list[dict]:
    password, salt = 'fixture-backup-master', 'AAECAwQFBgcICQoLDA0ODw=='
    cipher = Fernet(crypto._derive_key(password, salt))
    def seal(value):
        return 'enc:v2:' + cipher._encrypt_from_parts(value.encode('utf-8'),1700000000,bytes(range(16))).decode('ascii')
    descriptor = {'salt':salt,'verifier':seal('numera-vault-ok')}
    profile = Profile(id='fixture-backup', name='backup α', fingerprint=generate(seed=42,os='windows'),
                      proxy=Proxy(server='http://127.0.0.1:8080',password='fixture-secret'),
                      created_at=1700000000.0,updated_at=1700000000.0).to_dict()
    parent.mkdir(parents=True, exist_ok=True)
    found = []
    for variant in ('normal','missing','locked','unlocked','unreadable-claim'):
        with tempfile.TemporaryDirectory(prefix='backup-oracle-', dir=parent) as directory:
            root = Path(directory)
            config = {'vault':descriptor} if variant in ('locked','unlocked') else {}
            (root / 'config.json').write_text(json.dumps(config),encoding='utf-8')
            store = ProfileStore(root,password=password if variant == 'unlocked' else None)
            record = json.loads(json.dumps(profile))
            if config: record['proxy']['password'] = seal('fixture-secret')
            if variant != 'missing': (root / 'profiles/fixture-backup.json').write_text(json.dumps(record),encoding='utf-8')
            session = {'Default/Cookies':b'fixture-cookie-bytes','Local State':'α session'.encode('utf-8')}
            for name,data in session.items():
                path = root / 'user-data/fixture-backup' / name
                path.parent.mkdir(parents=True,exist_ok=True)
                path.write_bytes(data)
            if variant == 'unreadable-claim':
                path = root / 'sessions/fixture-backup.json'
                path.parent.mkdir(parents=True,exist_ok=True)
                path.write_text('{ broken',encoding='utf-8')
            dest = root / 'backup.zip'
            with patch('numera.sessions.platform.node',return_value='fixture-host'):
                try:
                    result,error = store.backup('fixture-backup',dest) is not None,None
                except (RuntimeError,ValueError) as exc:
                    result,error = None,str(exc)
            members = {}
            if dest.exists():
                with zipfile.ZipFile(dest) as archive:
                    for name in archive.namelist():
                        value = archive.read(name)
                        members[name] = json.loads(value.decode('utf-8')) if name == 'profile.json' else base64.b64encode(value).decode('ascii')
            found.append({'name':variant,'profile':record,'config':config,'password':password if variant == 'unlocked' else None,
                          'missing':variant == 'missing','damaged_claim':variant == 'unreadable-claim',
                          'session':{name:base64.b64encode(data).decode('ascii') for name,data in session.items()},
                          'result':result,'error':error,'destination_present':dest.exists(),'members':members})
    return found
