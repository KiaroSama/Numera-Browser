"""The desktop negative proof must compile, fail semantically and restore bytes."""
from contextlib import contextmanager
import hashlib
import importlib.util
import logging
from pathlib import Path
import sys
from types import SimpleNamespace

import pytest

ROOT = Path(__file__).resolve().parents[2]


def load(monkeypatch):
    from test_native_regressions import load as native_load

    native = native_load()
    monkeypatch.setitem(sys.modules, 'native_regressions', native)
    monkeypatch.setitem(sys.modules, 'run_owned', SimpleNamespace(run=None))
    monkeypatch.setitem(sys.modules, 'run_logging', SimpleNamespace(run_log=None))
    spec = importlib.util.spec_from_file_location('desktop_history_fixture', ROOT / 'scripts/ci/desktop_history.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module, native


@pytest.mark.timeout(10)
@pytest.mark.parametrize('platform', ['linux', 'darwin'])
@pytest.mark.parametrize('damage', [None, 'compile', 'run', 'zero', 'wrong-assertion', 'restore', 'hash'])
def test_desktop_history_requires_semantic_failure_and_fixed_source_restoration(
        tmp_path, monkeypatch, platform, damage):
    module, native = load(monkeypatch)
    case = module.CASE
    source = tmp_path / next(iter(case['files']))
    source.parent.mkdir(parents=True)
    fixed, old = b'fixed updater source\n', b'historical updater source\n'
    source.write_bytes(fixed)
    monkeypatch.setattr(module, 'sys', SimpleNamespace(platform=platform))
    monkeypatch.setenv('RUNNER_OS', 'Linux' if platform == 'linux' else 'macOS')
    monkeypatch.setattr(module, 'require_hosted_source', lambda root: 'a' * 40)
    monkeypatch.setattr(module, 'CASE', dict(case, files={str(source.relative_to(tmp_path)):
                        hashlib.sha256(old).hexdigest()}))
    monkeypatch.setattr(native, 'git', lambda *args: old + (b'changed' if damage == 'hash' else b''))
    commands = []

    def execute(command, **options):
        commands.append(command)
        if command[0] == 'git':
            assert options == {'stdin': module.subprocess.DEVNULL, 'check': True, 'timeout': 10}
            assert source.read_bytes() == fixed
            return
        assert command == ['cargo', 'test', '--manifest-path', str(tmp_path / 'desktop/src-tauri/Cargo.toml'),
                           '--locked', '--offline', '--no-default-features', '--lib', '--no-run']
        assert options == {'stdin': module.subprocess.DEVNULL, 'check': True}
        assert source.read_bytes() == old
        if damage == 'compile':
            raise module.subprocess.CalledProcessError(1, command)

    def run(command, wall, idle, logger):
        commands.append(command)
        assert source.read_bytes() == old
        assert (wall, idle) == (180, 90)
        assert command == ['cargo', 'test', '--manifest-path', str(tmp_path / 'desktop/src-tauri/Cargo.toml'),
                           '--locked', '--offline', '--no-default-features', '--lib', '--',
                           case['test'], '--exact', '--test-threads=1', '--color=never']
        if damage == 'run':
            raise TimeoutError('Fixture runner failed')
        logger.info('running 0 tests' if damage == 'zero' else 'running 1 test')
        logger.info('test %s ... FAILED', case['test'])
        logger.info('%s', 'unrelated assertion' if damage == 'wrong-assertion' else case['symptom'])
        logger.info('test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; '
                    '1 filtered out; finished in 0.01s')
        return 0 if damage == 'zero' else 101

    actual_swap = module.historical_sources

    @contextmanager
    def swap(root, selected, inputs):
        with actual_swap(root, selected, inputs):
            yield
        if damage == 'restore':
            raise RuntimeError('Fixed native source restoration failed')

    monkeypatch.setattr(module.subprocess, 'run', execute)
    monkeypatch.setattr(module, 'run', run)
    monkeypatch.setattr(module, 'historical_sources', swap)
    logger = logging.Logger('desktop-history-fixture', level=logging.INFO)
    if damage:
        with pytest.raises((ValueError, RuntimeError, TimeoutError, module.subprocess.CalledProcessError)):
            module.prove(tmp_path, logger)
    else:
        module.prove(tmp_path, logger)
    assert source.read_bytes() == fixed
    assert logger.handlers == []
    assert len(commands) == (1 if damage == 'hash' else 2 if damage == 'compile' else 3)


@pytest.mark.timeout(10)
@pytest.mark.parametrize('damage', ['windows', 'wrong-runner', 'untrusted-source', 'dirty'])
def test_desktop_history_refuses_before_cargo_on_unverified_source(monkeypatch, damage):
    module, _ = load(monkeypatch)
    monkeypatch.setattr(module, 'sys', SimpleNamespace(platform='win32' if damage == 'windows' else 'linux'))
    monkeypatch.setenv('RUNNER_OS', 'macOS' if damage == 'wrong-runner' else 'Linux')
    calls = []

    def require(root):
        if damage == 'untrusted-source':
            raise ValueError('Exact hosted source required')
        return 'a' * 40

    def execute(command, **options):
        calls.append(command)
        assert command[0] == 'git'
        raise module.subprocess.CalledProcessError(1, command)

    monkeypatch.setattr(module, 'require_hosted_source', require)
    monkeypatch.setattr(module.subprocess, 'run', execute)
    monkeypatch.setattr(module, 'run', lambda *args: pytest.fail('Unverified source executed Cargo'))
    with pytest.raises((ValueError, module.subprocess.CalledProcessError)):
        module.prove(ROOT, logging.Logger('desktop-history-refusal'))
    assert len(calls) == (1 if damage == 'dirty' else 0)
