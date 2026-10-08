"""Retained bind refusal decisions only; never opens a socket or server."""
from __future__ import annotations

import json
import tempfile
from pathlib import Path
from unittest.mock import patch
from numera.server import _refuse_insecure_bind


def cases(parent: Path) -> list[dict]:
    parent.mkdir(parents=True, exist_ok=True)
    variants = [('127.0.0.1', '', '', None), (' LOCALHOST ', '', '', None),
                ('::1', '', '', None), ('0:0:0:0:0:0:0:1', '', '', None),
                ('', '', '', None), ('0.0.0.0', '', '', None),
                ('::', '', '', None), ('127.0.0.2', '', '', None),
                ('192.0.2.1', 'fixture-token', '', None),
                ('0.0.0.0', '', '0', None), ('0.0.0.0', '', ' ', None),
                ('0.0.0.0', '', '', [{'id': 'fixture', 'name': 'fixture', 'hash': 'synthetic'}]),
                ('0.0.0.0', '', '', []), ('0.0.0.0', '', '', '{'),
                ('0.0.0.0', '', '', {})]
    found = []
    for index, (host, environment, override, tokens) in enumerate(variants):
        with tempfile.TemporaryDirectory(prefix='bind-policy-oracle-', dir=parent) as directory:
            root = Path(directory)
            if tokens is not None:
                (root / 'tokens.json').write_text(tokens if isinstance(tokens, str) else json.dumps(tokens), encoding='utf-8')
            with patch.dict('os.environ', {'NUMERA_API_TOKEN': environment, 'NUMERA_ALLOW_INSECURE_BIND': override}):
                refusal = _refuse_insecure_bind(host, root)
            found.append({'name': f'bind-{index}', 'host': host, 'environment': environment,
                          'override': override, 'tokens': tokens, 'refusal': refusal})
    return found
