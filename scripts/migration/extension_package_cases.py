"""Source CRX2/3 and plain ZIP extraction; no browser or downloads."""
from __future__ import annotations

import base64
import io
import json
import struct
import tempfile
import zipfile
from pathlib import Path
from numera import webstore


def cases(parent: Path) -> list[dict]:
    parent.mkdir(parents=True,exist_ok=True)
    found = []
    for variant in ('zip','crx2','crx3','traversal','drive','unsupported','broken-zip'):
        members = {'manifest.json':json.dumps({'name':'Fixture α','version':'1','manifest_version':3}),
                   'js/content.js':'fixture-content'}
        if variant == 'traversal': members['../escaped.txt'] = 'no'
        if variant == 'drive': members['C:/escaped.txt'] = 'no'
        buffer = io.BytesIO()
        with zipfile.ZipFile(buffer,'w') as archive:
            for name,value in members.items():
                info = zipfile.ZipInfo(name,date_time=(2023,11,14,22,13,20))
                info.filename = name
                info.create_system = 3
                info.external_attr = 0o600 << 16
                info.compress_type = zipfile.ZIP_STORED
                archive.writestr(info,value.encode('utf-8'))
        package = buffer.getvalue()
        if variant == 'crx2': package = b'Cr24' + struct.pack('<III',2,3,4) + b'keysign' + package
        elif variant == 'crx3': package = b'Cr24' + struct.pack('<II',3,6) + b'header' + package
        elif variant == 'unsupported': package = b'Cr24' + struct.pack('<I',7) + package
        elif variant == 'broken-zip': package = b'not a ZIP'
        with tempfile.TemporaryDirectory(prefix='extension-package-oracle-',dir=parent) as directory:
            dest = Path(directory) / 'extension'
            dest.mkdir()
            try:
                if variant.startswith('crx') or variant == 'unsupported': webstore._unpack(package,dest)
                else: webstore._unpack_zip(package,dest)
                error = None
            except (RuntimeError,ValueError) as exc:
                error = str(exc)
            written = {path.relative_to(dest).as_posix():base64.b64encode(path.read_bytes()).decode('ascii')
                       for path in sorted(dest.rglob('*')) if path.is_file()}
            found.append({'name':variant,'package':base64.b64encode(package).decode('ascii'),
                          'crx':variant.startswith('crx') or variant == 'unsupported','error':error,'written':written})
    return found
