"""Original secret-file migration outcomes; compare decrypted values, not random IVs."""
from __future__ import annotations

import json
import tempfile
from pathlib import Path
from numera import crypto, vault


def cases(parent: Path) -> list[dict]:
    password='fixture-migration-master'
    salt='AAECAwQFBgcICQoLDA0ODw=='
    variants=[{'profiles/a.json':{'proxy':{'password':'fixture-a'},'future':42},
               'dashboard/a.json':{'proxy':{'pass':'fixture-a'},'folder':'keep'},
               'proxies.json':[{'pass':'fixture-b'},{'pass':''},{'host':'fixture'}]},
              {'profiles/a.json':{'proxy':None},'proxies.json':[]},
              {'profiles/a.json':{'proxy':{'password':'enc:v9:fixture'}}},
              {'profiles/a.json':{'proxy':{'password':'fixture-a'}},'dashboard/b.json':'broken-json'}]
    parent.mkdir(parents=True,exist_ok=True)
    found=[]
    for files in variants:
        with tempfile.TemporaryDirectory(prefix='migration-oracle-',dir=parent) as directory:
            root=Path(directory)
            for name,data in files.items():
                path=root/name;path.parent.mkdir(parents=True,exist_ok=True)
                path.write_text('{broken' if data=='broken-json' else json.dumps(data),encoding='utf-8')
            before={name:(root/name).read_bytes() for name in files}
            step=vault._Migration(root)
            try:
                count,error=step.encrypt_all(password,salt),None
            except ValueError:
                count,error=None,'unreadable'
                assert not step.rollback(),'Source rollback failed'
            observed={}
            encrypted=0
            for path,site in vault.secret_files(root):
                raw=path.read_text(encoding='utf-8')
                try:data=json.loads(raw)
                except json.JSONDecodeError:
                    observed[path.relative_to(root).as_posix()]='broken-json';continue
                for holder in vault.holders(data,site):
                    value=holder[site.field]
                    if crypto.is_supported_envelope(value):
                        encrypted+=1
                        holder[site.field]=crypto.decrypt(password,salt,value)
                observed[path.relative_to(root).as_posix()]=data
            found.append({'files':files,'password':password,'salt':salt,'count':count,'error':error,
                          'observed':observed,'encrypted':encrypted,
                          'unchanged':{name:before[name]==(root/name).read_bytes() for name in files}})
    return found
