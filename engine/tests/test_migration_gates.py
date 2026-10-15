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


def test_repaired_baseline_is_reachable_without_changing_fixture_provenance(monkeypatch):
    inventory = load('inventory')

    def original_paths(prefix):
        assert prefix == 'engine/numera'
        return [ROOT / 'engine/numera/models.py']

    monkeypatch.setattr(inventory, 'baseline_paths', original_paths)
    assert inventory.tracked_paths('engine/numera') == [ROOT / 'engine/numera/models.py']
    assert inventory.BASELINE_TREE == '760f9873ded13c5ebfecae1f16b385e34f695124'
    assert inventory.BASELINE == '13b6d9c6973281fe123353eaec05b29d65781a2d'


def test_original_inventory_reads_frozen_bytes_instead_of_working_source(tmp_path, monkeypatch):
    import hashlib
    inventory = load('inventory')
    source = tmp_path / 'original/engine/numera/models.py'
    source.parent.mkdir(parents=True)
    original = b'def original_contract():\n    return 1\n'
    source.write_bytes(original)
    monkeypatch.setattr(inventory, 'source_root', lambda: tmp_path / 'original')
    row = inventory.describe(source)
    assert row['path'] == 'engine/numera/models.py'
    assert row['sha256'] == hashlib.sha256(original).hexdigest()
    assert row['symbols'] == ['original_contract']


def test_frozen_reference_rejects_drift_and_keeps_current_python_modules(tmp_path, monkeypatch):
    import pytest
    baseline = load('baseline_source')
    monkeypatch.setattr(baseline, 'ROOT', tmp_path)
    monkeypatch.delenv(baseline.ENVIRONMENT, raising=False)
    names = ('engine/numera/__init__.py', 'engine/tests/test_original.py')
    contents = (b'VALUE = "original"\n', b'def test_original(): pass\n')
    rows = tuple((name, baseline.blob_oid(raw), len(raw), False) for name, raw in zip(names, contents))
    monkeypatch.setattr(baseline, 'entries', lambda: rows)
    calls = []
    def batch(*arguments, data):
        assert arguments == ('cat-file', '--batch')
        calls.append(data)
        return b''.join(f'{oid} blob {size}\n'.encode('ascii') + raw + b'\n'
                        for (_, oid, size, _), raw in zip(rows, contents))
    monkeypatch.setattr(baseline, 'git', batch)
    with baseline.reference() as root:
        assert baseline.verify(root) == root
        assert (root / names[0]).read_bytes() == contents[0]
        (root / names[0]).write_bytes(b'VALUE = "changed"\n')
        with pytest.raises(ValueError, match='bytes differ'):
            baseline.verify(root)
        (root / names[0]).write_bytes(contents[0])
        (root / 'unexpected.py').write_bytes(b'raise RuntimeError("unreviewed")\n')
        with pytest.raises(ValueError, match='inventory differs'):
            baseline.verify(root)
    assert not root.exists() and len(calls) == 1
    from types import SimpleNamespace
    monkeypatch.setattr(baseline, 'source_root', lambda: tmp_path / 'original')
    monkeypatch.setitem(sys.modules, 'numera', SimpleNamespace(__file__=str(tmp_path / 'current/numera/__init__.py')))
    with pytest.raises(RuntimeError, match='nonbaseline'):
        baseline.activate()


