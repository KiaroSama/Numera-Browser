"""Ownership, output and report controls against the current receipt protocol.

The existing scenarios are retained. Zero units now require an initialized
receipt, and a report failure before spawning must create no child at all.
Observations precede teardown; fixture cleanup is never acceptance evidence.
"""
import json
import os
import subprocess
import sys
import threading
import time
from pathlib import Path

import pytest
from numera import proctree, unit_report
from numera.server import helpers
from numera.server.helper_streams import Streams

BUDGET_OK = 60.0
BUDGET_WEDGED = 3.0


@pytest.fixture
def manager():
    made = helpers.HelperManager()
    yield made
    for helper in made.live():
        made.settle(helper, deadline=2.0)
        if helper.proc is None or helper.proc.poll() is not None:
            if helper.streams is not None:
                helper.streams.close(1.0)
            made.fail_start(helper)


@pytest.fixture
def scripts(tmp_path):
    start = '''import subprocess, sys, time
from numera import proctree
proc, tree = proctree._spawn_leader(subprocess.Popen,
    [sys.executable, '-c', 'import time; time.sleep(120)'], (),
    {'stdout': sys.stdout, 'stderr': sys.stderr, 'close_fds': False})
'''
    bodies = {
        'outlives': start + "sys.stdout.write('TRAILING-NO-NEWLINE');sys.stdout.flush()\n",
        'wedged_driver': start + "time.sleep(120)\n",
        'same_group': "import subprocess,sys\nsubprocess.Popen([sys.executable,'-c','import time;time.sleep(120)'],stdout=sys.stdout,stderr=sys.stderr,close_fds=False)\nsys.stdout.write('SAME-GROUP-TAIL');sys.stdout.flush()\n",
        'quick': "print('ok')\n",
        'unhealthy': "import sys\nsys.stdout.write('{\"ok\": false}');sys.exit(3)\n",
    }
    paths = {}
    for name, body in bodies.items():
        path = tmp_path / (name + '.py')
        prefix = 'import sys;sys.path.insert(0,' + repr(str(Path(proctree.__file__).resolve().parents[1])) + ')\n'
        path.write_text(prefix + body, encoding='utf-8')
        paths[name] = path
    return paths


def run(manager, path, timeout=BUDGET_OK):
    return helpers.run_helper([sys.executable, str(path)], timeout, manager=manager)


def running(pid, token):
    stat = Path(f'/proc/{pid}/stat')
    if os.name != 'nt' and stat.exists():
        return stat.read_text(encoding='utf-8').rsplit(')', 1)[1].split()[0] != 'Z'
    return proctree.alive(pid, token)


def assert_processes_ended(trees, ends_at):
    pending = list(trees)
    while pending:
        pending = [tree for tree in pending if running(tree.pid, tree.token)]
        if not pending:
            return
        remaining = ends_at - time.monotonic()
        assert remaining > 0, 'Identified helper processes survived the existing deadline'
        time.sleep(min(.01, remaining))


def test_a_driver_holding_both_writers_does_not_truncate_the_answer(scripts, manager):
    result = run(manager, scripts['outlives'])
    assert result.returncode == 0 and result.stdout == 'TRAILING-NO-NEWLINE'
    assert not manager.live() and not manager.stragglers()


def test_the_driver_is_gone_according_to_the_operating_system(scripts, manager, monkeypatch):
    seen = []
    original = helpers.read_units
    def read(path):
        report = original(path)
        seen.extend(report.trees)
        return report
    monkeypatch.setattr(helpers, 'read_units', read)
    ends_at = time.monotonic() + BUDGET_OK + helpers.CLEANUP_BUDGET
    run(manager, scripts['outlives'])
    assert seen, 'the relevant driver was never started/reported'
    assert manager.live() == [], 'helper ownership was not settled before observation'
    assert_processes_ended(seen, ends_at)
    assert time.monotonic() <= ends_at, 'process observation exceeded the existing deadline'


@pytest.mark.parametrize('disappears,budget', [(True, .05), (False, .05), (False, .005), (False, 0)])
def test_process_observation_retains_identity_and_rejects_survivors(monkeypatch, disappears, budget):
    from types import SimpleNamespace

    module = sys.modules[__name__]
    clock = [0.0]
    calls, waits = [], []
    tree = proctree.Tree(pid=123, token=456)

    def alive(pid, token):
        calls.append((pid, token))
        return not disappears or clock[0] < .02

    def sleep(seconds):
        assert 0 < seconds <= budget - clock[0]
        waits.append(seconds)
        clock[0] += seconds

    def forbidden(*args, **kwargs):
        raise AssertionError('Observation must not terminate or settle a process')

    monkeypatch.setattr(module, 'os', SimpleNamespace(name='nt'))
    monkeypatch.setattr(module, 'time', SimpleNamespace(monotonic=lambda: clock[0], sleep=sleep))
    monkeypatch.setattr(proctree, 'alive', alive)
    monkeypatch.setattr(proctree, 'end_tree', forbidden)
    monkeypatch.setattr(helpers.HelperManager, 'settle', forbidden)
    monkeypatch.setattr(subprocess.Popen, 'kill', forbidden)
    if disappears:
        assert_processes_ended([tree], budget)
        assert clock[0] == pytest.approx(.02)
    else:
        with pytest.raises(AssertionError, match='Identified helper processes survived'):
            assert_processes_ended([tree], budget)
    assert calls and set(calls) == {(123, 456)}
    assert bool(waits) == (budget > 0)
    assert clock[0] <= budget


