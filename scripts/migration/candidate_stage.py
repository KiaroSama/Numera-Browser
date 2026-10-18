"""Stage only reviewed native candidate runtime inputs; never a published product."""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import shutil

ROOT = Path(__file__).resolve().parents[2]
BINARY = 'numera-native-candidate'
ADAPTERS = ('adapter.py', 'frame_input.py', 'hints.py')
NOTICES = ('LICENSE', 'docs/licensing.md', 'docs/browser-notices.txt',
           'docs/migration-random-notices.txt',
           'engine-rust/src/http/documentation-assets/NOTICE.md')


def executable_name() -> str:
    return BINARY + ('.exe' if os.name == 'nt' else '')


def artifact(report: Path, *, release: bool = False) -> Path:
    if report.stat().st_size > 16 * 1024 * 1024:
        raise ValueError('Cargo artifact report exceeds its bound')
    records = [json.loads(line) for line in report.read_text(encoding='utf-8').splitlines() if line]
    finished = [row for row in records if row.get('reason') == 'build-finished']
    rows = [row for row in records if row.get('reason') == 'compiler-artifact'
            and row.get('target', {}).get('name') == BINARY
            and row.get('target', {}).get('kind') == ['bin']]
    if len(finished) != 1 or finished[0].get('success') is not True or len(rows) != 1:
        raise ValueError('Cargo must report one successful candidate build')
    row = rows[0]
    if (Path(row.get('manifest_path', '')).resolve() != (ROOT / 'engine-rust/Cargo.toml').resolve()
            or 'native-candidate' not in row.get('features', []) or row.get('profile', {}).get('test') is not False
            or not isinstance(row.get('executable'), str)):
        raise ValueError('Cargo candidate identity, features or profile differ')
    if release and (row['profile'].get('opt_level') != '3'
                    or row['profile'].get('debug_assertions') is not False):
        raise ValueError('Performance candidate must use the optimized release profile')
    path = Path(row['executable']).resolve()
    if not path.is_relative_to((ROOT / 'engine-rust/target').resolve()):
        raise ValueError('Cargo candidate artifact is outside its build root')
    return path


def files(directory: Path) -> dict:
    result = {}
    total = 0
    for path in sorted(directory.rglob('*')):
        if path.is_symlink() or getattr(path.lstat(), 'st_file_attributes', 0) & 0x400:
            raise ValueError('Candidate inputs must not contain links')
        if path.is_file() and path.relative_to(directory).as_posix() != 'CANDIDATE.json':
            name = path.relative_to(directory).as_posix()
            if not (name == executable_name() or name in NOTICES
                    or name in {'scripts/browser-adapter/' + name for name in ADAPTERS}
                    or name == 'web/index.html' or name.startswith('web/assets/')):
                raise ValueError('Candidate contains an unapproved runtime file')
            total += path.stat().st_size
            if len(result) >= 2048 or total > 512 * 1024 * 1024:
                raise ValueError('Candidate readback exceeds staging bounds')
            digest = hashlib.sha256()
            with path.open('rb') as stream:
                for block in iter(lambda: stream.read(1_048_576), b''):
                    digest.update(block)
            result[name] = {'size': path.stat().st_size, 'sha256': digest.hexdigest()}
    return result


def stage(binary: Path, web: Path, destination: Path, identity: dict) -> None:
    if not (web / 'index.html').is_file() or not (web / 'assets').is_dir():
        raise ValueError('Candidate dashboard requires its index and assets')
    inputs = {executable_name(): binary}
    inputs.update({'scripts/browser-adapter/' + name: ROOT / 'scripts/browser-adapter' / name
                   for name in ADAPTERS})
    inputs.update({name: ROOT / name for name in NOTICES})
    inputs['web/index.html'] = web / 'index.html'
    for path in (web / 'assets').rglob('*'):
        if path.is_symlink() or getattr(path.lstat(), 'st_file_attributes', 0) & 0x400:
            raise ValueError('Candidate dashboard contains linked inputs')
        if path.is_file():
            inputs['web/' + path.relative_to(web).as_posix()] = path
    if len(inputs) > 2048 or sum(path.stat().st_size for path in inputs.values()) > 512 * 1024 * 1024:
        raise ValueError('Candidate runtime inputs exceed staging bounds')
    destination.mkdir()  # Never merge with or replace an existing installation.
    for name, source in inputs.items():
        if source.is_symlink() or not source.is_file() or getattr(source.lstat(), 'st_file_attributes', 0) & 0x400:
            raise ValueError('Candidate input is missing, linked or not regular')
        output = destination / name
        output.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, output)
        if source.read_bytes() != output.read_bytes():
            raise ValueError('Candidate staged input readback differs')
    record = {'schema': 1, 'identity': identity, 'external_sdk_python': True, 'files': files(destination)}
    (destination / 'CANDIDATE.json').write_text(json.dumps(record, sort_keys=True) + '\n', encoding='utf-8')
    verify(destination)


def build(report: Path, logger, *, release: bool = False) -> Path:
    import logging
    from run_owned import run

    records = []

    class Artifacts(logging.Handler):
        def emit(self, record):
            text = record.getMessage()
            if not text.startswith('{'):
                return
            try:
                value = json.loads(text)
            except ValueError:
                return
            if (value.get('reason') == 'build-finished' or
                    (value.get('reason') == 'compiler-artifact' and
                     value.get('target', {}).get('name') == BINARY)):
                if len(records) >= 8:
                    raise ValueError('Unexpected duplicate candidate build records')
                records.append(value)

    capture = Artifacts()
    logger.addHandler(capture)
    try:
        command = ['cargo', 'build', '--manifest-path', str(ROOT / 'engine-rust/Cargo.toml'),
                   '--locked', '--features', 'native-candidate', '--bin', BINARY,
                   '--message-format=json']
        if release:
            command.append('--release')
        code = run(command, 900, 300, logger)
        if code:
            raise ValueError('Native candidate build failed')
    finally:
        logger.removeHandler(capture)
        capture.close()
    report.parent.mkdir(parents=True, exist_ok=True)
    with report.open('x', encoding='utf-8', newline='\n') as stream:
        stream.write(''.join(json.dumps(row) + '\n' for row in records))
    return artifact(report, release=release)


def verify(directory: Path) -> dict:
    record = json.loads((directory / 'CANDIDATE.json').read_text(encoding='utf-8'))
    actual = files(directory)
    required = {executable_name(), 'web/index.html', *NOTICES,
                *('scripts/browser-adapter/' + name for name in ADAPTERS)}
    if (record.get('schema') != 1 or record.get('external_sdk_python') is not True
            or not required <= actual.keys() or not any(name.startswith('web/assets/') for name in actual)
            or record.get('files') != actual):
        raise ValueError('Candidate runtime inventory or hash readback differs')
    return record['identity']


if __name__ == '__main__':
    import argparse
    from run_logging import run_log

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--report', type=Path, required=True)
    parser.add_argument('--release', action='store_true', help='Require optimized performance inputs')
    args = parser.parse_args()
    with run_log('native-candidate-build') as logger:
        path = build(args.report, logger, release=args.release)
        logger.info('Current candidate artifact observed name=%s', path.name)
