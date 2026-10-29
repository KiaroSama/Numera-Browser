"""Exercise original/native password prompts through an owned POSIX terminal."""
from __future__ import annotations
import argparse
import json
import os
import pty
import select
import subprocess
import sys
import tempfile
import termios
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
from baseline_source import source_root
ORIGINAL = source_root()
sys.path.insert(0, str(ROOT / 'engine'))
from numera import proctree
from run_logging import run_log
from native_terminal_cases import followups


def execute(argv, env, stages, *, wall=15, idle=5):
    master, slave = pty.openpty()
    process = tree = None
    try:
        # proctree creates the session; this small bootstrap attaches its tty,
        # then exec preserves the captured PID/birth/group for the actual entry.
        bootstrap = 'import fcntl,os,sys,termios; fcntl.ioctl(0,termios.TIOCSCTTY,0); os.execve(sys.argv[1],sys.argv[1:],os.environ)'
        process, tree = proctree.spawn([sys.executable, '-I', '-c', bootstrap, *argv], env=env,
                                       stdin=slave, stdout=slave, stderr=slave)
        os.close(slave)
        slave = None
        output = bytearray()
        stage = 0
        end = time.monotonic() + wall
        progress = time.monotonic()
        while True:
            if select.select([master], [], [], 0.02)[0]:
                try:
                    data = os.read(master, 8192)
                except OSError as error:
                    if error.errno != 5:
                        raise
                    data = b''
                output.extend(data)
                if data:
                    progress = time.monotonic()
                if len(output) > 65536:
                    raise RuntimeError('TTY output exceeds bound')
            if stage < len(stages) and stages[stage][0].encode('utf-8') in output:
                data = stages[stage][1].encode('utf-8')
                if os.write(master, data) != len(data):
                    raise RuntimeError('TTY input was not written completely')
                stage += 1
            if process.poll() is not None:
                # Drain only bytes actually ready after the child has exited.
                while select.select([master], [], [], 0)[0]:
                    try:
                        data = os.read(master, 8192)
                    except OSError as error:
                        if error.errno != 5:
                            raise
                        break
                    if not data:
                        break
                    output.extend(data)
                    if len(output) > 65536:
                        raise RuntimeError('TTY final output exceeds bound')
                break
            if time.monotonic() >= end or time.monotonic() - progress > idle:
                raise TimeoutError(f'TTY wall/idle bound exceeded input_stage={stage}')
        assert stage == len(stages), 'TTY prompts were not exercised'
        assert termios.tcgetattr(master)[3] & termios.ECHO, 'TTY echo was not restored'
        return process.returncode, output.decode('utf-8')
    finally:
        clean = True
        try:
            if tree:
                clean = proctree.end_tree(tree, grace=1, kill_grace=3)
        finally:
            try:
                if process:
                    process.wait(timeout=5)
            finally:
                try:
                    if slave is not None:
                        os.close(slave)
                finally:
                    os.close(master)
        if not clean:
            raise RuntimeError('TTY child tree did not settle')


def main(candidate, log):
    if os.name != 'posix':
        raise RuntimeError('This fixture requires a POSIX terminal')
    with tempfile.TemporaryDirectory(prefix='native-tty-', dir=ROOT / '.ci-work/migration-temp') as temporary:
        parent = Path(temporary)
        home = parent / 'home'
        home.mkdir()
        env = os.environ.copy()
        for key in ('NUMERA_PASSWORD', 'NUMERA_API_TOKEN', 'NUMERA_DATA_DIR', 'NUMERA_NATIVE_SUPERVISE',
                    'NUMERA_NATIVE_JOB_GATE', 'NUMERA_HELPER_UNITS', 'NUMERA_HELPER_SCOPE', 'NUMERA_PARENT_PID'):
            env.pop(key, None)
        env.update(PYTHONUTF8='1', PYTHONDONTWRITEBYTECODE='1', PYTHONPATH=str(ORIGINAL / 'engine'), HOME=str(home),
                   NUMERA_APP_DIR=str(parent), NUMERA_LOG_DIR=str(parent / 'logs'),
                   NUMERA_BROWSER_ADAPTER=str(parent / 'absent.py'), NUMERA_SDK_PYTHON=sys.executable)
        for label, executable in [('native', [str(candidate)]), ('source', [sys.executable, '-m', 'numera'])]:
            for accepted in (False, True):
                root = parent / f'{label}-{accepted}'
                first, second = 'fixture-α-valid', 'fixture-α-valid' if accepted else 'fixture-other'
                code, text = execute([*executable, '--data-dir', str(root), 'vault', 'enable'], env,
                                     [('Master password: ', first + ('x\x7f' if accepted else '') + '\n'),
                                      ('Confirm master password: ', second + '\n')],
                                     wall=45 if accepted else 15, idle=30 if accepted else 5)
                assert code == (0 if accepted else 1), (label, accepted, code)
                assert first not in text and second not in text, 'TTY echoed password input'
                if accepted:
                    assert 'Vault enabled' in text
                    config = json.loads((root / 'config.json').read_text(encoding='utf-8'))
                    assert config.get('vault'), 'TTY vault enable did not persist descriptor'
                else:
                    assert "Passwords didn't match." in text
                    assert not (root / 'vault-migration.json').exists()
                log.info('TTY vault prompt/no-echo/owned cleanup passed runtime=%s accepted=%s', label, accepted)
            followups(parent, executable, label, env, execute, '\n', log)
    return 0


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--candidate', type=Path, required=True)
    args = parser.parse_args()
    with run_log('native-tty-acceptance') as logger:
        raise SystemExit(main(args.candidate.resolve(strict=True), logger))
