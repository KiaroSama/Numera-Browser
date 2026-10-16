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


@pytest.mark.parametrize('failure', [None, 'ordinary', 'unproved'])
def test_problem_block_notices_identify_each_admitted_module_before_execution(tmp_path, monkeypatch, failure):
    from contextlib import contextmanager
    import json
    from types import SimpleNamespace

    module = runner(monkeypatch)
    (tmp_path / 'engine').mkdir()
    monkeypatch.setattr(module, 'ROOT', tmp_path)
    names = [f'test_{index:02}.py' for index in range(37)]
    names[18:27] = ['test_profile_schema.py', 'test_proxy_removal_intent.py', 'test_public_surface.py',
                    'test_release_distribution.py', 'test_resilient_and_guarded.py', 'test_rotate_transport.py',
                    'test_server_api.py', 'test_session_tree_mandatory.py', 'test_startup_resources.py']
    files = list(map(Path, names))
    monkeypatch.setattr(module, 'python_sources', lambda *args: files)
    clock, messages, calls = [0.0], [], []
    monkeypatch.setattr(module, 'time', SimpleNamespace(monotonic=lambda: clock[0]))
    logger = SimpleNamespace(info=lambda fmt, *args: messages.append(fmt % args), error=lambda *args: None)

    @contextmanager
    def log(*args):
        yield logger

    monkeypatch.setattr(module, 'run_log', log)
    output = tmp_path / 'build/source-windows-1'
    output.mkdir(parents=True)
    checkpoint = output / 'checkpoint.json'
    state = dict(end=600, next=2, blocks=4, modules=names)
    checkpoint.write_text(json.dumps(state), encoding='utf-8')

    def run(command, wall, idle, log):
        name = Path(command[-1]).name
        # This must exist before spawn or a blocked module can erase its identity.
        notices = [message for message in messages if message.startswith('##[notice]')]
        assert notices, f'{name} started without a timeline notice'
        notice = notices[-1]
        assert notice.startswith(f'##[notice]Source block starting modules={name} remaining=')
        assert f'remaining={600 - 10 * len(calls):.1f}s' in notice
        assert wall == 600 - 10 * len(calls) and idle == 120
        previous = ','.join(f'{old}:{int(failure == "ordinary" and i == 0)}'
                            for i, old in enumerate(calls)) or 'none'
        assert notice.endswith('settled=' + previous)
        calls.append(name)
        clock[0] += 10
        if failure == 'unproved' and len(calls) == 3:
            raise RuntimeError('fixture cleanup unproved')
        return int(failure == 'ordinary' and len(calls) == 1)

    monkeypatch.setattr(module, 'run', run)
    monkeypatch.chdir(tmp_path)
    if failure == 'unproved':
        with pytest.raises(RuntimeError, match='cleanup unproved'):
            module.verify(1, 4, logger, block=2, blocks=4)
        assert calls == names[18:21]
        assert json.loads(checkpoint.read_text(encoding='utf-8')) == state
    else:
        assert module.verify(1, 4, logger, block=2, blocks=4) == int(failure == 'ordinary')
        assert calls == names[18:27]
        assert json.loads(checkpoint.read_text(encoding='utf-8'))['next'] == 3
    notices = [message for message in messages if message.startswith('##[notice]')]
    assert len(notices) == len(calls) + 1 <= 10
    if failure == 'unproved':
        assert notices[-1].startswith('##[notice]Source module unproved name=test_public_surface.py type=RuntimeError')
    else:
        assert notices[-1] == '##[notice]Source block settled modules=' + ','.join(
            f'{name}:{int(failure == "ordinary" and i == 0)}' for i, name in enumerate(calls))
    rows = [json.loads(line) for line in (output / 'progress.jsonl').read_text(encoding='utf-8').splitlines()]
    assert [row['module'] for row in rows if row['phase'] == 'start'] == calls
    assert rows[-1]['phase'] == ('unproved' if failure == 'unproved' else 'finish')


