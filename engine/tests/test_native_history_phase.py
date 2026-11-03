"""Compiler diagnostics never publish raw tracing or change historical verdicts."""
from __future__ import annotations

import importlib.util
import json
import logging
from pathlib import Path
from types import SimpleNamespace

import pytest

from test_native_regressions import load as load_runner

ROOT = Path(__file__).resolve().parents[2]
pytestmark = pytest.mark.timeout(10)


def load():
    spec = importlib.util.spec_from_file_location(
        'history_phase_fixture', ROOT / 'scripts/ci/native_history_phase.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class Capture(logging.Handler):
    def __init__(self):
        super().__init__()
        self.rows = []

    def emit(self, record):
        self.rows.append(record.getMessage())


def trace(root, path):
    return {'reason': 'compiler-message', 'manifest_path': str(root / 'engine-rust/Cargo.toml'),
            'target': {'name': 'numera_core', 'kind': ['lib']},
            'message': {'level': 'INFO', 'target': 'rustc_codegen_ssa::back::link',
                        'fields': {'message': 'preparing rlib to ' + json.dumps(str(path))}}}


def artifact(root, path, *, fresh=False):
    return {'reason': 'compiler-artifact', 'manifest_path': str(root / 'engine-rust/Cargo.toml'),
            'target': {'name': 'numera_core', 'kind': ['lib']},
            'profile': {'test': False}, 'fresh': fresh, 'filenames': [str(path)]}


@pytest.mark.parametrize('damage', [None, 'missing', 'duplicate', 'malformed', 'foreign',
                                    'wrong-file', 'reordered', 'unfinished', 'fresh'])
def test_phase_projection_requires_one_matching_archive_and_artifact(tmp_path, monkeypatch, damage):
    module = load()
    clock = iter(range(100, 10000, 100))
    monkeypatch.setattr(module.time, 'monotonic_ns', lambda: next(clock))
    logger = logging.Logger('compiler-phase-fixture', level=logging.INFO)
    capture = Capture()
    logger.addHandler(capture)
    path = tmp_path / 'engine-rust/target/debug/deps/libnumera_core-0123456789abcdef.rlib'
    event = artifact(tmp_path, path, fresh=damage == 'fresh')
    if damage == 'foreign':
        event['manifest_path'] = str(tmp_path / 'foreign/Cargo.toml')
    elif damage == 'wrong-file':
        event['filenames'] = [str(path.with_name('foreign.rlib'))]
    with module.compiler_phase(tmp_path, 7, logger, enabled=True) as options:
        assert options == ['--message-format=json']
        if damage == 'reordered':
            logger.info('%s', json.dumps(event))
        if damage not in ('missing', 'fresh'):
            logger.info('%s', json.dumps(trace(tmp_path, path)))
        if damage == 'duplicate':
            logger.info('%s', json.dumps(trace(tmp_path, path)))
        if damage == 'malformed':
            logger.info('{"private malformed compiler data')
        if damage != 'reordered':
            logger.info('%s', json.dumps(event))
        logger.info('%s', json.dumps({'reason': 'build-script-executed',
                                     'env': [['PRIVATE_VALUE', 'never-publish']]}))
        if damage != 'unfinished':
            logger.info('%s', json.dumps({'reason': 'build-finished', 'success': True}))
        logger.info('running 1 test')
    assert logger.filters == []
    joined = '\n'.join(capture.rows)
    assert str(tmp_path) not in joined and 'private directory' not in joined
    assert 'never-publish' not in joined and 'PRIVATE_VALUE' not in joined
    assert 'manifest_path' not in joined and 'numera_core-' not in joined
    assert 'running 1 test' in capture.rows
    summary = capture.rows[-1]
    expected = 'complete' if damage is None else 'fresh' if damage == 'fresh' else 'unavailable'
    assert f'status={expected}' in summary
    assert 'receipt-interval-not-exclusive-compiler' in summary
    if damage is None:
        assert any('event=archive-start elapsed_ns=100' in row for row in capture.rows)
        assert any('event=archive-complete elapsed_ns=200 archive_to_artifact_ns=100' in row
                   for row in capture.rows)


@pytest.mark.parametrize('payload', [None, [], 'private text', {'fields': None},
                                    {'reason': 'build-finished', 'success': 'private'},
                                    {'target': 'rustc_codegen_ssa::back::link', 'fields': {'message': []}}])
def test_malformed_json_never_reaches_handlers(tmp_path, payload):
    module = load()
    logger = logging.Logger('compiler-json-fixture', level=logging.INFO)
    capture = Capture()
    logger.addHandler(capture)
    with module.compiler_phase(tmp_path, 0, logger, enabled=True):
        logger.info('%s', json.dumps({'private': payload}))
        logger.info('%s', '{' + 'private' * 10000)
    assert len(capture.rows) == 1 and 'status=unavailable' in capture.rows[0]
    assert 'private' not in capture.rows[0]


@pytest.mark.parametrize('enabled', [False, True])
@pytest.mark.parametrize('failure', [False, True])
def test_scoped_environment_and_logger_restore_after_success_or_failure(tmp_path, monkeypatch, enabled, failure):
    module = load()
    prior = {'RUSTC_LOG': 'existing-filter', 'RUSTC_LOG_OUTPUT_TARGET': 'private-existing-file',
             'RUSTC_LOG_FORMAT_JSON': 'prior-format', 'RUSTC_LOG_BACKTRACE': 'prior-backtrace'}
    for key in list(module.os.environ):
        if key.startswith('RUSTC_LOG'):
            monkeypatch.delenv(key)
    for key, value in prior.items():
        monkeypatch.setenv(key, value)
    logger = logging.Logger('compiler-environment-fixture', level=logging.INFO)
    untouched = logging.Filter()
    logger.addFilter(untouched)
    original = TimeoutError('original proof error')

    def exercise():
        with module.compiler_phase(tmp_path, 0, logger, enabled=enabled) as options:
            actual = {k: v for k, v in module.os.environ.items() if k.startswith('RUSTC_LOG')}
            if enabled:
                assert actual == {'RUSTC_LOG': 'rustc_codegen_ssa::back::link=info',
                                  'RUSTC_LOG_FORMAT_JSON': '1', 'RUSTC_LOG_COLOR': 'never'}
                assert options == ['--message-format=json']
            else:
                assert actual == prior and options == []
            if failure:
                raise original
    if failure:
        with pytest.raises(TimeoutError) as caught:
            exercise()
        assert caught.value is original
    else:
        exercise()
    assert logger.filters == [untouched]
    assert {k: v for k, v in module.os.environ.items() if k.startswith('RUSTC_LOG')} == prior
    assert not (tmp_path / 'private-existing-file').exists()


@pytest.mark.parametrize('platform', ['win32', 'linux'])
@pytest.mark.parametrize('failure', [False, True])
def test_public_proof_preserves_assertions_source_and_diagnostic_scope(tmp_path, monkeypatch, platform, failure):
    import hashlib
    import sys

    monkeypatch.syspath_prepend(str(ROOT / 'scripts/ci'))
    module = load_runner()
    source = tmp_path / 'engine-rust/src/models.rs'
    source.parent.mkdir(parents=True)
    source.write_bytes(b'fixed')
    old = b'previous defect'
    case = dict(module.REGRESSIONS[0], files={
        'engine-rust/src/models.rs': hashlib.sha256(old).hexdigest()}, companions={})
    monkeypatch.setattr(module, 'REGRESSIONS', (case,))
    monkeypatch.setattr(module, 'sys', SimpleNamespace(platform=platform))
    monkeypatch.setattr(module, 'require_hosted_source', lambda root: 'a' * 40)
    monkeypatch.setattr(module, 'git', lambda *args: old)
    monkeypatch.setitem(sys.modules, 'baseline_source', SimpleNamespace(ENVIRONMENT='NUMERA_ORACLE_SOURCE_ROOT'))
    monkeypatch.setenv('RUSTC_LOG', 'existing-filter')
    calls = []

    def warm(command, **kwargs):
        assert '--message-format=json' not in command and module.os.environ['RUSTC_LOG'] == 'existing-filter'
        calls.append('cold')

    original = TimeoutError('original proof failure')

    def run(command, wall, idle, logger):
        assert source.read_bytes() == old and (wall, idle) == (180, 90)
        assert command[-4:] == [case['test'], '--exact', '--test-threads=1', '--color=never']
        assert ('--message-format=json' in command) == (platform == 'win32')
        calls.append('proof')
        if failure:
            raise original
        for line in ('running 1 test', f"test {case['test']} ... FAILED", case['symptom'],
                     'test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; '
                     '1 filtered out; finished in 0.01s'):
            logger.info('%s', line)
        return 101

    monkeypatch.setattr(module.subprocess, 'run', warm)
    monkeypatch.setitem(sys.modules, 'run_owned', SimpleNamespace(run=run))
    logger = logging.Logger('compiler-public-proof-fixture', level=logging.INFO)
    if failure:
        with pytest.raises(TimeoutError) as caught:
            module.prove(tmp_path, tmp_path / 'original', logger)
        assert caught.value is original
    else:
        module.prove(tmp_path, tmp_path / 'original', logger)
    assert calls == ['cold', 'proof']
    assert source.read_bytes() == b'fixed' and logger.handlers == [] and logger.filters == []
    assert module.os.environ['RUSTC_LOG'] == 'existing-filter'


@pytest.mark.parametrize('damage', [None, 'malformed', 'oversized', 'linked', 'cleanup-error'])
def test_generated_compiler_cache_preserves_diagnostics_and_refuses_uncertainty(tmp_path, monkeypatch, damage):
    module = load()
    logger = logging.Logger('compiler-cache-fixture', level=logging.INFO)
    capture = Capture()
    logger.addHandler(capture)
    directory = tmp_path / 'engine-rust/target/debug/.fingerprint/numera-core-0123456789abcdef'
    directory.mkdir(parents=True)
    output = directory / 'output-lib-numera_core'
    diagnostic = b'{"message":"required diagnostic","level":"warning"}\r\n'
    raw = json.dumps(trace(tmp_path, tmp_path / 'private.rlib')['message']).encode('utf-8') + b'\n'
    payload = diagnostic + raw
    if damage == 'malformed':
        payload += b'{private malformed record\n'
    elif damage == 'oversized':
        monkeypatch.setattr(module, 'CACHE_BYTES', 32)
    output.write_bytes(payload)
    untouched = directory / 'lib-numera_core.json'
    untouched.write_bytes(b'fingerprint bytes must not change')
    if damage == 'linked':
        import os
        os.link(output, directory / 'shared-output')
    elif damage == 'cleanup-error':
        original_open = Path.open

        def unavailable(path, mode='r', *args, **kwargs):
            if path == output and mode == 'rb':
                raise PermissionError('private cleanup error')
            return original_open(path, mode, *args, **kwargs)
        monkeypatch.setattr(Path, 'open', unavailable)
    if damage:
        with pytest.raises(RuntimeError, match='Compiler diagnostic cache could not be sanitized'):
            with module.compiler_phase(tmp_path, 0, logger, enabled=True):
                pass
        if damage == 'cleanup-error':
            monkeypatch.setattr(Path, 'open', original_open)
        assert output.read_bytes() == payload
        assert any('status=cache-cleanup-failed' in row for row in capture.rows)
    else:
        with module.compiler_phase(tmp_path, 0, logger, enabled=True):
            pass
        assert output.read_bytes() == diagnostic
        assert any('cache_records_removed=1' in row for row in capture.rows)
    assert untouched.read_bytes() == b'fingerprint bytes must not change'
    assert logger.filters == [] and all('private' not in row for row in capture.rows)


@pytest.mark.parametrize('failure', [False, True])
def test_cache_cleanup_keeps_original_proof_error(tmp_path, monkeypatch, failure):
    module = load()
    logger = logging.Logger('compiler-cache-error-fixture', level=logging.INFO)
    capture = Capture()
    logger.addHandler(capture)
    monkeypatch.setattr(module, 'sanitize_cache', lambda *args: (_ for _ in ()).throw(OSError('private')))
    original = TimeoutError('original proof timeout')
    with pytest.raises(TimeoutError if failure else RuntimeError) as caught:
        with module.compiler_phase(tmp_path, 0, logger, enabled=True):
            if failure:
                raise original
    if failure:
        assert caught.value is original
    assert logger.filters == [] and any('status=cache-cleanup-failed' in row for row in capture.rows)
    assert all('private' not in row for row in capture.rows)


@pytest.mark.parametrize('damage', [None, 'missing', 'save', 'workspace'])
def test_windows_compiler_trace_cache_never_leaves_the_hosted_job(damage):
    from copy import deepcopy
    import yaml

    spec = importlib.util.spec_from_file_location('compiler_cache_coverage', ROOT / 'scripts/ci/suites.py')
    coverage = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(coverage)
    steps = yaml.safe_load((ROOT / '.github/workflows/ci.yml').read_text(encoding='utf-8'))['jobs']['rust-core']['steps']
    steps = deepcopy(steps)
    cache = next(step for step in steps if step.get('uses', '').startswith('Swatinem/rust-cache@'))
    if damage == 'missing':
        cache['with'].pop('save-if')
    elif damage == 'save':
        cache['with']['save-if'] = 'true'
    elif damage == 'workspace':
        cache['with']['cache-workspace-crates'] = 'true'
    if damage:
        with pytest.raises(ValueError, match='Historical compiler diagnostics must not enter Windows caches'):
            coverage.check_native_compiler_cache(steps)
    else:
        coverage.check_native_compiler_cache(steps)


@pytest.mark.parametrize('damage', [None, 'write', 'flush', 'replace'])
def test_atomic_cache_filter_keeps_original_bytes_until_replacement(tmp_path, monkeypatch, damage):
    module = load()
    directory = tmp_path / 'engine-rust/target/debug/.fingerprint/numera-core-0123456789abcdef'
    directory.mkdir(parents=True)
    path = directory / 'output-lib-numera_core'
    diagnostic = b'{"level":"warning","message":"retain this diagnostic"}\n'
    raw = json.dumps(trace(tmp_path, tmp_path / 'private.rlib')['message']).encode('utf-8') + b'\n'
    path.write_bytes(diagnostic + raw)
    if damage == 'replace':
        monkeypatch.setattr(module.os, 'replace', lambda *args: (_ for _ in ()).throw(OSError('synthetic')))
    elif damage:
        create = module.tempfile.NamedTemporaryFile

        class Output:
            def __enter__(self):
                self.file = create(dir=directory, prefix='compiler-phase-', delete=False)
                return self

            def __exit__(self, *args):
                self.file.close()

            def __getattr__(self, name):
                if name == damage:
                    return lambda *args: (_ for _ in ()).throw(OSError('synthetic'))
                return getattr(self.file, name)

        monkeypatch.setattr(module.tempfile, 'NamedTemporaryFile', lambda **kwargs: Output())
    seen = {}
    if damage:
        with pytest.raises(OSError):
            module.sanitize_cache(tmp_path, seen)
        assert path.read_bytes() == diagnostic + raw and seen == {}
    else:
        assert module.sanitize_cache(tmp_path, seen) == 1
        assert path.read_bytes() == diagnostic
    assert {entry.name for entry in directory.iterdir()} == {path.name}


def test_unchanged_diagnostic_cache_needs_no_write_and_is_not_reread(tmp_path, monkeypatch):
    module = load()
    directory = tmp_path / 'engine-rust/target/debug/.fingerprint/dependency-0123456789abcdef'
    directory.mkdir(parents=True)
    path = directory / 'output-lib-dependency'
    diagnostic = b'{"level":"warning","message":"unchanged dependency diagnostic"}\n'
    path.write_bytes(diagnostic)
    opened = []
    original = Path.open

    def read_only(file, mode='r', *args, **kwargs):
        if file == path:
            assert mode == 'rb' and not opened
            opened.append(file)
        return original(file, mode, *args, **kwargs)

    monkeypatch.setattr(Path, 'open', read_only)
    seen = {}
    assert module.sanitize_cache(tmp_path, seen) == 0
    assert module.sanitize_cache(tmp_path, seen) == 0
    assert opened == [path]


def test_rust_debug_unicode_path_binds_to_cargo_artifact(tmp_path):
    module = load()
    root = tmp_path / ('unicode' + chr(0x200d) + 'path')
    path = root / 'engine-rust/target/debug/deps/libnumera_core-0123456789abcdef.rlib'
    event = trace(root, path)
    inner = json.dumps(str(path), ensure_ascii=False).replace(chr(0x200d), r'\u{200d}')
    event['message']['fields']['message'] = 'preparing rlib to ' + inner
    logger = logging.Logger('compiler-unicode-fixture', level=logging.INFO)
    capture = Capture()
    logger.addHandler(capture)
    with module.compiler_phase(root, 0, logger, enabled=True):
        for row in (event, artifact(root, path), {'reason': 'build-finished', 'success': True}):
            logger.info('%s', json.dumps(row))
    assert 'status=complete' in capture.rows[-1]
    assert all('unicode' not in row and str(path) not in row for row in capture.rows)


def test_returned_invalid_proof_takes_precedence_over_cache_cleanup(tmp_path, monkeypatch):
    import hashlib
    import sys

    monkeypatch.syspath_prepend(str(ROOT / 'scripts/ci'))
    module = load_runner()
    monkeypatch.setitem(module.compiler_phase.__wrapped__.__globals__, 'sanitize_cache',
                        lambda *args: (_ for _ in ()).throw(OSError('private cleanup error')))
    source = tmp_path / 'engine-rust/src/models.rs'
    source.parent.mkdir(parents=True)
    source.write_bytes(b'fixed')
    old = b'previous defect'
    case = dict(module.REGRESSIONS[0], files={
        'engine-rust/src/models.rs': hashlib.sha256(old).hexdigest()}, companions={})
    monkeypatch.setattr(module, 'REGRESSIONS', (case,))
    monkeypatch.setattr(module, 'sys', SimpleNamespace(platform='win32'))
    monkeypatch.setattr(module, 'require_hosted_source', lambda root: 'a' * 40)
    monkeypatch.setattr(module, 'git', lambda *args: old)
    monkeypatch.setattr(module.subprocess, 'run', lambda *args, **kwargs: None)
    monkeypatch.setitem(sys.modules, 'baseline_source', SimpleNamespace(ENVIRONMENT='NUMERA_ORACLE_SOURCE_ROOT'))
    monkeypatch.setitem(sys.modules, 'run_owned', SimpleNamespace(run=lambda *args: 101))
    logger = logging.Logger('compiler-returned-proof-fixture', level=logging.INFO)
    with pytest.raises(ValueError, match='Historical native regression did not fail at its required production assertion'):
        module.prove(tmp_path, tmp_path / 'original', logger)
    assert source.read_bytes() == b'fixed' and logger.filters == [] and logger.handlers == []
