"""Compare real persistent-cookie export commands on the selected hosted browser."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import sys
import time

from benchmark import REPETITIONS, execute, fixture_directory, verdict
from candidate_acceptance import bind_sdk_cache, failure_diagnostics
from candidate_stage import artifact
from replay_rollback import exact_json
from run_logging import run_log

ROOT = Path(__file__).resolve().parents[2]
SENTINEL = b'\x00\xffretained session\r\n'


def environment(original, parent, engine):
    home = parent / 'synthetic home'
    home.mkdir()
    env = {key: value for key, value in os.environ.items()
           if not key.startswith('NUMERA_') and key not in ('PYTHONHOME', 'PYTHONPATH')}
    env['XDG_CACHE_HOME'] = os.environ.get('XDG_CACHE_HOME') or str(Path.home() / '.cache')
    env['CLOAKBROWSER_CACHE_DIR'] = os.environ.get('CLOAKBROWSER_CACHE_DIR') or str(Path.home() / '.cloakbrowser')
    bind_sdk_cache(env, engine)
    env.update(PYTHONPATH=str(original / 'engine'), PYTHONUTF8='1', PYTHONIOENCODING='utf-8',
               PYTHONDONTWRITEBYTECODE='1', HOME=str(home), USERPROFILE=str(home),
               NUMERA_APP_DIR=str(parent), NUMERA_LOG_DIR=str(parent / 'logs'),
               NUMERA_BROWSER_ADAPTER=str(ROOT / 'scripts/browser-adapter/adapter.py'),
               NUMERA_SDK_PYTHON=sys.executable)
    if os.environ.get('NUMERA_HELPER_UNITS'):
        env['NUMERA_HELPER_UNITS'] = os.environ['NUMERA_HELPER_UNITS']
    return env


def baseline(parent, env, engine):
    root = parent / 'baseline'

    def command(*arguments, browser=False):
        code, stdout, stderr = execute(
            [sys.executable, '-m', 'numera', '--data-dir', str(root), *arguments, '--json'],
            env, timeout=100 if browser else 15)
        if code or stderr:
            kinds = {'ValueError', 'RuntimeError', 'OSError', 'FileNotFoundError', 'PermissionError',
                     'TimeoutError', 'Error', 'TargetClosedError', 'ImportError', 'ModuleNotFoundError'}
            kind = 'unknown'
            for line in stderr[:65536].splitlines():
                match = re.fullmatch(r'([A-Za-z_][A-Za-z_0-9]*):.*', line)
                if match and match[1] in kinds:
                    kind = match[1]
            operation = 'cookies-' + arguments[1] if arguments[0] == 'cookies' else 'create'
            raise ValueError('Original browser benchmark baseline command failed '
                             f'operation={operation} exit={code} type={kind} '
                             f'stdout_bytes={len(stdout.encode("utf-8"))} stderr_bytes={len(stderr.encode("utf-8"))}')
        return json.loads(stdout)

    created = command('create', 'Browser benchmark fixture', '--seed', '42', '--engine', engine)
    identifier = created['profile']['id']
    if not isinstance(identifier, str) or len(identifier) != 12 or any(c not in '0123456789abcdef' for c in identifier):
        raise ValueError('Original browser benchmark profile identity is invalid')
    cookie = {'name': 'persistent-fixture', 'value': 'synthetic', 'domain': '.fixture.test',
              'path': '/', 'expires': int(time.time()) + 86400, 'secure': False,
              'httpOnly': True, 'sameSite': 'Lax'}
    source = parent / 'cookie input.json'
    cookies = [cookie, {**cookie, 'name': 'second-fixture', 'httpOnly': False}]
    source.write_text(json.dumps({'cookies': cookies}), encoding='utf-8')
    if command('cookies', 'import', identifier, str(source), '--clear', browser=True) != {
            'ok': True, 'imported': 2, 'cleared': True}:
        raise ValueError('Original browser benchmark cookie seeding failed')
    destination = parent / 'baseline cookies.json'
    result = command('cookies', 'export', identifier, str(destination), '--format', 'json', browser=True)
    if result != {'ok': True, 'count': 2, 'file': str(destination), 'format': 'json'}:
        raise ValueError('Original browser benchmark export metadata differs')
    payload = json.loads(destination.read_text(encoding='utf-8'))
    actual = payload.get('cookies')
    if (not isinstance(actual, list) or len(actual) != len(cookies)
            or sorted(actual, key=lambda row: row['name']) != sorted(cookies, key=lambda row: row['name'])
            or payload.get('origins') != []):
        raise ValueError('Original browser benchmark cookie persistence differs')
    marker = root / 'user-data' / identifier / 'sentinel.bin'
    marker.write_bytes(SENTINEL)
    return root, identifier, payload


def metadata_bytes(root):
    result = {}
    for name in ('config.json', 'dashboard-config.json', 'proxies.json', 'profiles', 'dashboard'):
        path = root / name
        paths = [path]
        for item in paths:
            if item.is_symlink() or (item.exists() and getattr(item.lstat(), 'st_file_attributes', 0) & 0x400):
                raise ValueError('Browser benchmark protected metadata contains a link')
            if item.is_dir():
                paths.extend(child for child in item.iterdir() if child.name != '.locks')
            elif item.is_file():
                result[item.relative_to(root).as_posix()] = item.read_bytes()
    return result


def observation(candidate, original, parent, template, env, runtime, identifier):
    with fixture_directory('browser-pair-', parent, retain=True) as workspace:
        root = workspace / 'data with spaces'
        # Firefox lock links name host/PID ownership, not a file to dereference.
        shutil.copytree(template, root, symlinks=True)
        before = metadata_bytes(root)
        marker = root / 'user-data' / identifier / 'sentinel.bin'
        if marker.read_bytes() != SENTINEL:
            raise ValueError('Browser benchmark input session sentinel differs')
        destination = workspace / 'cookies.json'
        code, stdout, stderr = execute(
            [sys.executable, '-I', str(ROOT / 'scripts/migration/benchmark_sample.py'),
             '--candidate', str(candidate), '--original', str(original), '--runtime', runtime,
             '--wall', '90', '--nested', '--', '--data-dir', str(root),
             'cookies', 'export', identifier, str(destination), '--format', 'json', '--json'],
            env, timeout=100)
        if code or len(stdout.encode('utf-8')) > 131072:
            raise ValueError(f'Browser benchmark observation refused runtime={runtime} exit={code}')
        sample = json.loads(stdout)
        payload = sample.pop('payload')
        if exact_json(payload) != exact_json({'ok': True, 'count': 2, 'file': str(destination), 'format': 'json'}):
            raise ValueError('Browser benchmark export metadata differs')
        if metadata_bytes(root) != before:
            raise ValueError('Browser benchmark changed protected profile or configuration bytes')
        if marker.read_bytes() != SENTINEL:
            raise ValueError('Browser benchmark changed its session sentinel')
        if any((root / 'sessions').glob('*.json')):
            raise ValueError('Browser benchmark retained a session claim after completion')
        if destination.stat().st_size > 131072:
            raise ValueError('Browser benchmark cookie export exceeds its bound')
        return sample, exact_json(json.loads(destination.read_text(encoding='utf-8')))


def run(report, destination, engine, logger):
    from baseline_source import source_root

    if (os.environ.get('GITHUB_ACTIONS') != 'true'
            or os.environ.get('GITHUB_REPOSITORY') != 'KiaroSama/Numera-Browser'
            or sys.platform not in ('win32', 'linux')
            or engine not in ('playwright', 'patchright', 'camoufox', 'cloak')):
        raise ValueError('Browser measurements require the owning hosted Windows/Linux engine job')
    candidate, original = artifact(report, release=True), source_root()
    code, stdout, stderr = execute([str(candidate), '--build-info'], os.environ.copy())
    identity = json.loads(stdout) if code == 0 and not stderr else {}
    if (identity.get('source_sha') != os.environ['GITHUB_SHA']
            or identity.get('generation') != int(os.environ['GITHUB_RUN_ID'])
            or identity.get('application_runtime') != 'native-candidate'):
        raise ValueError('Browser benchmark candidate identity differs from this batch')
    temporary = ROOT / '.ci-work/migration-temp'
    temporary.mkdir(parents=True, exist_ok=True)
    with fixture_directory('browser-benchmark-', temporary) as parent:
        env = environment(original, parent, engine)
        template, identifier, baseline_payload = baseline(parent, env, engine)
        baseline_payload = exact_json(baseline_payload)
        samples = []
        for index in range(-1, REPETITIONS):
            order = ('original', 'native') if index % 2 == 0 else ('native', 'original')
            observed = {}
            for runtime in order:
                sample, payload = observation(candidate, original, parent, template, env, runtime, identifier)
                if payload != baseline_payload:
                    raise ValueError('Browser benchmark cookie export parity differs')
                observed[runtime] = sample
                logger.info('Measured engine=%s runtime=%s repetition=%s elapsed_ns=%s peak_bytes=%s',
                            engine, runtime, index, sample['elapsed_ns'], sample['memory']['bytes'])
            if index >= 0:
                samples.append({'order': list(order), **observed})
        metrics = {row[runtime]['memory']['metric'] for row in samples for runtime in ('original', 'native')}
        if len(metrics) != 1:
            raise ValueError('Browser benchmark mixed incompatible memory metrics')
        result = {'scenario': 'browser-cookie-export', 'samples': samples,
                  'time': verdict([row['native']['elapsed_ns'] for row in samples],
                                  [row['original']['elapsed_ns'] for row in samples]),
                  'memory': verdict([row['native']['memory']['bytes'] for row in samples],
                                    [row['original']['memory']['bytes'] for row in samples]),
                  'latency_scope': 'foreground command through natural SDK settlement',
                  'memory_scope': 'lifetime OS peak; Linux largest waited child, not concurrent tree RSS'}
        evidence = {'schema': 1, 'source_sha': identity['source_sha'], 'generation': identity['generation'],
                    'profile': 'release-opt3', 'binary_sha256': hashlib.sha256(candidate.read_bytes()).hexdigest(),
                    'platform': sys.platform, 'machine': platform.machine(), 'python': platform.python_version(),
                    'engine': engine, 'repetitions': REPETITIONS, 'warmup_pairs': 1, 'results': [result],
                    'scope': 'real persistent cookie export; not every browser or migration capability'}
        destination.parent.mkdir(parents=True, exist_ok=True)
        with destination.open('x', encoding='utf-8') as stream:
            stream.write(json.dumps(evidence, indent=2, allow_nan=False) + '\n')
        logger.info('Browser performance verdict engine=%s time=%s memory=%s', engine, result['time'], result['memory'])
        if any(result[key] != 'no-regression' for key in ('time', 'memory')):
            failure_diagnostics(parent / 'logs', logger)
            raise ValueError('Browser performance blocked: regression or inconclusive measurements; inputs retained')
    return 0


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--report', type=Path, required=True)
    parser.add_argument('--destination', type=Path, required=True)
    parser.add_argument('--engine', choices=('playwright', 'patchright', 'camoufox', 'cloak'), required=True)
    args = parser.parse_args()
    with run_log('browser-performance') as logger:
        raise SystemExit(run(args.report, args.destination, args.engine, logger))