@pytest.mark.parametrize('failure', [None, 'ordinary', 'cleanup', 'interrupted'])
def test_single_module_checkpoints_keep_one_deadline_and_stop_on_uncertain_cleanup(tmp_path, monkeypatch, failure):
    from contextlib import contextmanager
    import json
    from types import SimpleNamespace

    module = runner(monkeypatch)
    (tmp_path / 'engine').mkdir()
    monkeypatch.setattr(module, 'ROOT', tmp_path)
    files = [Path(f'test_{index:02}.py') for index in range(37)]
    monkeypatch.setattr(module, 'python_sources', lambda *args: files)
    clock, calls = [20.0], []
    monkeypatch.setattr(module, 'time', SimpleNamespace(monotonic=lambda: clock[0]))
    logger = SimpleNamespace(info=lambda *args: None, error=lambda *args: None)

    @contextmanager
    def log(*args):
        yield logger

    monkeypatch.setattr(module, 'run_log', log)
    output = tmp_path / 'build/source-windows-1'
    output.mkdir(parents=True)
    checkpoint = output / 'checkpoint.json'
    state = dict(end=600, next=2, blocks=4, modules=[path.name for path in files])
    checkpoint.write_text(json.dumps(state), encoding='utf-8')

    def run(command, wall, idle, log):
        name = Path(command[-1]).name
        saved = json.loads(checkpoint.read_text(encoding='utf-8'))
        assert saved['active'] == name, 'spawn began without a durable pending identity'
        calls.append((name, wall, idle))
        clock[0] += 10
        if failure == 'cleanup' and len(calls) == 3:
            raise RuntimeError('fixture cleanup unproved')
        if failure == 'interrupted' and len(calls) == 3:
            return 2
        return int(failure == 'ordinary' and len(calls) == 1)

    monkeypatch.setattr(module, 'run', run)
    monkeypatch.chdir(tmp_path)
    for index in range(9):
        if failure in ('cleanup', 'interrupted') and index == 2:
            with pytest.raises(RuntimeError):
                module.verify(1, 4, logger, block=2, blocks=4, module_index=index)
            assert json.loads(checkpoint.read_text(encoding='utf-8'))['active'] == files[20].name
            with pytest.raises(ValueError, match='checkpoint'):
                module.settled_exit(index)
            for rejected in (index, index + 1):
                with pytest.raises(ValueError, match='checkpoint'):
                    module.verify(1, 4, logger, block=2, blocks=4, module_index=rejected)
            with pytest.raises(ValueError, match='checkpoint'):
                module.verify(1, 4, logger, block=2, blocks=4)
            assert len(calls) == 3
            break
        # Protocol success follows cleanup, not the test verdict. The action
        # publishes settled only after its outer owner also exits successfully.
        assert module.verify(1, 4, logger, block=2, blocks=4, module_index=index) == 0
        assert module.settled_exit(index) == int(failure == 'ordinary' and index == 0)
        saved = json.loads(checkpoint.read_text(encoding='utf-8'))
        assert saved['end'] == 600 and saved['module_next'] == index + 1 and 'active' not in saved
        assert saved['next'] == (3 if index == 8 else 2)
        if index < 8:
            with pytest.raises(ValueError, match='checkpoint'):
                module.verify(1, 4, logger, block=2, blocks=4, module_index=index)
            with pytest.raises(ValueError, match='checkpoint'):
                module.verify(1, 4, logger, block=2, blocks=4, module_index=index + 2)
            with pytest.raises(ValueError, match='checkpoint'):
                module.verify(1, 4, logger, block=2, blocks=4)
        clock[0] += 5  # Completed immutable uploads consume the partition budget.
    assert [call[0] for call in calls] == [path.name for path in files[18:18 + len(calls)]]
    assert [call[1] for call in calls] == [580 - 15 * index for index in range(len(calls))]
    assert all(call[2] == 120 for call in calls)
    if failure in (None, 'ordinary'):
        assert saved['module_results'] == [dict(module=path.name, exit=int(failure == 'ordinary' and index == 0))
                                          for index, path in enumerate(files[18:27])]
        clock[0] = 601
        checkpoint.write_text(json.dumps(state), encoding='utf-8')
        with pytest.raises(TimeoutError):
            module.verify(1, 4, logger, block=2, blocks=4, module_index=0)
        assert len(calls) == 9


