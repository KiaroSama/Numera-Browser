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


def save_checkpoint(path: Path, state: dict) -> None:
    with path.open('w', encoding='utf-8', newline='\n') as stream:
        stream.write(json.dumps(state) + '\n')
        stream.flush()
        os.fsync(stream.fileno())


def module_cursor(state: dict, sources: list[Path], index: int, *, settled: bool = False) -> list:
    first, last = len(sources) * 2 // 4, len(sources) * 3 // 4
    cursor = index + int(settled)
    results = state.get('module_results', [])
    if (last - first != 9 or not 0 <= index < 9 or state.get('blocks') != 4
            or state.get('modules') != [source.name for source in sources]
            or state.get('next') != (3 if cursor == 9 else 2)
            or 'active' in state or state.get('module_next', 0) != cursor
            or not isinstance(results, list) or len(results) != cursor
            or any(not isinstance(row, dict) or row.get('module') != sources[first + offset].name
                   or type(row.get('exit')) is not int or row['exit'] not in (0, 1)
                   for offset, row in enumerate(results))):
        raise ValueError('Source module checkpoint is incomplete, repeated or belongs to another partition')
    return results


def settled_exit(index: int) -> int:
    state = json.loads((ROOT / 'build/source-windows-1/checkpoint.json').read_text(encoding='utf-8'))
    return module_cursor(state, python_sources(1, 4), index, settled=True)[-1]['exit']


def verify(shard: int, shards: int, logger, *, block: int = 0, blocks: int = 1,
           module_index: int | None = None) -> int:
    sources = python_sources(shard, shards)
    if module_index is not None and (shard, shards, block, blocks) != (1, 4, 2, 4):
        raise ValueError('Single module checkpoint requires Windows partition 1 block 2')
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
    if module_index is not None:
        module_cursor(state, sources, module_index)
        first += module_index
        last = first + 1
    elif state.get('next') == 2 and any(key in state for key in ('active', 'module_next', 'module_results')):
        raise ValueError('Source module checkpoint cannot be bypassed by a whole block')
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
        if module_index is not None:
            state['active'] = source.name
            save_checkpoint(checkpoint, state)
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
                if module_index is not None and (type(code) is not int or code not in (0, 1)):
                    raise RuntimeError('Source module did not finish with a complete test verdict')
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
    if module_index is not None:
        state.pop('active')
        state.setdefault('module_results', []).append(dict(module=source.name, exit=code))
        state['module_next'] = module_index + 1
        state['next'] = block + int(module_index == 8)
    save_checkpoint(checkpoint, state)
    logger.info('Source block finished block=%s modules=%s failed=%s', block, last - first, failed)
    # The composite propagates the stored verdict only after its outer owner
    # also proves cleanup. Protocol success is never the final test result.
    return 0 if module_index is not None else int(bool(failed))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--shard', type=int)
    parser.add_argument('--shards', type=int)
    parser.add_argument('--block', type=int, default=0)
    parser.add_argument('--blocks', type=int, default=1)
    parser.add_argument('--module-index', type=int)
    parser.add_argument('--settled-index', type=int)
    args = parser.parse_args()
    if args.settled_index is not None:
        print(settled_exit(args.settled_index))
        return 0
    if args.shard is None or args.shards is None:
        parser.error('--shard and --shards are required for execution')
    with run_log('source-partition') as logger:
        return verify(args.shard, args.shards, logger, block=args.block, blocks=args.blocks,
                      module_index=args.module_index)


if __name__ == '__main__':
    raise SystemExit(main())
