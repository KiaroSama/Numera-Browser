"""Entry acceptance preserves caller ownership and retains failed fixtures."""
from __future__ import annotations

import importlib.util
import logging
from pathlib import Path
import subprocess
from types import SimpleNamespace

import pytest

ROOT = Path(__file__).resolve().parents[2]
pytestmark = pytest.mark.timeout(10)


def load(monkeypatch):
    monkeypatch.syspath_prepend(str(ROOT / 'scripts/migration'))
    import baseline_source
    monkeypatch.setattr(baseline_source, 'source_root', lambda: ROOT)
    spec = importlib.util.spec_from_file_location(
        'entry_acceptance_fixture', ROOT / 'scripts/migration/native_entry_acceptance.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


@pytest.mark.parametrize('reported', [False, True], ids=['no-parent', 'parent-report'])
@pytest.mark.parametrize('mode', ['normal', 'input', 'communicate', 'decode', 'settlement', 'record', 'wait', 'cleanup'])
def test_execute_preserves_child_environment_and_settles_only_its_direct_tree(tmp_path, monkeypatch, mode, reported):
    helper = load(monkeypatch)
    from numera.proctree import Tree

    calls = []
    child_report = tmp_path / 'deliberate-ancestor.units'
    header = b'{"version":2,"event":"header"}\n'
    child_report.write_bytes(header)
    argv = ['inert-candidate', 'list']
    environment = {'fixture': 'value', 'NUMERA_HELPER_UNITS': str(child_report)}
    parent_report = tmp_path / 'outer.units'
    if reported:
        parent_report.write_bytes(header)
        monkeypatch.setenv(helper.unit_report.UNITS_ENV, str(parent_report))
    else:
        monkeypatch.delenv(helper.unit_report.UNITS_ENV, raising=False)
    data = b'y\n' if mode == 'input' else None
    tree = Tree(pid=42, token=43, group=42, kind='pgid')

    class Pipe:
        def close(self):
            calls.append('close')

    class Process:
        returncode = 7
        stdin, stdout, stderr = Pipe(), Pipe(), Pipe()

        def communicate(self, supplied, timeout):
            assert supplied == data and timeout == 15
            calls.append('communicate')
            if mode == 'communicate':
                raise subprocess.TimeoutExpired(argv, timeout)
            return (b'\xff' if mode == 'decode' else 'Fixture α'.encode('utf-8')), b''

        def wait(self, timeout):
            assert timeout == 5
            calls.append('wait')
            if mode == 'wait':
                raise subprocess.TimeoutExpired(argv, timeout)
            return self.returncode

    def spawn(arguments, **options):
        assert arguments is argv and options['env'] is environment
        assert options['stdin'] == (subprocess.PIPE if data is not None else subprocess.DEVNULL)
        assert options['stdout'] == options['stderr'] == subprocess.PIPE
        calls.append('spawn')
        return Process(), tree

    def end_tree(owned, **options):
        assert owned is tree and options == {'grace': 1, 'kill_grace': 3}
        calls.append('end-tree')
        if mode == 'cleanup':
            raise OSError('Synthetic cleanup query failed')
        return mode != 'settlement'

    record = helper.unit_report.record

    def record_tree(owned, *, attempt):
        assert owned is tree and bool(attempt) == reported
        calls.append('record')
        if mode == 'record':
            raise RuntimeError('Synthetic receipt failure')
        record(owned, attempt=attempt)

    monkeypatch.setattr(helper, 'spawn', spawn)
    monkeypatch.setattr(helper, 'end_tree', end_tree)
    monkeypatch.setattr(helper.unit_report, 'record', record_tree)
    errors = {'communicate': subprocess.TimeoutExpired, 'decode': UnicodeDecodeError,
              'settlement': RuntimeError, 'record': RuntimeError,
              'wait': subprocess.TimeoutExpired, 'cleanup': OSError}
    if mode in errors:
        with pytest.raises(errors[mode]):
            helper.execute(argv, environment, data)
    else:
        assert helper.execute(argv, environment, data) == (7, 'Fixture α', '')
    expected = ['spawn', 'record'] + ([] if mode == 'record' else ['communicate'])
    expected += ['end-tree'] + ([] if mode == 'cleanup' else ['wait']) + ['close'] * 3
    assert calls == expected
    assert environment == {'fixture': 'value', 'NUMERA_HELPER_UNITS': str(child_report)}
    assert child_report.read_bytes() == header, 'Executor replaced or reclaimed its child-selected ancestor'
    if reported:
        receipt = helper.unit_report.read(parent_report)
        assert receipt.complete == (mode != 'record')
        assert receipt.trees == ([] if mode == 'record' else [tree])


@pytest.mark.parametrize('reported', [False, True], ids=['no-parent', 'parent-report'])
@pytest.mark.parametrize('failure', [False, True])
def test_main_removes_only_successful_entry_fixtures(tmp_path, monkeypatch, failure, reported):
    helper = load(monkeypatch)
    temporary = tmp_path / 'owned-entry'
    temporary.mkdir()
    marker = temporary / 'owner.json'
    marker.write_bytes(b'owned evidence')
    report = tmp_path / 'outer.units'
    if reported:
        report.write_bytes(b'{"version":2,"event":"header"}\n')
        monkeypatch.setenv(helper.unit_report.UNITS_ENV, str(report))
    else:
        monkeypatch.delenv(helper.unit_report.UNITS_ENV, raising=False)
    monkeypatch.setattr(helper.tempfile, 'mkdtemp', lambda **kwargs: str(temporary))

    class Store:
        def __init__(self, root):
            self.root = root

        def save(self, profile):
            directory = self.root / 'profiles'
            directory.mkdir(parents=True)
            (directory / 'fixture.json').write_text('{"updated_at":1000}', encoding='utf-8')

    monkeypatch.setattr(helper, 'ProfileStore', Store)
    monkeypatch.setattr(helper, 'Profile', lambda **kwargs: SimpleNamespace(**kwargs))
    monkeypatch.setattr(helper, 'generate', lambda **kwargs: {})

    def execute(argv, environment, *args):
        assert environment.get(helper.unit_report.UNITS_ENV) == (str(report) if reported else None)
        if failure:
            raise RuntimeError('Synthetic unsettled entry')
        return 0, '{}', ''

    monkeypatch.setattr(helper, 'execute', execute)
    for name in ('confirmation_acceptance', 'replay_rollback', 'configured_lock_acceptance'):
        monkeypatch.setattr(helper, name, lambda *args: None)
    logger = logging.getLogger('entry-retention-fixture')
    if failure:
        with pytest.raises(RuntimeError, match='Synthetic unsettled entry'):
            helper.main(tmp_path / 'inert-candidate', logger)
        assert marker.read_bytes() == b'owned evidence'
    else:
        assert helper.main(tmp_path / 'inert-candidate', logger) == 0
        assert not temporary.exists()


@pytest.mark.parametrize('damage', [
    None, 'metadata', 'missing-command', 'missing-sdk', 'foreign-sdk', 'same-command',
    'command-token', 'sdk-token', 'sdk-group', 'dead-sdk', 'pending', 'malformed',
])
def test_detached_sdk_receipts_match_actual_command_and_session_claim(tmp_path, monkeypatch, damage):
    import json
    from numera.proctree import Tree

    load(monkeypatch)
    spec = importlib.util.spec_from_file_location(
        'job_acceptance_fixture', ROOT / 'scripts/migration/native_job_acceptance.py')
    helper = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(helper)
    root = tmp_path / 'store'
    (root / 'jobs').mkdir(parents=True)
    (root / 'sessions').mkdir()
    command = Tree(pid=41, token=410, group=41, kind='pgid')
    sdk = Tree(pid=42, token=420, group=42, kind='pgid')
    foreign = Tree(pid=43, token=430, group=43, kind='pgid')
    job = {'id': 'fixture-job', 'pid': command.pid, 'pidToken': command.token,
           'tree': command.as_record()}
    claim = {'pid': sdk.pid, 'token': sdk.token, 'tree': sdk.as_record()}
    trees = [command, sdk]
    if damage == 'metadata':
        trees.insert(1, foreign)
    elif damage == 'missing-command':
        trees.remove(command)
    elif damage == 'missing-sdk':
        trees.remove(sdk)
    elif damage == 'foreign-sdk':
        trees = [command, foreign]
    elif damage == 'same-command':
        claim = {'pid': command.pid, 'token': command.token, 'tree': command.as_record()}
    elif damage == 'command-token':
        job['pidToken'] += 1
    elif damage == 'sdk-token':
        claim['token'] += 1
    elif damage == 'sdk-group':
        claim['tree'] = {'kind': 'job', 'group': 'different-owned-tree'}
    records = [{'version': 2, 'event': 'header'}]
    for index, tree in enumerate(trees):
        records += [{'event': 'pending', 'attempt': str(index)},
                    {'event': 'unit', 'attempt': str(index), 'pid': tree.pid,
                     'pidToken': tree.token, 'tree': tree.as_record()}]
    if damage == 'pending':
        records.append({'event': 'pending', 'attempt': 'interrupted'})
    if damage == 'malformed':
        records[-1]['pidToken'] = None
    receipt = root / 'jobs/fixture-job.units'
    receipt.write_text(''.join(json.dumps(row) + '\n' for row in records), encoding='utf-8')
    claim_path = root / 'sessions/fixture-profile.json'
    claim_path.write_text(json.dumps(claim), encoding='utf-8')
    before = receipt.read_bytes(), claim_path.read_bytes()
    observed = []

    def alive(tree):
        observed.append(tree)
        return tree == sdk and damage != 'dead-sdk'

    monkeypatch.setattr(helper.proctree, 'alive_tree', alive)
    if damage in (None, 'metadata'):
        assert helper.sdk_units(root, job, 'fixture-profile') == trees
        assert observed == [sdk]
    else:
        with pytest.raises(AssertionError, match='ownership|SDK'):
            helper.sdk_units(root, job, 'fixture-profile')
    assert (receipt.read_bytes(), claim_path.read_bytes()) == before
