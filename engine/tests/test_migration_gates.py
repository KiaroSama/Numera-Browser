"""Migration readiness must not be inferred from a small passing Rust slice."""
import importlib.util
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SCRIPTS = ROOT / 'scripts' / 'migration'


def load(name):
    sys.path.insert(0, str(SCRIPTS))
    try:
        spec = importlib.util.spec_from_file_location(f'migration_{name}', SCRIPTS / f'{name}.py')
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        return module
    finally:
        sys.path.remove(str(SCRIPTS))


def test_empty_missing_and_forged_ledgers_cannot_allow_switch(tmp_path, monkeypatch):
    gate = load('check_coverage')
    source = {'files':[{'path':'source.py'}], 'capabilities':[{'id':'source.py::required'}]}
    monkeypatch.setattr(gate, 'discover', lambda: source)
    path = tmp_path / 'coverage.json'
    for document in ([], {}, {'capabilities':[]},
                     {'files':source['files'], 'capabilities':[{'id':'source.py::other','status':'verified'}]},
                     {'files':source['files'], 'capabilities':[{'id':'source.py::required','status':'pending'}]},
                     {'files':source['files'], 'capabilities':[{'id':'source.py::required','status':'verified','target':'../outside','test':'../outside'}]}):
        path.write_text(json.dumps(document), encoding='utf-8')
        assert gate.check(path), 'Unproved coverage was accepted'


def test_owned_migration_command_propagates_exit_and_bounds_idle(tmp_path):
    helper = load('run_owned')
    with helper.run_log('owned-command-fixture', tmp_path) as logger:
        assert helper.run([sys.executable, '-c', 'print("ready"); raise SystemExit(7)'], 10, 5, logger) == 7
        # An Event wait models a silent blocked child; the outer owner must end it.
        import pytest
        with pytest.raises(TimeoutError):
            helper.run([sys.executable, '-c', 'import threading; threading.Event().wait()'], 5, 0.5, logger)
    assert not logger.handlers


def test_migration_logs_are_unique_sanitized_closed_and_fallback(tmp_path, capsys):
    helper = load('run_logging')
    directory = tmp_path / 'path with spaces'
    for _ in range(2):
        with helper.run_log('fixture', directory) as logger:
            logger.warning('password=synthetic-fixture-value')
    files = list(directory.glob('fixture_*_UTC*.log'))
    assert len(files) == 2
    text = ''.join(path.read_text(encoding='utf-8') for path in files)
    assert 'synthetic-fixture-value' not in text + capsys.readouterr().err
    assert '[WARNING]' in text and 'UTC]' in text and 'exit=0' in text
    for path in files:
        path.unlink()  # Windows cannot remove an open file handler.
    blocked = tmp_path / 'not-a-directory'
    blocked.write_text('fixture', encoding='utf-8')
    with helper.run_log('fallback-fixture', blocked) as logger:
        logger.error('token=synthetic-fixture-value')
    console = capsys.readouterr().err
    assert 'console only' in console and 'synthetic-fixture-value' not in console
