"""Exercise the retained HTTP middleware through real ASGI requests, no browsers."""
from __future__ import annotations

import json
import os
import tempfile
from pathlib import Path
from unittest.mock import patch
from fastapi.testclient import TestClient
from numera.server import create_app
from numera.server.stores import TokenStore


def cases(parent: Path) -> list[dict]:
    parent.mkdir(parents=True,exist_ok=True)
    fixed='fixture-env-api-token'
    named='fixture-named-api-token'
    rows=[{'id':'fixture-token','name':'Fixture','hash':TokenStore.hash(named),'created':1,'lastUsed':1700000000.0}]
    variants=[
        ('open','GET','/api/config',{},'',[],None),
        ('missing','GET','/api/config',{},fixed,[],None),
        ('bearer','GET','/api/config',{'authorization':'bEaReR '+fixed},fixed,[],None),
        ('x-header','GET','/api/config',{'x-numera-token':' '+fixed+' '},fixed,[],None),
        ('bearer-precedence','GET','/api/config',{'authorization':'Bearer wrong','x-numera-token':fixed},fixed,[],None),
        ('other-scheme','GET','/api/config',{'authorization':'Basic wrong','x-numera-token':fixed},fixed,[],None),
        ('named','GET','/api/config',{'authorization':'Bearer '+named},fixed,rows,{'protected':True}),
        ('revoked','GET','/api/config',{'authorization':'Bearer '+named},'',[],{'protected':True}),
        ('damaged','GET','/api/config',{},'','{ broken',{'protected':False}),
        ('ready','GET','/api/ready/',{},fixed,[],None),
        ('options','OPTIONS','/api/config',{},fixed,[],None),
        ('outside','GET','/not-an-api-route',{},fixed,[],None),
        ('api-root','GET','/api',{},fixed,[],None),
        ('wrong','GET','/api/config',{'authorization':'Bearer fixture-wrong'},fixed,[],None),
    ]
    found=[]
    for label,method,path,headers,environment,table,policy in variants:
        with tempfile.TemporaryDirectory(prefix='request-auth-oracle-',dir=parent) as directory:
            root=Path(directory)
            (root/'tokens.json').write_text(table if isinstance(table,str) else json.dumps(table),encoding='utf-8')
            if policy is not None:(root/'auth-policy.json').write_text(json.dumps(policy),encoding='utf-8')
            with patch.dict(os.environ,{'NUMERA_API_TOKEN':environment,'NUMERA_ALLOWED_ORIGINS':''}),patch('numera.server.stores.time.time',return_value=1700000000.0):
                app=create_app(data_dir=str(root),start_scheduler=False)
                with TestClient(app) as client:
                    response=client.request(method,path,headers=headers,follow_redirects=True)
                # Only auth verdict is compared here; downstream router/SPA has its own seam.
                denied=response.status_code==401
                body=response.json() if denied else None
                required=bool(environment) or TokenStore(root).protected()
            found.append({'name':label,'method':method,'path':path,'headers':headers,'environment':environment,
                          'table':table,'policy':policy,'denied':denied,'denial_body':body,'required':required})
    return found
