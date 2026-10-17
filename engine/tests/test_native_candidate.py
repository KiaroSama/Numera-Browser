"""Candidate packaging is explicit, hash-bound and cannot select the product runtime."""
from __future__ import annotations

import importlib.util
import json
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[2]
pytestmark = pytest.mark.timeout(10)


def load(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / f'scripts/migration/{name}.py')
    module = importlib.util.module_from_spec(spec)
    import sys
    sys.path.insert(0, str(ROOT / 'scripts/migration'))
    try:
        spec.loader.exec_module(module)
    finally:
        sys.path.remove(str(ROOT / 'scripts/migration'))
    return module


def test_candidate_target_is_explicit_and_not_part_of_default_builds():
    gate = load('verify_default')
    target = {'name': 'numera-native-candidate', 'kind': ['bin'],
              'src_path': str(ROOT / 'engine-rust/examples/native_candidate.rs'),
              'required-features': ['native-candidate'], 'test': False}
    package = {'name': 'numera-core', 'features': {'default': [], 'native-candidate': []},
               'targets': [target]}
    gate.verify_candidate_target({'packages': [package]})
    for changed in (
        {**package, 'features': {'default': ['native-candidate'], 'native-candidate': []}},
        {**package, 'targets': [{**target, 'required-features': []}]},
        {**package, 'targets': [target, {**target, 'name': 'unexpected'}]},
        {**package, 'targets': [{**target, 'src_path': str(ROOT / 'engine-rust/src/main.rs')}]},
    ):
        with pytest.raises(ValueError, match='candidate'):
            gate.verify_candidate_target({'packages': [changed]})


def test_cargo_report_requires_success_and_one_current_enabled_binary(tmp_path):
    stage = load('candidate_stage')
    binary = ROOT / 'engine-rust/target/fixture-candidate'
    artifact = {'reason': 'compiler-artifact', 'manifest_path': str(ROOT / 'engine-rust/Cargo.toml'),
                'target': {'name': stage.BINARY, 'kind': ['bin']},
                'features': ['native-candidate'], 'profile': {'test': False},
                'executable': str(binary)}
    complete = {'reason': 'build-finished', 'success': True}
    report = tmp_path / 'cargo.jsonl'
    report.write_text('\n'.join(map(json.dumps, (artifact, complete))) + '\n', encoding='utf-8')
    assert stage.artifact(report) == binary.resolve()
    for rows in ([artifact], [artifact, {**complete, 'success': False}],
                 [artifact, artifact, complete],
                 [{**artifact, 'features': []}, complete],
                 [{**artifact, 'profile': {'test': True}}, complete],
                 [{**artifact, 'executable': str(tmp_path / 'foreign')}, complete]):
        report.write_text('\n'.join(map(json.dumps, rows)) + '\n', encoding='utf-8')
        with pytest.raises(ValueError):
            stage.artifact(report)


def test_candidate_layout_readback_detects_mutation_missing_and_unlisted_files(tmp_path, monkeypatch):
    stage = load('candidate_stage')
    source = tmp_path / 'source'
    source.mkdir()
    binary = source / stage.executable_name()
    binary.write_bytes(b'inert compiler artifact, never executed by this test')
    for name in stage.ADAPTERS:
        path = source / 'scripts/browser-adapter' / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text('# inert SDK fixture\n', encoding='utf-8')
    for name in stage.NOTICES:
        path = source / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text('fixture notice\n', encoding='utf-8')
    web = tmp_path / 'dashboard'
    (web / 'assets').mkdir(parents=True)
    (web / 'index.html').write_text('<html>fixture</html>', encoding='utf-8')
    (web / 'assets/fixture.js').write_text('/* fixture */', encoding='utf-8')
    identity = {'schema': 1, 'owner': 'KiaroSama', 'product': 'Numera Browser',
                'version': '0.36.0', 'source_sha': 'a' * 40, 'generation': 42,
                'application_runtime': 'native-candidate', 'acceptance': True}
    monkeypatch.setattr(stage, 'ROOT', source)
    destination = tmp_path / 'candidate with spaces'
    stage.stage(binary, web, destination, identity)
    assert stage.verify(destination) == identity
    adapter = destination / 'scripts/browser-adapter/adapter.py'
    original = adapter.read_bytes()
    for damage in ('changed', 'missing', 'extra'):
        if damage == 'changed':
            adapter.write_bytes(b'changed')
        elif damage == 'missing':
            adapter.unlink()
        else:
            (destination / 'unlisted.py').write_text('# not approved\n', encoding='utf-8')
        with pytest.raises(ValueError):
            stage.verify(destination)
        if damage == 'extra':
            (destination / 'unlisted.py').unlink()
        adapter.write_bytes(original)
    assert stage.verify(destination) == identity
    with pytest.raises(FileExistsError):
        stage.stage(binary, web, destination, identity)
    (web / 'index.html').unlink()
    with pytest.raises(ValueError, match='dashboard'):
        stage.stage(binary, web, tmp_path / 'incomplete', identity)


