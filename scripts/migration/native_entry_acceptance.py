"""Compare the source and acceptance-only native entry against isolated stores."""
from __future__ import annotations
import argparse
import json
import os
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
from baseline_source import source_root
ORIGINAL = source_root()
sys.path.insert(0, str(ROOT / 'engine'))
from numera.proctree import spawn, end_tree
from numera.store import ProfileStore
from numera.models import Profile
from numera.fingerprint import generate
from run_logging import run_log
from replay_rollback import run as replay_rollback


def execute(argv, env, stdin=None):
    import subprocess
    process, tree = spawn(argv, env=env, stdin=subprocess.PIPE if stdin is not None else subprocess.DEVNULL,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    try:
        stdout, stderr = process.communicate(stdin, timeout=15)
        return process.returncode, stdout.decode('utf-8'), stderr.decode('utf-8')
    finally:
        if not end_tree(tree, grace=1, kill_grace=3):
            raise RuntimeError('Entry acceptance process tree did not settle')
        process.wait(timeout=5)


def configured_lock_acceptance(candidate, parent, environment, log):
    import subprocess
    import time
    from concurrent.futures import ThreadPoolExecutor, TimeoutError
    from replay_rollback import protected_bytes

    def snapshot(root):
        values = protected_bytes(root)
        for name in ('tokens.json', 'auth-policy.json'):
            path = root / name
            values[name] = path.read_bytes() if path.exists() else None
        return values

    fixture = ROOT / 'scripts/migration/lock_fixture.py'
    cases = [
        ('root', ['config']),
        ('dashboard-config.json', ['folder', 'create', 'After release']),
        ('config.json', ['config', '--headless-launch', 'on']),
        ('tokens.json', ['token', 'revoke', 'missing']),
    ]
    for index, (resource, arguments) in enumerate(cases):
        root = parent / f'configured CLI {index}'
        root.mkdir()
        for name, value in [('config.json', {'future': [True, 7]}),
                            ('dashboard-config.json', {'future': [True, 7], 'folders': []}),
                            ('tokens.json', []), ('proxies.json', [])]:
            (root / name).write_text(json.dumps(value), encoding='utf-8')
        before = snapshot(root)
        observed = []
        for runtime, executable in [('original', [sys.executable, '-m', 'numera']),
                                    ('native', [str(candidate)])]:
            ready = parent / f'configured-ready-{index}-{runtime}'
            command_env = dict(environment, PYTHONPATH=str(ORIGINAL / 'engine'),
                               NUMERA_LOCK_TIMEOUT='0' if index == 0 else '0.02')
            holder, tree = spawn(
                [sys.executable, '-B', str(fixture),
                 'root' if resource == 'root' else 'file',
                 str(root if resource == 'root' else root / resource),
                 '--hold', '--ready', str(ready)],
                env=command_env, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )
            try:
                deadline = time.monotonic() + 5
                while not ready.exists():
                    remaining = deadline - time.monotonic()
                    if remaining <= 0 or holder.poll() is not None:
                        raise AssertionError('Configured CLI lock owner did not become ready')
                    time.sleep(min(.01, remaining))
                assert ready.read_text(encoding='utf-8') == 'acquired'
                with ThreadPoolExecutor(max_workers=1) as worker:
                    future = worker.submit(execute,
                        [*executable, '--data-dir', str(root), *arguments, '--json'], command_env)
                    early = True
                    try:
                        result = future.result(timeout=3)
                    except TimeoutError:
                        early = False
                        result = None
                    finally:
                        live = holder.poll() is None
                        holder.stdin.write(b'release\n')
                        holder.stdin.close()
                    if result is None:
                        result = future.result(timeout=5)
                assert holder.wait(timeout=5) == 0, 'Configured CLI lock owner failed'
                code, stdout, stderr = result
                payload = json.loads(stdout)
                assert stderr == '' and code == 1, 'Configured CLI refusal envelope differs'
                assert early and live and payload.get('ok') is False, 'Configured CLI did not refuse while owned'
                assert set(payload) == {'ok', 'error'}
                message = payload['error']
                assert 'NUMERA_LOCK_TIMEOUT controls this wait' in message
                assert ('more than 0s.' if index == 0 else 'more than 0.02s.') in message
                observed.append(payload)
                assert snapshot(root) == before, 'Configured CLI refusal changed protected bytes'
                assert not (root / 'vault-migration.json').exists()
                assert not (root / '.updates/work').exists() or not any((root / '.updates/work').iterdir())
            finally:
                try:
                    if not end_tree(tree, grace=1, kill_grace=3):
                        raise RuntimeError('Configured CLI lock owner did not settle')
                    holder.wait(timeout=5)
                finally:
                    for stream in (holder.stdin, holder.stdout, holder.stderr):
                        if stream is not None:
                            stream.close()
            log.info('Configured CLI refusal matched runtime=%s case=%s', runtime, index)
        assert observed[0] == observed[1], 'Configured CLI complete refusal differs'
        # These success paths include admission release and nested/reentrant store locks.
        for executable in ([sys.executable, '-m', 'numera'], [str(candidate)]):
            code, stdout, stderr = execute(
                [*executable, '--data-dir', str(root), *arguments, '--json'], command_env)
            assert code == (1 if index == 3 else 0) and stderr == '', 'Released configured CLI operation failed'
            payload = json.loads(stdout)
            if index == 3:
                assert payload == {'ok': False, 'error': 'no token with that id'}
                assert snapshot(root) == before, 'Released missing-token refusal changed stored bytes'
            elif index == 1:
                assert payload == {'folders': ['After release'], 'created': 'After release'}
            elif index == 2:
                assert payload['headless_launch'] is True
                stored = json.loads((root / 'config.json').read_text(encoding='utf-8'))
                assert stored['headless_launch'] is True and stored['future'] == [True, 7]
        log.info('Configured CLI contention, protected bytes and post-release success passed case=%s', index)


def main(candidate, log):
    with tempfile.TemporaryDirectory(prefix='native-entry-', dir=ROOT / '.ci-work/migration-temp') as temporary:
        parent = Path(temporary)
        roots = [parent / 'source', parent / 'native']
        env = os.environ.copy()
        for key in ['NUMERA_PASSWORD', 'NUMERA_API_TOKEN', 'NUMERA_DATA_DIR', 'NUMERA_NATIVE_SUPERVISE',
                    'NUMERA_PARENT_PID', 'NUMERA_NATIVE_JOB_GATE', 'NUMERA_HELPER_UNITS', 'NUMERA_HELPER_SCOPE']:
            env.pop(key, None)
        home = parent / 'home'
        home.mkdir()
        env.update(PYTHONUTF8='1', PYTHONDONTWRITEBYTECODE='1', PYTHONPATH=str(ORIGINAL / 'engine'), HOME=str(home), USERPROFILE=str(home), NUMERA_APP_DIR=str(parent),
                   NUMERA_LOG_DIR=str(parent / 'logs'), NUMERA_BROWSER_ADAPTER=str(parent / 'absent.py'),
                   NUMERA_SDK_PYTHON=sys.executable)
        for root in roots:
            store = ProfileStore(root)
            store.save(Profile(id='fixture', name='Fixture', fingerprint=generate(seed=42),
                               created_at=1000.0, updated_at=1000.0))
            # Save refreshes time. Freeze only the fixture's persisted timestamp before either entry runs.
            record = root / 'profiles/fixture.json'
            value = json.loads(record.read_text(encoding='utf-8'))
            value['updated_at'] = 1000.0
            record.write_text(json.dumps(value), encoding='utf-8')
        commands = [['list'], ['show', 'fixture'], ['show', 'missing'], ['check', 'fixture'],
                    ['folder', 'list'], ['folder', 'create', 'Acceptance'], ['folder', 'list'],
                    ['pool', 'list'], ['jobs', 'list'], ['config'],
                    ['vault', 'enable'], ['vault', 'change'], ['delete', 'fixture'],
                    ['launch', 'missing', '--headless'], ['pool', 'add', '-']]
        for argv in commands:
            results = []
            for root, executable in zip(roots, [[sys.executable, '-m', 'numera'], [str(candidate)]]):
                code, stdout, stderr = execute([*executable, '--data-dir', str(root), *argv, '--json'], env)
                if not stdout.strip():
                    raise AssertionError(f'Entry produced no JSON: argv={argv!r} executable={executable[-1]!r} exit={code} stderr={stderr!r}')
                payload = json.loads(stdout)
                normalized = json.dumps(payload, ensure_ascii=False).replace(str(root).replace('\\', '\\\\'), '<root>')
                results.append((code, json.loads(normalized), stderr))
            if results[0] != results[1]:
                log.error('Entry mismatch command=%s source_exit=%s native_exit=%s', ' '.join(argv), results[0][0], results[1][0])
                # Fixtures contain only synthetic data; preserve bounded discrepancy diagnostics.
                raise AssertionError(f'{argv}: source={results[0]!r}, native={results[1]!r}')
            log.info('Entry matched command=%s exit=%s', ' '.join(argv), results[0][0])
        # Vault getpass may attach to the host console on Windows; its interactive
        # acceptance requires a hidden console, not a DEVNULL stdin approximation.
        for argv in [['list'], ['show', 'missing'], ['delete', 'fixture']]:
            results = [execute([*executable, '--data-dir', str(root), *argv], env, b'')
                       for root, executable in zip(roots, [[sys.executable, '-m', 'numera'], [str(candidate)]])]
            assert results[0] == results[1], f'Human entry mismatch {argv}: {results!r}'
            log.info('Human entry matched command=%s exit=%s', ' '.join(argv), results[0][0])
        messages = b'{"id":1,"method":"ping"}\n{"id":2,"method":"tools/call","params":{"name":"list"}}\n'
        results = []
        for root, executable in zip(roots, [[sys.executable, '-m', 'numera'], [str(candidate)]]):
            code, stdout, stderr = execute([*executable, '--data-dir', str(root), 'mcp'], env, messages)
            results.append((code, [json.loads(line) for line in stdout.splitlines()], stderr))
        if results[0] != results[1]:
            raise AssertionError(f'MCP entry mismatch: {results!r}')
        log.info('MCP ping and actual native child list matched; all owned trees settled')
        replay_rollback(candidate, ORIGINAL, parent, env, execute, log)
        configured_lock_acceptance(candidate, parent, env, log)
    return 0


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--candidate', type=Path, required=True)
    args = parser.parse_args()
    with run_log('native-entry-acceptance') as logger:
        raise SystemExit(main(args.candidate.resolve(strict=True), logger))
