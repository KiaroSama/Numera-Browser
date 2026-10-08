"""Authenticated vault completion outcomes from the original recovery boundary."""
from __future__ import annotations

import json
import tempfile
from pathlib import Path
from cryptography.fernet import Fernet
from numera import crypto
from numera.vault_recovery import completion_problems


def cases(parent: Path) -> list[dict]:
    salt='AAECAwQFBgcICQoLDA0ODw=='
    password='fixture-completion-master'
    cipher=Fernet(crypto._derive_key(password,salt))
    def seal(value):
        return 'enc:v2:'+cipher._encrypt_from_parts(value.encode('utf-8'),1700000000,bytes(range(16))).decode('ascii')
    descriptor={'salt':salt,'verifier':seal('numera-vault-ok')}
    variants=[({},None), ({},password), ({},'wrong-fixture-master'),
              ({'proxies.json':[{'pass':'fixture-plaintext'}]},password),
              ({'proxies.json':[{'pass':seal('fixture-secret')}]},password),
              ({'proxies.json':[{'pass':'enc:v9:fixture'}]},password),
              ({'proxies.json':None},password),({'proxies.json':[42]},password),
              ({'profiles/a.json':{'proxy':'broken'}},password),
              ({'dashboard/a.json':{'proxy':{'pass':42}}},password),
              ({'profiles/a.json':{'proxy':None}},password),
              ({'profiles/a.json':{'proxy':{'password':seal('fixture-secret')}},'proxies.json':[{'pass':seal('fixture-secret')}]},password)]
    parent.mkdir(parents=True,exist_ok=True)
    found=[]
    for files,key in variants:
        with tempfile.TemporaryDirectory(prefix='completion-oracle-',dir=parent) as directory:
            root=Path(directory)
            for name,data in files.items():
                path=root/name
                path.parent.mkdir(parents=True,exist_ok=True)
                path.write_text(json.dumps(data),encoding='utf-8')
            before={name:(root/name).read_bytes() for name in files}
            expected=[{'code':p.code,'detail':p.detail} for p in completion_problems(root,descriptor,key)]
            assert before=={name:(root/name).read_bytes() for name in files},'Completion mutated source files'
            found.append({'files':files,'descriptor':descriptor,'password':key,'expected':expected})
    return found
