"""Source-runtime contender for native lock interoperability, test inputs only."""
from __future__ import annotations

import argparse
import os
import sys
from pathlib import Path

from baseline_source import activate
activate()
from numera.data_locks import transaction_lock, session_lease, _lock_for  # noqa: E402


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('kind', choices=('root', 'file', 'session'))
    parser.add_argument('path', type=Path)
    parser.add_argument('--hold', action='store_true')
    args = parser.parse_args()
    if not args.path.is_absolute():
        raise ValueError('Lock fixture path must be absolute')
    os.environ['NUMERA_LOCK_TIMEOUT'] = '0'
    lock = (transaction_lock(args.path) if args.kind == 'root' else
            _lock_for(args.path) if args.kind == 'file' else
            session_lease(args.path, 'fixture-profile'))
    with lock:
        print('acquired', flush=True)
        if args.hold:
            # The parent owns and bounds this child, signals release through
            # stdin and kills/reaps on any failure; never a blind timed sleep.
            if sys.stdin.readline().strip() != 'release':
                raise RuntimeError('Lock fixture did not receive a release signal')


if __name__ == '__main__':
    main()
