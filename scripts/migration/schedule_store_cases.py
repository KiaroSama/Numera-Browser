"""Original occurrence claiming and outcome bookkeeping, no warm-up or browser."""
from __future__ import annotations

import json
import tempfile
from pathlib import Path
from numera.scheduler import ScheduleStore, next_run_at, due_schedules


def cases(parent: Path) -> list[dict]:
    parent.mkdir(parents=True, exist_ok=True)
    base = {'id': 'fixture-schedule', 'profileId': 'fixture-profile', 'everyHours': 1,
            'minutes': 5, 'preset': 'general', 'enabled': True, 'lastRun': None,
            'createdAt': 1000, 'future': {'retained': True}}
    variants = [('due', {}, 1000), ('not-due', {'lastRun': 1000}, 1001),
                ('disabled', {'enabled': False}, 1000),
                ('claimed', {'claimedBy': 'earlier', 'claimedAt': 999}, 1000),
                ('expired', {'claimedBy': 'earlier', 'claimedAt': 1}, 43201),
                ('string-time', {'everyHours': '2', 'lastRun': '1000'}, 8200),
                ('floored', {'everyHours': 0.0001, 'lastRun': 1000}, 1900)]
    found = []
    for name, patch, now in variants:
        now = float(now)
        with tempfile.TemporaryDirectory(prefix='schedule-store-oracle-', dir=parent) as directory:
            root = Path(directory)
            path = root / 'schedules.json'
            rows = [{**base, **patch}]
            path.write_text(json.dumps(rows), encoding='utf-8')
            before = path.read_bytes()
            store = ScheduleStore(root)
            due = due_schedules(store.list(), now)
            next_at = next_run_at(rows[0])
            read_unchanged = path.read_bytes() == before
            claimed = store.claim_due(base['id'], 'fixture-worker', now)
            after_claim = store.list()
            again = store.claim_due(base['id'], 'second-worker', now + 1)
            store.release(base['id'])
            released = store.get(base['id'])
            outcome = store.record(base['id'], lastRun=now, lastOutcome='done', lastError=None)
            after_record = store.list()
            try:
                store.record(base['id'], minutes=1)
                error = None
            except ValueError as exc:
                error = str(exc)
            missing = store.get('missing')
            deleted = store.delete(base['id'])
            deleted_again = store.delete(base['id'])
            found.append({'name': name, 'initial': rows, 'now': now, 'due': due,
                          'next': next_at, 'read_unchanged': read_unchanged,
                          'claimed': claimed, 'after_claim': after_claim, 'again': again,
                          'released': released, 'outcome': outcome, 'after_record': after_record,
                          'owned_error': error, 'missing': missing, 'deleted': deleted,
                          'deleted_again': deleted_again, 'final': store.list()})
    return found
