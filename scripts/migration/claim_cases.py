"""Claim uncertainty from original public state/read boundaries, no OS doubles."""
from __future__ import annotations

import platform
import tempfile
from pathlib import Path
from numera import sessions


def cases(parent: Path) -> list[dict]:
    host = platform.node()
    base = {'session': 'fixture-session', 'host': host, 'pid': 42, 'token': None,
            'tree': {'kind': 'job', 'group': 'fixture-job'}}
    records = [None, {}, {'session': 'fixture-session'},
               {**base, 'host': 'fixture-foreign-host'},
               *({**base, 'host': value} for value in (True, 42, ['fixture-host'])),
               *({**base, 'session': value} for value in (None, '', True, 7, [])),
               *({**base, 'pid': value} for value in (-1, True, 4.5, '42', None)),
               *({**base, 'token': value} for value in (True, {}, [])),
               *({**base, 'tree': value} for value in (
                   None, [], {}, {'kind': '', 'group': 'fixture'},
                   {'kind': 'future', 'group': 'fixture'},
                   {'kind': 'job', 'group': ''}, {'kind': 'job', 'group': 42},
                   {'kind': 'pgid', 'group': True}, {'kind': 'pgid', 'group': 0},
                   {'kind': 'pgid', 'group': -1}, {'kind': 'pgid', 'group': '42'}))]
    result = [{'input': record, 'state': sessions.state(record)} for record in records]
    parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='claim-oracle-', dir=parent) as directory:
        root = Path(directory)
        path = root / 'sessions' / 'fixture-profile.json'
        path.parent.mkdir()
        for raw in ('{broken', '[]', 'null', 'true', '42', '{}'):
            path.write_text(raw, encoding='utf-8')
            claim = sessions.read(root, 'fixture-profile')
            result.append({'raw': raw, 'state': sessions.state(claim)})
        path.unlink()
        result.append({'raw': None, 'state': sessions.state(sessions.read(root, 'fixture-profile'))})
    # A stable local host lets the target test reproduce source equality without
    # baking the machine's identity into a tracked fixture.
    for case in result:
        record = case.get('input')
        if isinstance(record, dict) and record.get('host') == host:
            record['host'] = 'fixture-local-host'
    return result
