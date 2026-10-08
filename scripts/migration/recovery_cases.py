"""Original interrupted-enable recovery; no actual user data or processes."""
from __future__ import annotations

import json
import tempfile
from pathlib import Path
from unittest.mock import patch
from cryptography.fernet import Fernet
from numera import crypto, vault


def cases(parent: Path) -> list[dict]:
    password='fixture-recovery-master'
    salt='AAECAwQFBgcICQoLDA0ODw=='
    cipher=Fernet(crypto._derive_key(password,salt))
    def seal(value):
        return 'enc:v2:'+cipher._encrypt_from_parts(value.encode('utf-8'),1700000000,bytes(range(16))).decode('ascii')
    desc={'salt':salt,'verifier':seal('numera-vault-ok')}
    journal={'new':desc,'op':'enable','version':1,'pid':0}
    variants=[({},None),({},password),({'proxies.json':[{'pass':'fixture-secret'}]},None),
              ({'proxies.json':[{'pass':'fixture-secret'}]},'wrong-fixture-password'),
              ({'proxies.json':[{'pass':'fixture-secret'}]},password),
              ({'proxies.json':[{'pass':seal('fixture-secret')}]},password),
              ({'proxies.json':[{'pass':'enc:v9:fixture'}]},password),
              ({'proxies.json':[42]},password)]
    parent.mkdir(parents=True,exist_ok=True)
    found=[]
    with patch('numera.crypto.encrypt',side_effect=lambda key,salt,value:seal(value)):
        for files,key in variants:
            with tempfile.TemporaryDirectory(prefix='recovery-oracle-',dir=parent) as directory:
                root=Path(directory)
                (root/'vault-migration.json').write_text(json.dumps(journal),encoding='utf-8')
                (root/'config.json').write_text('{}',encoding='utf-8')
                for name,data in files.items():
                    path=root/name;path.parent.mkdir(parents=True,exist_ok=True)
                    path.write_text(json.dumps(data),encoding='utf-8')
                before={name:(root/name).read_bytes() for name in (*files,'config.json','vault-migration.json')}
                def get():return json.loads((root/'config.json').read_text(encoding='utf-8'))
                def save(value):
                    from numera.paths import write_json
                    write_json(root/'config.json',value)
                try:
                    result,error,codes=vault.recover(root,get,save,password=key),None,[]
                except (RuntimeError,ValueError) as exc:
                    result,error,codes=None,str(exc),sorted(getattr(exc,'codes',[]))
                observed={}
                for name in files:
                    data=json.loads((root/name).read_text(encoding='utf-8'))
                    if isinstance(data,list):
                        for row in data:
                            if isinstance(row,dict) and crypto.is_supported_envelope(row.get('pass')):
                                row['pass']=crypto.decrypt(password,salt,row['pass'])
                    observed[name]=data
                found.append({'files':files,'journal':journal,'descriptor':desc,'password':key,
                              'result':result,'error':error,'codes':codes,'observed':observed,
                              'config':get(),'journal_retained':(root/'vault-migration.json').exists(),
                              'unchanged':{name:(root/name).exists() and before[name]==(root/name).read_bytes() for name in before}})
    return found
