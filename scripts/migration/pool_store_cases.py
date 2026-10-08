"""Source pool read/edit/delete outcomes, preserving credential write intent."""
from __future__ import annotations

import json
import tempfile
from pathlib import Path
from numera.server.stores import ProxyPool


def cases(parent: Path) -> list[dict]:
    initial = [{'id':'fixture-a', 'type':'HTTP', 'host':'host.test', 'port':'8080',
                'user':'fixture-user', 'pass':'fixture-secret', 'status':'untested', 'future':7},
               {'id':'fixture-b', 'type':'SOCKS5', 'host':'host.test', 'port':'1080',
                'user':None, 'pass':None, 'status':'untested'}]
    variants = [('update', 'fixture-a', {'status':'working'}),
                ('update', 'fixture-a', {'pass':''}),
                ('update', 'missing', {'status':'working'}),
                ('update-many', None, {'fixture-a':{'status':'working'}, 'fixture-b':{'country':'US'}}),
                ('delete', 'fixture-a', None), ('delete', 'missing', None)]
    parent.mkdir(parents=True, exist_ok=True)
    found = []
    for operation, pid, changes in variants:
        with tempfile.TemporaryDirectory(prefix='pool-store-oracle-', dir=parent) as directory:
            root = Path(directory)
            path = root / 'proxies.json'
            path.write_text(json.dumps(initial), encoding='utf-8')
            pool = ProxyPool(root)
            before = path.read_bytes()
            result = pool.update(pid, changes) if operation == 'update' else pool.update_many(changes) if operation == 'update-many' else pool.delete(pid)
            found.append({'initial':initial, 'operation':operation, 'id':pid, 'changes':changes, 'result':result,
                          'listed':pool.list(), 'written':json.loads(path.read_text(encoding='utf-8')), 'unchanged':before == path.read_bytes()})
    return found
