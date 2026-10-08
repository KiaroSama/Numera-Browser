"""Read-only source journal classification in disposable local directories."""
from __future__ import annotations

import json
import tempfile
from pathlib import Path
from numera.vault_recovery import classify


def cases(root: Path) -> list[dict]:
    descriptor = {'salt':'AAECAwQFBgcICQoLDA0ODw==','verifier':'fixture-verifier'}
    inputs = [None, 'broken-json', {}, [], True,
              {'salt':descriptor['salt'],'verifier':descriptor['verifier']},
              {'new':descriptor,'op':'enable'}, {'new':descriptor,'old':descriptor,'op':'rotate'},
              {'new':descriptor,'op':'rotate'}, {'new':descriptor,'op':'unknown'}]
    inputs += [{'new':descriptor,'version':value} for value in (True,False,0,1,1.0,2,-1,'1',None)]
    inputs += [{'new':{'salt':salt,'verifier':verifier}} for salt,verifier in
               [('not-base64','fixture'),('','fixture'),(descriptor['salt'],''),(descriptor['salt'],None)]]
    root.mkdir(parents=True,exist_ok=True)
    found = []
    with tempfile.TemporaryDirectory(prefix='journal-oracle-',dir=root) as directory:
        path = Path(directory) / 'vault-migration.json'
        for value in inputs:
            path.unlink(missing_ok=True)
            if value == 'broken-json':
                path.write_text('{broken',encoding='utf-8')
            elif value is not None:
                path.write_text(json.dumps(value),encoding='utf-8')
            before = path.read_bytes() if path.exists() else None
            state, record = classify(directory)
            after = path.read_bytes() if path.exists() else None
            if before != after:
                raise RuntimeError('Read-only source classification mutated journal')
            found.append({'name':f'journal-{len(found)}','input':value,'expected':state,'record':record})
    return found
