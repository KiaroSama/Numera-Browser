"""Retained restore staging, session replacement, name and malformed ZIP cases."""
from __future__ import annotations

import base64
import json
import tempfile
import zipfile
from pathlib import Path
from unittest.mock import patch
from numera.fingerprint import generate
from numera.models import Profile
from numera.store import ProfileStore
from numera.repo import ProfileRepo


def cases(parent: Path) -> list[dict]:
    profile = Profile(id='fixture-restore',name='restore α',fingerprint=generate(seed=42,os='windows'),
                      created_at=1700000000.0,updated_at=1700000000.0).to_dict()
    parent.mkdir(parents=True,exist_ok=True)
    found = []
    variants = ('normal','replace','taken-name','missing-profile','bad-json','traversal','drive','leading-slash','backslash','unreadable-claim')
    for variant in variants:
        with tempfile.TemporaryDirectory(prefix='restore-oracle-',dir=parent) as directory:
            root = Path(directory)
            store = ProfileStore(root)
            repo = ProfileRepo(store)
            members = {'user-data/Default/Cookies':'fixture-new-session'}
            if variant == 'missing-profile': pass
            elif variant == 'bad-json': members['profile.json'] = '{ broken'
            else: members['profile.json'] = json.dumps(profile)
            if variant == 'traversal': members['user-data/../../../escaped'] = 'no'
            if variant == 'drive': members['user-data/C:/escaped'] = 'no'
            if variant == 'leading-slash': members['user-data//extra/file'] = 'allowed'
            if variant == 'backslash': members['user-data/extra\\file'] = 'allowed'
            source = root / 'incoming.zip'
            with zipfile.ZipFile(source,'w',zipfile.ZIP_DEFLATED) as archive:
                for name,value in members.items():
                    info = zipfile.ZipInfo(name,date_time=(2023,11,14,22,13,20))
                    # ZipInfo normalizes Windows separators on Windows only.
                    # Keep the wire input identical on both oracle platforms.
                    info.filename = name
                    info.create_system = 3
                    info.external_attr = 0o600 << 16
                    # Fixed stored bytes avoid zlib-build variance in input provenance;
                    # production backup compression is exercised by its separate seam.
                    info.compress_type = zipfile.ZIP_STORED
                    archive.writestr(info,value.encode('utf-8'))
            with patch('numera.repo.time.time',return_value=1700000000.0), patch('numera.sessions.platform.node',return_value='fixture-host'):
                if variant in ('replace','taken-name'):
                    existing = {**profile,'notes':'old notes'}
                    if variant == 'taken-name': existing['id'] = 'fixture-other'
                    repo.create(Profile.from_dict(existing))
                if variant == 'replace':
                    path = root / 'user-data/fixture-restore/stale.txt'
                    path.parent.mkdir(parents=True,exist_ok=True)
                    path.write_text('old session',encoding='utf-8')
                if variant == 'unreadable-claim':
                    path = root / 'sessions/fixture-restore.json'
                    path.parent.mkdir(parents=True,exist_ok=True)
                    path.write_text('{ broken',encoding='utf-8')
                initial = {path.relative_to(root).as_posix():json.loads(path.read_text(encoding='utf-8'))
                           for kind in ('profiles','dashboard') for path in sorted((root / kind).glob('*.json'))}
                initial_session = {path.relative_to(root / 'user-data').as_posix():base64.b64encode(path.read_bytes()).decode('ascii')
                                   for path in sorted((root / 'user-data').rglob('*')) if path.is_file()}
                try:
                    result,error = store.restore(source).to_dict(),None
                except (RuntimeError,ValueError) as exc:
                    result,error = None,str(exc)
                written = {path.relative_to(root).as_posix():json.loads(path.read_text(encoding='utf-8'))
                           for kind in ('profiles','dashboard') for path in sorted((root / kind).glob('*.json'))}
                session = {path.relative_to(root / 'user-data').as_posix():base64.b64encode(path.read_bytes()).decode('ascii')
                           for path in sorted((root / 'user-data').rglob('*')) if path.is_file()}
            found.append({'name':variant,'archive':base64.b64encode(source.read_bytes()).decode('ascii'),
                          'initial':initial,'initial_session':initial_session,'damaged_claim':variant == 'unreadable-claim',
                          'result':result,'error':error,'written':written,'session':session,
                          'staging_present':bool(list(root.glob('numera-restore-*')))})
    return found
