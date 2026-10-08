"""Original token mint response, duplicate-name refusal and stored digest only."""
from __future__ import annotations

import json
import tempfile
import uuid
from pathlib import Path
from unittest.mock import patch
from numera.server.stores import TokenStore


def cases(parent: Path) -> list[dict]:
    parent.mkdir(parents=True,exist_ok=True)
    token_suffix = 'fixture-token-suffix-only'
    found = []
    for name,existing in [('Client',[]),('  Client  ',[]),('',[]),('Client',[{'id':'original','name':' Client ','hash':'fixture-digest'}])]:
        with tempfile.TemporaryDirectory(prefix='token-create-oracle-',dir=parent) as directory:
            root = Path(directory)
            path = root / 'tokens.json'
            path.write_text(json.dumps(existing),encoding='utf-8')
            before = path.read_bytes()
            store = TokenStore(root)
            with patch('numera.server.stores.secrets.token_urlsafe',return_value=token_suffix), patch('numera.server.stores.uuid.uuid4',return_value=uuid.UUID('00112233-4455-4677-8899-aabbccddeeff')), patch('numera.server.stores.time.time',return_value=1700000000.0):
                try:
                    result,error = store.create(name),None
                except (RuntimeError,ValueError) as exc:
                    result,error = None,str(exc)
            found.append({'input':name,'initial':existing,'result':result,'error':error,'listed':store.list(),
                          'table_unchanged':before == path.read_bytes(),'protected':store.protected(),
                          'policy':json.loads((root / 'auth-policy.json').read_text(encoding='utf-8')) if (root / 'auth-policy.json').exists() else None})
    return found