@pytest.mark.parametrize('damage', ['wrong-size', 'partial-json', 'wrong-inventory', 'bad-verdict'])
def test_module_checkpoint_refuses_ambiguous_saved_state_before_spawn(tmp_path, monkeypatch, damage):
    import json
    from types import SimpleNamespace

    module = runner(monkeypatch)
    (tmp_path / 'engine').mkdir()
    monkeypatch.setattr(module, 'ROOT', tmp_path)
    sources = [Path(f'test_{index:02}.py') for index in range(40 if damage == 'wrong-size' else 37)]
    monkeypatch.setattr(module, 'python_sources', lambda *args: sources)
    state = dict(end=600, next=2, blocks=4, modules=[path.name for path in sources])
    if damage == 'wrong-inventory': state['modules'][0] = 'test_foreign.py'
    if damage == 'bad-verdict':
        state.update(module_next=1, module_results=[dict(module=sources[18].name, exit=124)])
    output = tmp_path / 'build/source-windows-1'
    output.mkdir(parents=True)
    (output / 'checkpoint.json').write_text('{' if damage == 'partial-json' else json.dumps(state), encoding='utf-8')
    monkeypatch.setattr(module, 'run', lambda *args: pytest.fail('Ambiguous checkpoint started a child'))
    monkeypatch.setattr(module, 'time', SimpleNamespace(monotonic=lambda: 0))
    monkeypatch.chdir(tmp_path)
    with pytest.raises(ValueError):
        module.verify(1, 4, SimpleNamespace(info=lambda *args: None), block=2, blocks=4,
                      module_index=int(damage == 'bad-verdict'))


def test_module_checkpoint_action_keeps_failures_and_requires_previous_cleanup_and_upload():
    import copy
    import yaml

    path = ROOT / '.github/actions/source-module-checkpoints/action.yml'
    action = yaml.safe_load(path.read_text(encoding='utf-8'))
    module = coverage()
    module.check_source_module_action(action)
    assert action['runs']['using'] == 'composite'
    steps = action['runs']['steps']
    assert len(steps) == 18
    assert action['outputs']['settled']['value'] == (
        "${{ steps.module_8.outputs.settled == 'true' && steps.checkpoint_8.outcome == 'success' }}")
    for index in range(9):
        run, upload = steps[index * 2:index * 2 + 2]
        assert run['id'] == f'module_{index}' and run['shell'] == 'pwsh'
        command = run['run']
        owner = command.index('run_owned.py --wall 600 --idle 120')
        proof = command.index('if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }', owner)
        settled_read = command.index(f'--settled-index {index}', proof)
        read_proof = command.index('if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }', settled_read)
        verdict = command.index(f'$testExit = $state.module_results[{index}].exit', read_proof)
        publish = command.index('settled=true', verdict)
        assert owner < proof < settled_read < read_proof < verdict < publish < command.index('exit [int]$testExit')
        assert upload['id'] == f'checkpoint_{index}'
        assert upload['with']['name'] == f'source-windows-checkpoint-2-module-{index}'
    for damage in ('repeat', 'skip-proof', 'ignored-exit', 'read-owner', 'softened', 'artifact-collision', 'false-settled'):
        broken = copy.deepcopy(action)
        run, upload = broken['runs']['steps'][2:4]
        if damage == 'repeat': run['run'] = run['run'].replace('--module-index 1', '--module-index 0')
        elif damage == 'skip-proof': run['if'] = '${{ !cancelled() }}'
        elif damage == 'ignored-exit': run['run'] = run['run'].replace('exit [int]$testExit', 'exit 0')
        elif damage == 'read-owner': run['run'] = run['run'].replace('run_owned.py --wall 30 --idle 15', 'run_owned.py')
        elif damage == 'softened': run['continue-on-error'] = True
        elif damage == 'false-settled': broken['outputs']['settled']['value'] = '${{ true }}'
        else: upload['with']['name'] = 'source-windows-checkpoint-2-module-0'
        with pytest.raises(ValueError, match='module checkpoint'):
            module.check_source_module_action(broken)


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
