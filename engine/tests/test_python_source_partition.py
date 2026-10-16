"""Windows diagnostic partitions cover every source module exactly once."""
import importlib.util
from pathlib import Path
import xml.etree.ElementTree as ET

import pytest

ROOT = Path(__file__).resolve().parents[2]


def coverage():
    spec = importlib.util.spec_from_file_location('source_partition_coverage', ROOT / 'scripts/ci/suites.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def runner(monkeypatch):
    monkeypatch.syspath_prepend(str(ROOT / 'scripts/ci'))
    spec = importlib.util.spec_from_file_location('source_partition_runner', ROOT / 'scripts/ci/python_source.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_all_four_partitions_are_disjoint_complete_and_nonempty():
    module = coverage()
    expected = module.discover()['python']
    partitions = [module.python_sources(index, 4) for index in range(4)]
    actual = [path for part in partitions for path in part]
    assert all(partitions)
    assert len(actual) == len(set(actual)) == len(expected)
    assert set(actual) == set(expected)
    assert max(map(len, partitions)) - min(map(len, partitions)) <= 1
    assert module.python_sources() == expected


@pytest.mark.parametrize('index,count', [(-1, 4), (4, 4), (0, 0), (0, 5)])
def test_invalid_partition_never_becomes_an_empty_success(index, count):
    with pytest.raises(ValueError, match='partition'):
        coverage().python_sources(index, count)


@pytest.mark.parametrize('damage', [None, 'missing', 'foreign', 'duplicate', 'failure'])
def test_partition_report_requires_every_selected_module(tmp_path, monkeypatch, damage):
    module = coverage()
    files = [Path(f'test_{name}.py') for name in ('one', 'two', 'three', 'four')]
    monkeypatch.setattr(module, 'discover', lambda root: {'python': files})
    selected = module.python_sources(0, 2)
    suite = ET.Element('testsuite')
    for path in selected:
        ET.SubElement(suite, 'testcase', classname='tests.' + path.stem, name='test_contract')
    if damage == 'missing':
        suite.remove(suite[1])
    elif damage == 'foreign':
        suite[1].set('classname', 'tests.test_two')
    elif damage == 'duplicate':
        suite[1].set('classname', 'tests.test_one')
    elif damage == 'failure':
        ET.SubElement(suite[1], 'failure')
    report = tmp_path / 'report.xml'
    ET.ElementTree(suite).write(report, encoding='utf-8', xml_declaration=True)
    if damage:
        with pytest.raises(ValueError):
            module.check_report(report, 'python', shard=0, shards=2)
    else:
        assert module.check_report(report, 'python', shard=0, shards=2) == {
            'passed': 2, 'explicit_limits': 0, 'suites': 2}


@pytest.mark.parametrize('damage', [None, 'missing', 'extra'])
def test_report_directory_inventory_matches_the_partition(tmp_path, monkeypatch, damage):
    module = coverage()
    sources = [Path('test_one.py'), Path('test_two.py')]
    monkeypatch.setattr(module, 'discover', lambda root: {'python': sources})
    for source in sources:
        suite = ET.Element('testsuite')
        ET.SubElement(suite, 'testcase', classname='tests.' + source.stem, name='test_contract')
        ET.ElementTree(suite).write(tmp_path / (source.stem + '.xml'), encoding='utf-8')
    if damage == 'missing':
        (tmp_path / 'test_two.xml').unlink()
    elif damage == 'extra':
        (tmp_path / 'test_foreign.xml').write_text('<testsuite/>', encoding='utf-8')
    if damage:
        with pytest.raises(ValueError, match='inventory differs'):
            module.check_report(tmp_path, 'python')
    else:
        assert module.check_report(tmp_path, 'python')['suites'] == 2


def test_module_failure_is_collected_without_repeating_or_omitting_other_modules(tmp_path, monkeypatch):
    from contextlib import contextmanager
    import logging

    module = runner(monkeypatch)
    (tmp_path / 'engine').mkdir()
    monkeypatch.setattr(module, 'ROOT', tmp_path)
    files = [tmp_path / 'engine/tests' / f'test_{name}.py' for name in ('one', 'two')]
    monkeypatch.setattr(module, 'python_sources', lambda *args: files)
    clock = [0.0]
    monkeypatch.setattr(module, 'time', __import__('types').SimpleNamespace(monotonic=lambda: clock[0]))
    logger = logging.getLogger('source-partition-fixture')

    @contextmanager
    def log(*args):
        yield logger

    monkeypatch.setattr(module, 'run_log', log)
    calls = []

    def run(command, wall, idle, log):
        calls.append((command, wall, idle))
        clock[0] += 10
        return 1 if len(calls) == 1 else 0

    monkeypatch.setattr(module, 'run', run)
    monkeypatch.chdir(tmp_path)
    assert module.verify(0, 4, logger) == 1
    assert [call[0][-1] for call in calls] == list(map(str, files))
    assert [call[1] for call in calls] == [600, 590]
    assert all(call[2] == 120 and call[0][:3] == [module.sys.executable, '-m', 'pytest'] for call in calls)
    assert all('--junitxml=' in ' '.join(call[0]) for call in calls)
    import json
    rows = [json.loads(line) for line in (tmp_path / 'build/source-windows-0/progress.jsonl').read_text(
        encoding='utf-8').splitlines()]
    assert [(row['module'], row['phase'], row.get('exit')) for row in rows] == [
        ('test_one.py', 'start', None), ('test_one.py', 'finish', 1),
        ('test_two.py', 'start', None), ('test_two.py', 'finish', 0)]


def test_unproved_module_cleanup_stops_the_partition(tmp_path, monkeypatch):
    from contextlib import contextmanager
    import logging

    module = runner(monkeypatch)
    (tmp_path / 'engine').mkdir()
    monkeypatch.setattr(module, 'ROOT', tmp_path)
    monkeypatch.setattr(module, 'python_sources', lambda *args: [Path('test_first.py'), Path('test_second.py')])
    logger = logging.getLogger('source-partition-fixture')

    @contextmanager
    def log(*args):
        yield logger

    monkeypatch.setattr(module, 'run_log', log)
    calls = []

    def run(*args):
        calls.append(args)
        raise RuntimeError('fixture cleanup unproved')

    monkeypatch.setattr(module, 'run', run)
    monkeypatch.chdir(tmp_path)
    with pytest.raises(RuntimeError, match='cleanup unproved'):
        module.verify(0, 4, logger)
    assert len(calls) == 1
    import json
    rows = [json.loads(line) for line in (tmp_path / 'build/source-windows-0/progress.jsonl').read_text(
        encoding='utf-8').splitlines()]
    assert [row['phase'] for row in rows] == ['start', 'unproved']
    assert rows[-1]['module'] == 'test_first.py' and rows[-1]['error'] == 'RuntimeError'


def test_checkpoint_blocks_share_one_budget_and_never_repeat_modules(tmp_path, monkeypatch):
    from contextlib import contextmanager
    import json
    import logging
    from types import SimpleNamespace

    module = runner(monkeypatch)
    (tmp_path / 'engine').mkdir()
    monkeypatch.setattr(module, 'ROOT', tmp_path)
    files = [Path(f'test_{index}.py') for index in range(8)]
    monkeypatch.setattr(module, 'python_sources', lambda *args: files)
    clock = [0.0]
    monkeypatch.setattr(module, 'time', SimpleNamespace(monotonic=lambda: clock[0]))
    logger = logging.getLogger('source-block-fixture')

    @contextmanager
    def log(*args):
        yield logger

    monkeypatch.setattr(module, 'run_log', log)
    calls = []

    def run(command, wall, idle, log):
        calls.append((command[-1], wall))
        clock[0] += 10
        return int(command[-1] == 'test_0.py')

    monkeypatch.setattr(module, 'run', run)
    monkeypatch.chdir(tmp_path)
    for block in range(4):
        assert module.verify(1, 4, logger, block=block, blocks=4) == int(block == 0)
        clock[0] += 5  # Artifact time also consumes the one partition budget.
    assert [row[0] for row in calls] == list(map(str, files))
    assert [row[1] for row in calls] == [600, 590, 575, 565, 550, 540, 525, 515]
    state = json.loads((tmp_path / 'build/source-windows-1/checkpoint.json').read_text(encoding='utf-8'))
    assert state['next'] == 4 and state['end'] == 600
    with pytest.raises(ValueError, match='checkpoint'):
        module.verify(1, 4, logger, block=3, blocks=4)


def test_incomplete_or_expired_checkpoint_never_admits_another_block(tmp_path, monkeypatch):
    import json
    import logging
    from types import SimpleNamespace

    module = runner(monkeypatch)
    (tmp_path / 'engine').mkdir()
    monkeypatch.setattr(module, 'ROOT', tmp_path)
    monkeypatch.setattr(module, 'python_sources', lambda *args: [Path(f'test_{i}.py') for i in range(8)])
    monkeypatch.setattr(module, 'time', SimpleNamespace(monotonic=lambda: 601))
    output = tmp_path / 'build/source-windows-1'
    output.mkdir(parents=True)
    state = dict(end=600, next=0, blocks=4, modules=[f'test_{i}.py' for i in range(8)])
    path = output / 'checkpoint.json'
    path.write_text(json.dumps(state), encoding='utf-8')
    logger = logging.getLogger('source-expired-fixture')
    monkeypatch.chdir(tmp_path)
    with pytest.raises(ValueError, match='checkpoint'):
        module.verify(1, 4, logger, block=1, blocks=4)
    state['next'] = 1
    path.write_text(json.dumps(state), encoding='utf-8')
    with pytest.raises(TimeoutError):
        module.verify(1, 4, logger, block=1, blocks=4)


def test_durable_notice_blocks_cover_every_module_with_at_most_ten_annotations(monkeypatch):
    reporter = runner(monkeypatch)
    files = [Path(f'test_{index:03}.py') for index in range(37)]
    from types import SimpleNamespace
    messages = []
    logger = SimpleNamespace(info=lambda fmt, *args: messages.append(fmt % args))
    reported = [reporter.announce(files, index, logger) for index in range(len(files))]
    assert sum(reported) == len(messages) == 10
    assert all(message.startswith('##[notice]Source block starting modules=') for message in messages)
    assert [name for message in messages for name in message.split('modules=', 1)[1].split(',')] == [
        source.name for source in files]
