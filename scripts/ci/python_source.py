"""Run one complete discovered Windows partition with module-local diagnostics."""
from __future__ import annotations

import argparse
import os
from pathlib import Path
import sys
import time

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts/migration'))
from run_logging import run_log
from run_owned import run
from suites import python_sources


def verify(shard: int, shards: int, logger) -> int:
    sources = python_sources(shard, shards)
    output = ROOT / 'build' / f'source-windows-{shard}'
    output.mkdir(parents=True, exist_ok=False)
    end = time.monotonic() + 600
    failed = []
    os.chdir(ROOT / 'engine')
    for source in sources:
        remaining = end - time.monotonic()
        if remaining <= 0:
            raise TimeoutError('Source partition exhausted its 600 second budget')
        logger.info('Source module starting name=%s remaining=%.1fs', source.name, remaining)
        command = [sys.executable, '-m', 'pytest', '-vv', '-s',
                   '-o', 'faulthandler_timeout=45',
                   '--ignore-glob=tests/test_*_live.py',
                   '--junitxml=' + str(output / (source.stem + '.xml')),
                   '--durations=3', str(source)]
        with run_log('source-' + source.stem, output) as module_log:
            try:
                code = run(command, remaining, min(120, remaining), module_log)
            except Exception as error:
                logger.error('Source module failed name=%s type=%s', source.name, type(error).__name__)
                raise  # Unproved process cleanup cannot authorize another module.
        logger.info('Source module finished name=%s exit=%s', source.name, code)
        if code:
            failed.append(source.name)
    logger.info('Source partition finished modules=%s failed=%s', len(sources), failed)
    return int(bool(failed))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--shard', type=int, required=True)
    parser.add_argument('--shards', type=int, required=True)
    args = parser.parse_args()
    with run_log('source-partition') as logger:
        return verify(args.shard, args.shards, logger)


if __name__ == '__main__':
    raise SystemExit(main())
