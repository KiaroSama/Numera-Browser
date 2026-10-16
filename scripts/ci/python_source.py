"""Run one complete discovered Windows partition with module-local diagnostics."""
from __future__ import annotations

import argparse
import json
import math
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


def announce(sources: list[Path], index: int, logger, *, detail: str = '') -> bool:
    width = max(1, (len(sources) + 9) // 10)
    if index % width:
        return False
    names = [source.name for source in sources[index:index + width]]
    if any(not re.fullmatch(r'test_[A-Za-z0-9_]+\.py', name) for name in names):
        raise ValueError('Source diagnostic module name is invalid')
    # The runner recognizes this even through the outer sanitized log prefix.
    # Ten small timeline notices survive independently of final log upload.
    logger.info('##[notice]Source block starting modules=%s%s', ','.join(names), detail)
    return True


def record(output: Path, source: Path, phase: str, **detail) -> None:
    with (output / 'progress.jsonl').open('a', encoding='utf-8', newline='\n') as stream:
        stream.write(json.dumps(dict(module=source.name, phase=phase, **detail)) + '\n')
        stream.flush()
        os.fsync(stream.fileno())


def verify(shard: int, shards: int, logger, *, block: int = 0, blocks: int = 1) -> int:
    sources = python_sources(shard, shards)
    if not 1 <= blocks <= 4 or not 0 <= block < blocks or len(sources) < blocks:
        raise ValueError('Invalid source checkpoint block')
    output = ROOT / 'build' / f'source-windows-{shard}'
    checkpoint = output / 'checkpoint.json'
    if block == 0:
        output.mkdir(parents=True, exist_ok=False)
        state = dict(end=time.monotonic() + 600, next=0, blocks=blocks,
                     modules=[source.name for source in sources])
    else:
        state = json.loads(checkpoint.read_text(encoding='utf-8'))
        if (state.get('next') != block or state.get('blocks') != blocks
                or state.get('modules') != [source.name for source in sources]):
            raise ValueError('Source checkpoint is incomplete or belongs to another partition')
    end = state['end']
    if not isinstance(end, (int, float)) or not math.isfinite(end):
        raise ValueError('Source checkpoint deadline is invalid')
    failed = []
    os.chdir(ROOT / 'engine')
    first, last = len(sources) * block // blocks, len(sources) * (block + 1) // blocks
    # The failing step loses its final logs. Use its ten timeline slots for
    # nine module starts and one outcome; other steps retain grouped notices.
    precise = shard == 1 and shards == 4 and blocks == 4 and block == 2 and last - first <= 9
    settled = []
    for index in range(first, last):
        source = sources[index]
        remaining = end - time.monotonic()
        if remaining <= 0:
            raise TimeoutError('Source partition exhausted its 600 second budget')
        if precise:
            announce(sources[first:last], index - first, logger,
                     detail=f' remaining={remaining:.1f}s settled={",".join(settled) or "none"}')
        else:
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
                if precise:
                    logger.info('##[notice]Source module unproved name=%s type=%s settled=%s',
                                source.name, type(error).__name__, ','.join(settled) or 'none')
                record(output, source, 'unproved', error=type(error).__name__)
                logger.error('Source module failed name=%s type=%s', source.name, type(error).__name__)
                raise  # Unproved process cleanup cannot authorize another module.
        settled.append(f'{source.name}:{code}')
        record(output, source, 'finish', exit=code)
        logger.info('Source module finished name=%s exit=%s', source.name, code)
        if code:
            failed.append(source.name)
    if precise:
        logger.info('##[notice]Source block settled modules=%s', ','.join(settled))
    state['next'] = block + 1
    with checkpoint.open('w', encoding='utf-8', newline='\n') as stream:
        stream.write(json.dumps(state) + '\n')
        stream.flush()
        os.fsync(stream.fileno())
    logger.info('Source block finished block=%s modules=%s failed=%s', block, last - first, failed)
    return int(bool(failed))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--shard', type=int, required=True)
    parser.add_argument('--shards', type=int, required=True)
    parser.add_argument('--block', type=int, default=0)
    parser.add_argument('--blocks', type=int, default=1)
    args = parser.parse_args()
    with run_log('source-partition') as logger:
        return verify(args.shard, args.shards, logger, block=args.block, blocks=args.blocks)


if __name__ == '__main__':
    raise SystemExit(main())
