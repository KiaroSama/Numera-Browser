"""Run one complete discovered Windows partition with module-local diagnostics."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import sys
import time

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts/migration'))
from run_logging import run_log
from run_owned import run
from suites import python_sources


def announce(sources: list[Path], index: int, logger) -> bool:
    width = max(1, (len(sources) + 9) // 10)
    if index % width:
        return False
    names = [source.name for source in sources[index:index + width]]
    if any(not re.fullmatch(r'test_[A-Za-z0-9_]+\.py', name) for name in names):
        raise ValueError('Source diagnostic module name is invalid')
    # The runner recognizes this even through the outer sanitized log prefix.
    # Ten small timeline notices survive independently of final log upload.
    logger.info('##[notice]Source block starting modules=%s', ','.join(names))
    return True


def record(output: Path, source: Path, phase: str, **detail) -> None:
    with (output / 'progress.jsonl').open('a', encoding='utf-8', newline='\n') as stream:
        stream.write(json.dumps(dict(module=source.name, phase=phase, **detail)) + '\n')
        stream.flush()
        os.fsync(stream.fileno())


def verify(shard: int, shards: int, logger) -> int:
    sources = python_sources(shard, shards)
    output = ROOT / 'build' / f'source-windows-{shard}'
    output.mkdir(parents=True, exist_ok=False)
    end = time.monotonic() + 600
    failed = []
    os.chdir(ROOT / 'engine')
    for index, source in enumerate(sources):
        remaining = end - time.monotonic()
        if remaining <= 0:
            raise TimeoutError('Source partition exhausted its 600 second budget')
        announce(sources, index, logger)
        record(output, source, 'start')
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
                record(output, source, 'unproved', error=type(error).__name__)
                logger.error('Source module failed name=%s type=%s', source.name, type(error).__name__)
                raise  # Unproved process cleanup cannot authorize another module.
        record(output, source, 'finish', exit=code)
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
