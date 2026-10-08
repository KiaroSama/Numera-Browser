"""Retained token-table states, sticky protection, verification and revocation."""
from __future__ import annotations

import json
import tempfile
from pathlib import Path
from unittest.mock import patch
from numera.server.stores import TokenStore


def cases(parent: Path) -> list[dict]:
    parent.mkdir(parents=True,exist_ok=True)
    token = 'fixture-api-token-only'
    entry = {'id':'fixture-token','name':'Fixture client','hash':TokenStore.hash(token),
             'created':1700000000.0,'lastUsed':None,'future':7}
    variants = [
        ('fresh',None,None,'state',None),
        ('legacy-tokens',[entry],None,'adopt',None),
        ('legacy-damaged','{ broken',None,'adopt',None),
        ('explicit-open',[entry],{'protected':False,'since':1},'adopt',None),
        ('empty-protected',[],{'protected':True,'since':1},'state',None),
        ('damaged-open','{ broken',{'protected':False,'since':1},'state',None),
        ('ambiguous-policy',[],{'protected':0},'state',None),
        ('verify',[entry],{'protected':True,'since':1},'verify',token),
        ('verify-recent',[{**entry,'lastUsed':1699999990.0}],{'protected':True},'verify',token),
        ('verify-wrong',[entry],{'protected':True},'verify','fixture-wrong'),
        ('verify-empty',[entry],None,'verify',''),
        ('verify-nonascii-hash',[{**entry,'hash':'α damaged'}],None,'verify',token),
        ('revoke-last',[entry],{'protected':True},'revoke',False),
        ('refuse-last',[entry],{'protected':True},'revoke',True),
        ('revoke-missing',[entry],{'protected':True},'revoke-missing',False),
        ('open-deliberately',[],{'protected':True},'set-open',None),
    ]
    found = []
    for label,table,policy,operation,argument in variants:
        with tempfile.TemporaryDirectory(prefix='token-oracle-',dir=parent) as directory:
            root = Path(directory)
            if table is not None:
                (root / 'tokens.json').write_text(table if isinstance(table,str) else json.dumps(table),encoding='utf-8')
            if policy is not None: (root / 'auth-policy.json').write_text(json.dumps(policy),encoding='utf-8')
            before = (root / 'tokens.json').read_bytes() if (root / 'tokens.json').exists() else None
            store = TokenStore(root)
            with patch('numera.server.stores.time.time',return_value=1700000000.0), patch('numera.server.auth_policy.time.time',return_value=1700000000.0):
                try:
                    if operation == 'adopt': result = store.adopt()
                    elif operation == 'verify': result = store.verify(argument)
                    elif operation in ('revoke','revoke-missing'): result = store.revoke('missing' if operation == 'revoke-missing' else 'fixture-token',refuse_lockout=argument)
                    elif operation == 'set-open': result = store.set_protected(False)
                    else: result = None
                    error = None
                except (RuntimeError,ValueError) as exc:
                    result,error = None,str(exc)
            after = (root / 'tokens.json').read_bytes() if (root / 'tokens.json').exists() else None
            found.append({'name':label,'table':table,'policy':policy,'operation':operation,'argument':argument,
                          'result':result,'error':error,'state':store.state(),'protected':store.protected(),
                          'listed':store.list(),'table_unchanged':before == after,
                          'persisted_policy':json.loads((root / 'auth-policy.json').read_text(encoding='utf-8')) if (root / 'auth-policy.json').exists() else None})
    return found