def test_original_observer_imports_reference_not_current_path(tmp_path):
    import os
    import subprocess
    fake = tmp_path / 'current/numera'
    fake.mkdir(parents=True)
    (fake / '__init__.py').write_text('raise RuntimeError("current source leaked")\n', encoding='utf-8')
    program = '''import json, os, pathlib, sys
import run_logging
assert "numera" not in sys.modules, "Logging utility pre-imported current source"
import baseline_source as baseline
baseline.ROOT = pathlib.Path(os.environ["FIXTURE_ROOT"])
raw = b'VALUE = "original"\\n'
oid = baseline.blob_oid(raw)
baseline.entries = lambda: (("engine/numera/__init__.py", oid, len(raw), False),)
calls = []
def git(*arguments, data):
    calls.append(data)
    return (oid + " blob " + str(len(raw)) + "\\n").encode("ascii") + raw + b"\\n"
baseline.git = git
root = baseline.activate()
import numera
assert numera.VALUE == "original"
assert pathlib.Path(numera.__file__).is_relative_to(root / "engine")
assert baseline.activate() == root and len(calls) == 1
print(json.dumps({"root": str(root), "baseline": baseline.BASELINE_TREE}))
'''
    environment = {**os.environ, 'PYTHONPATH': os.pathsep.join((str(SCRIPTS), str(fake.parent))),
                   'PYTHONUTF8': '1', 'FIXTURE_ROOT': str(tmp_path)}
    environment.pop('NUMERA_ORACLE_SOURCE_ROOT', None)
    completed = subprocess.run([sys.executable, '-c', program], env=environment, stdin=subprocess.DEVNULL,
                               capture_output=True, encoding='utf-8', errors='strict', timeout=10, check=True)
    result = json.loads(completed.stdout)
    assert result['baseline'] == '760f9873ded13c5ebfecae1f16b385e34f695124'
    assert not Path(result['root']).exists(), 'Owned original-source reference survived observer exit'


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
    text = '\n'.join(path.read_text(encoding='utf-8') for path in tmp_path.glob('owned-command-fixture_*_UTC*.log'))
    for phase in ('spawn', 'monitor', 'settle-tree', 'reap-leader', 'join-reader', 'close-output', 'remove-temp'):
        assert 'phase=' + phase in text


def test_linux_temp_root_fits_chromium_socket_and_keeps_windows_contained(monkeypatch):
    from pathlib import PurePosixPath
    import pytest

    helper = load('run_owned')
    monkeypatch.setattr(helper, 'Path', PurePosixPath)
    hosted = {'GITHUB_ACTIONS': 'true', 'RUNNER_TEMP': '/home/runner/work/_temp'}
    root = PurePosixPath('/home/runner/work/Numera-Browser/Numera-Browser')
    selected = helper.temporary_parent(root, hosted, 'linux')
    assert selected == root / '.ci-work'
    assert len(str(selected / 't000000').encode('utf-8')) + len(helper.CHROMIUM_SOCKET_SUFFIX) <= 107
    long_root = PurePosixPath('/' + 'a' * 100)
    assert helper.temporary_parent(long_root, hosted, 'linux') == PurePosixPath(hosted['RUNNER_TEMP'])
    assert helper.temporary_parent(long_root, hosted, 'win32') == long_root / '.ci-work'
    unicode_root = PurePosixPath('/' + 'Ω' * 40)
    assert helper.temporary_parent(unicode_root, hosted, 'linux') == PurePosixPath(hosted['RUNNER_TEMP'])
    with pytest.raises(ValueError, match='hosted RUNNER_TEMP'):
        helper.temporary_parent(long_root, {}, 'linux')
    with pytest.raises(ValueError, match='also exceeds'):
        helper.temporary_parent(long_root, {**hosted, 'RUNNER_TEMP': '/' + 'x' * 100}, 'linux')


def test_owned_temp_is_unique_and_cleanup_removes_only_its_run(tmp_path, monkeypatch):
    helper = load('run_owned')
    root = tmp_path / 'project with spaces'
    monkeypatch.setattr(helper, 'ROOT', root)
    created = []
    create = helper.create_temporary

    def record(*args):
        path = create(*args)
        created.append(path)
        return path

    monkeypatch.setattr(helper, 'create_temporary', record)
    program = '''import os,socket,sys
from pathlib import Path
if sys.platform == "linux":
    directory=Path(os.environ["TMPDIR"])/"org.chromium.Chromium.123456"
    directory.mkdir()
    path=directory/"SingletonSocket"
    assert len(str(path).encode("utf-8")) <= 107
    with socket.socket(socket.AF_UNIX,socket.SOCK_STREAM) as listener:
        listener.bind(str(path))
        print("actual singleton socket bound",flush=True)
else:
    root=Path(os.environ["TMPDIR"])
    assert root.is_dir() and root.name.startswith("t")
    print("owned platform temp resolved",flush=True)
'''
    with helper.run_log('temp-fixture', tmp_path) as logger:
        assert helper.run([sys.executable, '-c', program], 10, 5, logger) == 0
    assert len(created) == 1 and not created[0].exists()
    assert created[0].parent == helper.temporary_parent(root, __import__('os').environ, sys.platform)
    first = helper.create_temporary(root, __import__('os').environ, sys.platform)
    second = helper.create_temporary(root, __import__('os').environ, sys.platform)
    try:
        assert first != second and first.is_dir() and second.is_dir()
    finally:
        first.rmdir()
        second.rmdir()


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
