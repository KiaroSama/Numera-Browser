"""Retained extension installation with a controlled package download seam."""
from __future__ import annotations

import base64
import io
import json
import struct
import tempfile
import zipfile
from pathlib import Path
from unittest.mock import patch
from numera import webstore


def package(files: dict) -> bytes:
    output = io.BytesIO()
    with zipfile.ZipFile(output,'w') as archive:
        for name,text in files.items():
            info = zipfile.ZipInfo(name,date_time=(2023,11,14,22,13,20))
            info.filename = name
            info.create_system = 3
            info.external_attr = 0o600 << 16
            archive.writestr(info,text.encode('utf-8'))
    return b'Cr24' + struct.pack('<II',3,0) + output.getvalue()


def cases(parent: Path) -> list[dict]:
    parent.mkdir(parents=True,exist_ok=True)
    ext_id = 'a' * 32
    found = []
    for variant in ('new','reused','force','missing-manifest','uninstall','already-gone'):
        with tempfile.TemporaryDirectory(prefix='extension-install-oracle-',dir=parent) as directory:
            root = Path(directory)
            dest = root / 'extensions' / ext_id
            initial = {}
            if variant in ('reused','force','uninstall'):
                initial = {'manifest.json':json.dumps({'name':'Original','version':'1'}),'old.txt':'original file'}
                dest.mkdir(parents=True)
                for name,text in initial.items(): (dest / name).write_text(text,encoding='utf-8')
            payload = package({'content.js':'new content'} if variant == 'missing-manifest' else
                              {'manifest.json':json.dumps({'name':'Installed α','version':'2'}),'new.txt':'new file'})
            with patch('numera.webstore._download',return_value=payload) as download:
                try:
                    if variant in ('uninstall','already-gone'):
                        result = webstore.uninstall(ext_id,root)
                    else:
                        result = webstore.install(ext_id,root,force=variant == 'force')
                        result['path'] = Path(result['path']).relative_to(root).as_posix()
                    error = None
                except webstore.WebStoreError as exc:
                    result,error = None,str(exc)
                calls = download.call_count
            written = {path.relative_to(dest).as_posix():base64.b64encode(path.read_bytes()).decode('ascii')
                       for path in sorted(dest.rglob('*')) if path.is_file()} if dest.exists() else {}
            found.append({'name':variant,'initial':initial,'package':base64.b64encode(payload).decode('ascii'),
                          'result':result,'error':error,'download_calls':calls,'written':written,'destination_present':dest.exists()})
    return found