def test_a_grandchild_in_the_same_group_is_the_control(scripts, manager):
    result = run(manager, scripts['same_group'])
    assert result.stdout == 'SAME-GROUP-TAIL' and not manager.live()


def test_a_wedged_helper_with_a_driver_times_out_within_the_declared_bound(scripts, manager):
    start = time.monotonic()
    with pytest.raises(helpers.HelperTimeout):
        run(manager, scripts['wedged_driver'], BUDGET_WEDGED)
    assert time.monotonic() - start < BUDGET_WEDGED + helpers.CLEANUP_BUDGET + 1
    assert not manager.live()


@pytest.mark.parametrize('name,code,text', [('quick', 0, 'ok\n'), ('unhealthy', 3, '{"ok": false}')])
def test_a_plain_helper_returns_its_whole_output(scripts, manager, name, code, text):
    result = run(manager, scripts[name])
    assert (result.returncode, result.stdout) == (code, text)
    assert not manager.live()


def test_a_reader_that_cannot_reach_the_end_says_so():
    read_fd, write_fd = os.pipe()
    pipe = os.fdopen(read_fd, 'rb')
    class Proc:
        stdout, stderr = pipe, None
    streams = Streams(Proc())
    try:
        os.write(write_fd, b'PARTIAL')
        out, err, complete = streams.collect(.4)
        assert (out, err, complete) == ('PARTIAL', '', False)
        assert streams.unfinished() == ['stdout']
    finally:
        os.close(write_fd)
        assert streams.close() == []


def test_a_closed_writer_lets_the_reader_finish():
    read_fd, write_fd = os.pipe()
    pipe = os.fdopen(read_fd, 'rb')
    class Proc:
        stdout, stderr = pipe, None
    streams = Streams(Proc())
    try:
        os.write(write_fd, b'WHOLE')
        os.close(write_fd)
        assert streams.collect(2) == ('WHOLE', '', True)
        assert streams.unfinished() == []
    finally:
        assert streams.close() == []


def test_no_reader_thread_outlives_the_call(scripts, manager):
    before = set(threading.enumerate())
    run(manager, scripts['outlives'])
    assert not [t for t in threading.enumerate() if t not in before and t.name.startswith('helper-')]


@pytest.mark.parametrize('body,why', [('{"pid": 1,\n', 'truncated'), ('garbage\n', 'invalid JSON'),
    ('{"tree":{"kind":"job","group":"g"}}\n', 'missing pid')])
def test_a_damaged_report_is_not_an_empty_one(tmp_path, body, why):
    path = tmp_path / 'units.jsonl'
    path.write_text(body, encoding='utf-8')
    report = helpers.read_units(path)
    assert not report.complete, why
    assert 'units.jsonl' in report.problem


def test_a_missing_report_is_not_an_empty_one(tmp_path):
    report = helpers.read_units(tmp_path / 'absent')
    assert not report.complete and 'missing' in report.problem


def test_no_report_path_at_all_is_not_an_empty_one():
    report = helpers.read_units(None)
    assert not report.complete and report.problem


def test_an_empty_report_is_a_real_answer(tmp_path):
    # Empty ownership is valid; an erased, uninitialized file is not its proof.
    path = tmp_path / 'units.jsonl'
    path.touch()
    assert not helpers.read_units(path).complete
    unit_report.initialize(path)
    report = helpers.read_units(path)
    assert (report.trees, report.complete, report.problem) == ([], True, None)


def test_several_units_all_come_back(tmp_path):
    path = tmp_path / 'units.jsonl'
    path.write_text(''.join(json.dumps({'pid': pid, 'pidToken': None,
        'tree': {'kind': 'job', 'group': f'g{pid}'}}) + '\n'
        for pid in (11, 22, 33)), encoding='utf-8')
    report = helpers.read_units(path)
    assert report.complete and [tree.pid for tree in report.trees] == [11, 22, 33]


def test_a_damaged_report_strands_the_helper_rather_than_clearing_it(scripts, manager):
    run(manager, scripts['quick'])
    helper = manager.admit([sys.executable, '-c', 'pass'])
    manager.child_env(helper)
    # Resolve the no-child fixture so this tests damaged evidence, not startup.
    manager.attach(helper, None, None)
    helper.units_path.write_text('{"pid": 4242,\n', encoding='utf-8')
    assert not manager.settle(helper, deadline=2)
    assert helper.state == helpers.STRANDED and helper in manager.live()
    assert helper.units_problem and helper.straggler


def test_a_report_that_cannot_be_written_ends_the_launch(tmp_path, monkeypatch):
    monkeypatch.setenv(proctree.UNITS_ENV, str(tmp_path / 'missing' / 'units'))
    started = []
    original = subprocess.Popen
    def watched(*args, **kwargs):
        proc = original(*args, **kwargs)
        started.append(proc)
        return proc
    with pytest.raises(proctree.UnitReportFailed):
        proctree._spawn_leader(watched,
            [sys.executable, '-c', 'import time;time.sleep(120)'], (),
            {'stdout': subprocess.DEVNULL, 'stderr': subprocess.DEVNULL})
    assert started == [], 'an unwritable pre-spawn receipt must create no process'
