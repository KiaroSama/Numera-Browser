"""Source schedule create/update validation, with no scheduled work executed."""
from __future__ import annotations

import json
import tempfile
from pathlib import Path
from unittest.mock import patch
from numera.scheduler import ScheduleStore, validate_schedule


def cases(parent: Path) -> list[dict]:
    parent.mkdir(parents=True, exist_ok=True)
    requests = [({}, False), ({'everyHours': 0.0001}, False),
                ({'everyHours': '2', 'minutes': '7', 'preset': 'ADS', 'enabled': False}, False),
                ({'everyHours': 0}, False), ({'everyHours': 721}, False),
                ({'everyHours': 'bad'}, False), ({'everyHours': 'inf'}, False),
                ({'minutes': 0}, False), ({'minutes': 481}, False),
                ({'minutes': 'nan'}, False), ({'minutes': None}, False),
                ({'preset': 'unknown'}, False), ({'preset': 1}, False),
                ({'enabled': 1}, False), ({'preset': None}, True),
                ({'future': 1}, True), ({'lastRun': 1}, True),
                ({'id': 'other'}, True), ({'profileId': 'other'}, True),
                ({'everyHours': '١.٥', 'minutes': '２'}, False),
                ({'everyHours': '1_0', 'minutes': '1_0'}, False),
                ({'everyHours': '1__0'}, False), ({'everyHours': '0x10'}, False),
                ({'minutes': 1e6}, False), ({'minutes': -1e-8}, False),
                ({'minutes': 480.1256789}, False)]
    found = []
    for index, (request, partial) in enumerate(requests):
        try:
            result, error = validate_schedule(request, partial=partial), None
        except ValueError as exc:
            result, error = None, str(exc)
        found.append({'name': f'validate-{index}', 'operation': 'validate',
                      'input': request, 'partial': partial, 'result': result, 'error': error})
    with tempfile.TemporaryDirectory(prefix='schedule-write-oracle-', dir=parent) as directory:
        root = Path(directory)
        store = ScheduleStore(root)
        with patch('numera.scheduler.uuid.uuid4') as random:
            random.return_value.hex = '0123456789abcdef0123456789abcdef'
            created = store.add(profile_id='fixture-profile', every_hours=0.0001, minutes=7,
                                preset='ADS', enabled=False, now=1000.0)
        before = store.list()
        updated = store.update(created['id'], {'everyHours': '2', 'enabled': True, 'preset': None, 'future': 7})
        after = store.list()
        bytes_before = store.path.read_bytes()
        try:
            store.update(created['id'], {'lastRun': 99})
            error = None
        except ValueError as exc:
            error = str(exc)
        missing = store.update('missing', {'minutes': 1})
        found.append({'name': 'create-update', 'operation': 'store', 'created': created,
                      'before': before, 'updated': updated, 'after': after, 'error': error,
                      'unchanged': bytes_before == store.path.read_bytes(), 'missing': missing})
    return found
