"""Alternating foreground CLI measurements; incomplete evidence never selects Rust."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import sys
import tempfile
import zipfile

from candidate_stage import artifact
from replay_rollback import exact_json, protected_bytes
from run_logging import run_log

ROOT = Path(__file__).resolve().parents[2]


def execute(argv, env, *, timeout=15):
    from candidate_acceptance import owned
    with owned(argv, env, ROOT) as process:
        stdout, stderr = process.communicate(timeout=timeout)
        return process.returncode, stdout.decode('utf-8'), stderr.decode('utf-8')


REPETITIONS = 7
SCENARIOS = ('create', 'list', 'vault-read', 'backup')


def verdict(native, original):
    if any(not rows or any(type(value) is not int or value <= 0 for value in rows)
           for rows in (native, original)):
        raise ValueError('Measurements must contain positive integer samples')
    if max(native) < min(original):
        return 'no-regression'
    if min(native) > max(original):
        return 'regression'
    return 'inconclusive'


def baseline(parent, env):
    root = parent / 'baseline'
    code, stdout, stderr = execute(
        [sys.executable, '-m', 'numera', '--data-dir', str(root), 'create', 'Benchmark fixture',
         '--seed', '42', '--engine', 'playwright', '--proxy',
         'http://fixture:synthetic-benchmark-value@127.0.0.1:8080', '--json'], env)
    if code or stderr:
        raise ValueError('Original benchmark baseline creation failed')
    profile = json.loads(stdout)['profile']
    identifier = profile['id']
    for directory in ('profiles', 'dashboard'):
        record = root / directory / (identifier + '.json')
        raw = json.loads(record.read_text(encoding='utf-8'))
        for index in range(32):
            name = f'{index:012x}'
            value = {**raw, 'id': name, 'name': f'Benchmark {index:02d}'}
            (record.parent / (name + '.json')).write_text(json.dumps(value), encoding='utf-8')
        if identifier not in {f'{index:012x}' for index in range(32)}:
            record.unlink()
    marker = root / 'user-data/000000000000/Default/synthetic.bin'
    marker.parent.mkdir(parents=True)
    marker.write_bytes(b'\x00\xffbenchmark session bytes\r\n')
    locked = parent / 'vault baseline'
    shutil.copytree(root, locked)
    password = 'synthetic-benchmark-master'
    code, stdout, stderr = execute(
        [sys.executable, '-m', 'numera', '--data-dir', str(locked), 'vault', 'enable', '--json'],
        {**env, 'NUMERA_PASSWORD': password})
    if code or stderr or json.loads(stdout) != {'ok': True, 'enabled': True, 'encrypted': 64}:
        raise ValueError('Original encrypted benchmark baseline failed')
    return root, locked, password


def observation(candidate, original, parent, template, env, runtime, scenario, password):
    with tempfile.TemporaryDirectory(prefix='pair-', dir=parent) as temporary:
        workspace = Path(temporary)
        root = workspace / 'data with spaces'
        shutil.copytree(template, root)
        before = protected_bytes(root)
        destination = workspace / 'backup.zip'
        commands = {'create': ['create', 'Measured creation', '--seed', '43', '--engine', 'playwright'],
                    'list': ['list'], 'vault-read': ['show', '000000000000'],
                    'backup': ['backup', '000000000000', str(destination)]}
        environment = dict(env)
        if scenario == 'vault-read':
            environment['NUMERA_PASSWORD'] = password
        code, stdout, stderr = execute(
            [sys.executable, '-I', str(ROOT / 'scripts/migration/benchmark_sample.py'),
             '--candidate', str(candidate), '--original', str(original), '--runtime', runtime,
             '--', '--data-dir', str(root), *commands[scenario], '--json'], environment, timeout=25)
        if code or len(stdout.encode('utf-8')) > 131072:
            raise ValueError(f'Benchmark observation refused runtime={runtime} scenario={scenario} exit={code}')
        sample = json.loads(stdout)
        payload = sample.pop('payload')
        if scenario == 'create':
            if payload['ok'] is not True:
                raise ValueError('Measured creation did not succeed')
            profile = payload['profile']
            stored = json.loads((root / 'profiles' / (profile['id'] + '.json')).read_text(encoding='utf-8'))
            if exact_json(stored) != exact_json(profile):
                raise ValueError('Measured creation did not persist its returned profile')
            after = protected_bytes(root)
            for relative in (f'profiles/{profile["id"]}.json', f'dashboard/{profile["id"]}.json'):
                if relative in before or relative not in after:
                    raise ValueError('Measured creation overwrote or omitted a profile copy')
                del after[relative]
            from numera.data_locks import _lock_path
            for directory in ('profiles', 'dashboard'):
                record = (root / directory / (profile['id'] + '.json')).resolve()
                relative = (record.parent / '.locks' / _lock_path(str(record), record.name)).relative_to(root.resolve()).as_posix()
                if relative not in before and after.get(relative) == b'':
                    del after[relative]
            if after != before:
                raise ValueError('Measured creation changed an unrelated protected record')
            profile = {key: value for key, value in profile.items()
                       if key not in ('id', 'created_at', 'updated_at')}
            dashboard = json.loads((root / 'dashboard' / (stored['id'] + '.json')).read_text(encoding='utf-8'))
            if dashboard.get('id') != stored['id']:
                raise ValueError('Measured creation dashboard identity differs')
            dashboard = {key: value for key, value in dashboard.items()
                         if key not in ('id', '_created')}
            payload = {**payload, 'profile': profile, 'dashboard': dashboard}
        else:
            if protected_bytes(root) != before:
                raise ValueError('Read-only benchmark changed protected input bytes')
        if scenario == 'backup':
            if payload != {'ok': True, 'id': '000000000000', 'name': 'Benchmark 00', 'file': str(destination)}:
                raise ValueError('Measured backup returned an invalid verdict')
            with zipfile.ZipFile(destination) as archive:
                members = {item.filename: archive.read(item) for item in archive.infolist() if not item.is_dir()}
            profile = json.loads(members.pop('profile.json'))
            if members != {'user-data/Default/synthetic.bin': b'\x00\xffbenchmark session bytes\r\n'}:
                raise ValueError('Measured backup lost or added session data')
            payload = profile
        return sample, exact_json(payload)


def run(report, destination, logger):
    from baseline_source import source_root
    original = source_root()
    if (os.environ.get('GITHUB_ACTIONS') != 'true'
            or os.environ.get('GITHUB_REPOSITORY') != 'KiaroSama/Numera-Browser'
            or sys.platform not in ('win32', 'linux')):
        raise ValueError('Performance verification requires its owning hosted Windows/Linux job')
    candidate = artifact(report, release=True)
    code, stdout, stderr = execute([str(candidate), '--build-info'], os.environ.copy())
    identity = json.loads(stdout) if code == 0 and not stderr else {}
    if (identity.get('source_sha') != os.environ['GITHUB_SHA']
            or identity.get('generation') != int(os.environ['GITHUB_RUN_ID'])
            or identity.get('application_runtime') != 'native-candidate'):
        raise ValueError('Optimized benchmark candidate identity differs from this verification batch')
    results = []
    with tempfile.TemporaryDirectory(prefix='benchmark-', dir=ROOT / '.ci-work/migration-temp') as temporary:
        parent = Path(temporary)
        home = parent / 'home'
        home.mkdir()
        env = {key: value for key, value in os.environ.items()
               if not key.startswith('NUMERA_') and key not in ('PYTHONHOME', 'PYTHONPATH')}
        env.update(PYTHONPATH=str(original / 'engine'), PYTHONUTF8='1', PYTHONIOENCODING='utf-8',
                   PYTHONDONTWRITEBYTECODE='1', HOME=str(home), USERPROFILE=str(home),
                   NUMERA_APP_DIR=str(parent), NUMERA_LOG_DIR=str(parent / 'logs'),
                   NUMERA_BROWSER_ADAPTER=str(parent / 'absent.py'), NUMERA_SDK_PYTHON=sys.executable)
        if os.environ.get('NUMERA_HELPER_UNITS'):
            env['NUMERA_HELPER_UNITS'] = os.environ['NUMERA_HELPER_UNITS']
        plain, encrypted, password = baseline(parent, env)
        for scenario in SCENARIOS:
            samples = []
            for index in range(-1, REPETITIONS):
                order = ('original', 'native') if index % 2 == 0 else ('native', 'original')
                observed = {}
                expected = None
                for runtime in order:
                    sample, payload = observation(candidate, original, parent, encrypted if scenario == 'vault-read' else plain,
                                                  env, runtime, scenario, password)
                    if expected is not None and payload != expected:
                        raise ValueError(f'Benchmark output parity differs scenario={scenario}')
                    expected = payload
                    observed[runtime] = sample
                    logger.info('Measured scenario=%s runtime=%s repetition=%s elapsed_ns=%s peak_bytes=%s',
                                scenario, runtime, index, sample['elapsed_ns'], sample['memory']['bytes'])
                if index >= 0:
                    samples.append({'order': list(order), **observed})
            metrics = {row[runtime]['memory']['metric'] for row in samples for runtime in ('original', 'native')}
            if len(metrics) != 1:
                raise ValueError('A benchmark comparison mixed incompatible memory metrics')
            time_verdict = verdict([row['native']['elapsed_ns'] for row in samples],
                                   [row['original']['elapsed_ns'] for row in samples])
            memory_verdict = verdict([row['native']['memory']['bytes'] for row in samples],
                                     [row['original']['memory']['bytes'] for row in samples])
            results.append({'scenario': scenario, 'time': time_verdict, 'memory': memory_verdict,
                            'samples': samples})
    evidence = {'schema': 1, 'source_sha': identity['source_sha'], 'generation': identity['generation'],
                'profile': 'release-opt3', 'binary_sha256': hashlib.sha256(candidate.read_bytes()).hexdigest(),
                'platform': sys.platform, 'machine': platform.machine(), 'python': platform.python_version(),
                'repetitions': REPETITIONS, 'warmup_pairs': 1, 'results': results,
                'scope': 'foreground-cli-only; not API/browser or full migration acceptance'}
    destination.parent.mkdir(parents=True, exist_ok=True)
    with destination.open('x', encoding='utf-8') as stream:
        stream.write(json.dumps(evidence, indent=2, allow_nan=False) + '\n')
    for result in results:
        logger.info('Performance verdict scenario=%s time=%s memory=%s', result['scenario'], result['time'], result['memory'])
    if any(row[key] != 'no-regression' for row in results for key in ('time', 'memory')):
        raise ValueError('Performance acceptance blocked: regression or inconclusive measurements; evidence retained')
    return 0


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--report', type=Path, required=True)
    parser.add_argument('--destination', type=Path, required=True)
    args = parser.parse_args()
    with run_log('native-performance') as log:
        raise SystemExit(run(args.report, args.destination, log))