@pytest.mark.timeout(30)
def test_outer_timeout_settles_reported_separate_group(tmp_path):
    import sys
    from numera import proctree

    runner = load('run_owned')
    marker = tmp_path / 'child.json'
    program = '''import json,os,pathlib,subprocess,sys,threading
sys.path.insert(0,sys.argv[1])
from numera import proctree,unit_report
attempt=unit_report.begin_spawn()
child,tree=proctree.spawn([sys.executable,'-I','-S','-c','import threading; threading.Event().wait(60)'],stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
unit_report.record(tree,attempt=attempt)
pathlib.Path(sys.argv[2]).write_text(json.dumps({'pid':tree.pid,'pidToken':tree.token,'tree':tree.as_record()}),encoding='utf-8')
print('separate owned child published',flush=True)
threading.Event().wait(60)
'''
    try:
        with runner.run_log('candidate-timeout', tmp_path) as logger:
            with pytest.raises(TimeoutError):
                runner.run([sys.executable, '-I', '-u', '-c', program, str(ROOT / 'engine'), str(marker)],
                           10, 3, logger, report_units=True)
        record = json.loads(marker.read_text(encoding='utf-8'))
        assert not proctree.alive_tree(proctree.Tree.from_record(record)), \
            'Outer timeout left the separately grouped child alive'
    finally:
        if marker.is_file():
            record = json.loads(marker.read_text(encoding='utf-8'))
            assert proctree.end_tree(proctree.Tree.from_record(record), grace=1, kill_grace=3)


def test_outer_cleanup_rereads_units_published_while_stopping_a_producer(tmp_path, monkeypatch):
    import sys
    from types import SimpleNamespace
    from numera import unit_report

    runner = load('run_owned')
    first = {'pid': 101, 'pidToken': 1, 'tree': {'kind': 'pgid', 'group': 101}}
    late = {'pid': 102, 'pidToken': 2, 'tree': {'kind': 'pgid', 'group': 102}}
    calls = []
    report = tmp_path / 'jobs/verification.units'
    report.parent.mkdir()
    report.touch()
    unit_report.initialize(report)
    monkeypatch.setenv(unit_report.UNITS_ENV, str(report))
    attempt = unit_report.begin_spawn()
    unit_report.record(SimpleNamespace(pid=101, token=1, as_record=lambda: first['tree']), attempt=attempt)

    def end(tree, **kwargs):
        calls.append(tree.pid)
        if tree.pid == 101 and 102 not in calls and calls.count(101) == 1:
            attempt = unit_report.begin_spawn()
            unit_report.record(SimpleNamespace(pid=102, token=2, as_record=lambda: late['tree']), attempt=attempt)
        return True

    monkeypatch.setattr(runner, 'end_tree', end)
    assert runner.settle_reported(report, True, 10) == (True, 2, True)
    assert 102 in calls, 'Late SDK ownership was omitted from outer cleanup'


def test_candidate_failure_diagnostics_keep_only_bounded_safe_events(tmp_path, monkeypatch):
    import logging

    candidate = load('candidate_acceptance')
    messages = []

    class Capture(logging.Handler):
        def emit(self, record):
            messages.append(record.getMessage())

    logger = logging.Logger('candidate-diagnostics-fixture')
    logger.addHandler(Capture())
    logs = tmp_path / 'logs'
    logs.mkdir()
    prefix = '[2026-10-17 16:54:37 UTC] [INFO] [numera.browser-adapter] '
    (logs / 'browser-adapter_fixture.log').write_text(
        prefix + 'Camoufox SDK import complete\n'
        + prefix + 'Operation failed operation=open type=FileNotFoundError\n'
        + prefix + 'Operation failed operation=open type=private_fixture_value\n'
        + prefix + 'secret=private_fixture_value\n'
        + 'unstructured private_fixture_value\n', encoding='utf-8')
    (logs / 'native-cli_fixture.log').write_text(
        '[2026-10-17 16:54:37 UTC] [INFO] [entry] Startup\n', encoding='utf-8')
    (logs / 'unapproved.log').write_text(prefix + 'Camoufox persistent context entered\n', encoding='utf-8')
    (logs / 'browser-adapter_large.log').write_bytes(b'x' * 65537)
    linked = logs / 'browser-adapter_link.log'
    linked.write_text(prefix + 'Camoufox persistent context entered\n', encoding='utf-8')
    original = Path.is_symlink
    monkeypatch.setattr(Path, 'is_symlink', lambda path: path == linked or original(path))
    candidate.failure_diagnostics(logs, logger)
    text = '\n'.join(messages)
    assert 'Camoufox SDK import complete' in text
    assert 'operation=open type=FileNotFoundError' in text
    assert 'type=unknown' in text
    assert 'Startup' in text
    assert 'private_fixture_value' not in text
    assert 'persistent context entered' not in text
    assert 'omitted=3' in text
    assert len(messages) <= 65
    assert (logs / 'browser-adapter_large.log').stat().st_size == 65537
    messages.clear()
    (logs / 'browser-adapter_fixture.log').write_text(
        (prefix + 'Camoufox SDK import complete\n') * 100, encoding='utf-8')
    candidate.failure_diagnostics(logs, logger)
    assert len(messages) == 65, 'Candidate diagnostics exceeded their event bound'
