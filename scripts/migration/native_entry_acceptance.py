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
    return 0


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--candidate', type=Path, required=True)
    args = parser.parse_args()
    with run_log('native-entry-acceptance') as logger:
        raise SystemExit(main(args.candidate.resolve(strict=True), logger))
