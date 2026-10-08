"""Refuse a default-runtime switch while source capabilities are unverified."""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from run_logging import run_log
from inventory import discover

ROOT = Path(__file__).resolve().parents[2]


def check(path: Path) -> list[str]:
    ledger = json.loads(path.read_text(encoding='utf-8'))
    if not isinstance(ledger, dict):
        return ['Migration capability ledger is not an object']
    rows = ledger.get('capabilities')
    if not isinstance(rows, list) or not rows:
        return ['Migration capability ledger is missing or empty']
    actual = discover()
    if ledger.get('files') != actual['files'] or ledger.get('source_commit') != actual.get('source_commit'):
        return ['Migration source inventory drifted; coverage cannot be trusted']
    expected_ids = {row['id'] for row in actual['capabilities']}
    recorded_ids = {row.get('id') for row in rows if isinstance(row, dict) and isinstance(row.get('id'), str)}
    if expected_ids != recorded_ids:
        return ['Migration capability inventory is incomplete or contains unknown source symbols']
    problems = []
    identifiers = set()
    for row in rows:
        if not isinstance(row, dict) or not isinstance(row.get('id'), str):
            problems.append('Malformed source capability entry')
            continue
        identifier = row['id']
        if identifier in identifiers:
            problems.append(f'Duplicate capability: {identifier}')
        identifiers.add(identifier)
        if row.get('status') != 'verified':
            problems.append(f'Unverified capability: {identifier}')
            continue
        for key in ('target', 'test'):
            value = row.get(key)
            if not isinstance(value, str):
                problems.append(f'Missing {key} evidence: {identifier}')
                continue
            candidate = (ROOT / value).resolve()
            if not candidate.is_relative_to(ROOT.resolve()) or not candidate.is_file():
                problems.append(f'Absent or external {key} evidence: {identifier}')
    return problems


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('ledger', type=Path)
    args = parser.parse_args(argv)
    try:
        problems = check(args.ledger)
    except (OSError, ValueError, TypeError):
        print('Migration ledger unreadable; original runtime remains required', file=sys.stderr)
        return 1
    if problems:
        print(f'Migration not ready: {len(problems)} coverage problems; original runtime remains required', file=sys.stderr)
        for problem in problems[:5]:
            print(problem, file=sys.stderr)
        return 1
    print('Capability mapping complete; actual platform/performance/rollback gates are still independently required')
    return 0


if __name__ == '__main__':
    with run_log('check-coverage') as log:
        code = main()
        log.info('Coverage verdict exit=%s', code)
    raise SystemExit(code)
