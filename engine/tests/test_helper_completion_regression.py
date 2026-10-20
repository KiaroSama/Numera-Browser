"""Prove helper termination, stream cancellation and deadline accounting."""
import json
import os
from pathlib import Path
import subprocess
import sys
import threading
import time

import pytest
from numera import proctree, unit_report
from numera.server import helper_streams, helpers


@pytest.fixture(autouse=True)
def tested_checkout_for_children(monkeypatch):
    """Child imports must resolve to the same checkout being tested."""
    engine = str(Path(proctree.__file__).resolve().parents[1])
    prior = os.environ.get("PYTHONPATH")
    monkeypatch.setenv("PYTHONPATH", engine + (os.pathsep + prior if prior else ""))


def stop(proc, streams=None):
    if proc.poll() is None:
        proc.kill()
    proc.wait(timeout=5)
    if streams is not None:
        streams.close(1.0)
    for pipe in (proc.stdin, proc.stdout, proc.stderr):
        if pipe is not None and not pipe.closed:
            pipe.close()


def test_stream_close_cancels_readers_without_waiting_for_live_writers():
    proc = subprocess.Popen([sys.executable, '-c', 'import time;time.sleep(30)'],
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    streams = helper_streams.Streams(proc)
    try:
        started = time.monotonic()
        retained = streams.close(.3)
        assert retained == [], 'reader cancellation must not depend on writer exit'
        assert time.monotonic() - started < .65
        assert not any(r.thread.is_alive() for r in streams.readers)
        assert not all(r.ended.is_set() for r in streams.readers), 'cancellation is not EOF'
        assert proc.poll() is None, 'closing local readers must not kill an unowned writer'
    finally:
        stop(proc, streams)


def test_partial_reader_start_unwinds_every_started_reader(monkeypatch):
    proc = subprocess.Popen([sys.executable, '-c', 'import time;time.sleep(30)'],
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    prior = set(threading.enumerate())
    original = threading.Thread.start
    def start(thread):
        if thread.name == 'helper-stderr':
            raise RuntimeError('fixture second-reader failure')
        return original(thread)
    try:
        with monkeypatch.context() as patch:
            patch.setattr(threading.Thread, 'start', start)
            with pytest.raises(RuntimeError, match='fixture'):
                helper_streams.Streams(proc)
        leaked = [t for t in threading.enumerate() if t not in prior and t.name.startswith('helper-')]
        assert leaked == [], 'failed construction retained a non-daemon reader'
    finally:
        stop(proc)
        for thread in set(threading.enumerate()) - prior:
            if thread.name.startswith('helper-'):
                thread.join(2)


def test_direct_settle_cannot_resolve_an_unstarted_reservation():
    manager = helpers.HelperManager()
    helper = manager.admit((sys.executable, '-c', 'pass'))
    manager.child_env(helper)
    try:
        assert manager.settle(helper, deadline=.05) is False
        assert helper in manager.live()
        assert not helper.resolved.is_set()
    finally:
        manager.fail_start(helper)


def test_end_all_uses_one_budget_across_all_helpers(monkeypatch):
    from types import SimpleNamespace
    manager = helpers.HelperManager()
    pending = [manager.admit(('fixture',)) for _ in range(3)]
    clock = [0.0]
    attempts = []

    class HeldLock:
        def acquire(self, *, timeout):
            attempts.append(timeout)
            clock[0] += timeout
            return False

    for helper in pending:
        manager.child_env(helper)
        manager.attach(helper, None, None)
        helper.finish = HeldLock()
    try:
        with monkeypatch.context() as patch:
            patch.setattr(helpers, 'time', SimpleNamespace(monotonic=lambda: clock[0]))
            left = manager.end_all(deadline=.08)
        assert left == pending
        assert attempts == pytest.approx([.08, 0, 0])
        assert clock[0] == pytest.approx(.08), 'each helper received a fresh budget'
        assert {id(helper) for helper in manager.live()} == {id(helper) for helper in pending}
        assert all(helper.straggler for helper in pending)
    finally:
        for helper in pending:
            manager.fail_start(helper)


def test_helper_stays_registered_until_stream_disposal(monkeypatch):
    manager = helpers.HelperManager()
    original = helper_streams.Streams.close
    seen = []
    def close(streams, *args, **kwargs):
        seen.append(len(manager.live()))
        return original(streams, *args, **kwargs)
    monkeypatch.setattr(helper_streams.Streams, 'close', close)
    answer = helpers.run_helper([sys.executable, '-c', 'print("complete")'], 5, manager=manager)
    assert answer.stdout == 'complete\n'
    assert seen and all(count == 1 for count in seen), 'registry release preceded stream disposal'
    assert manager.live() == []


@pytest.mark.parametrize('method', ['adopt', 'attach'])
def test_failure_before_binding_does_not_lose_the_spawned_child(monkeypatch, method):
    manager = helpers.HelperManager()
    processes = []
    spawn = proctree.spawn
    def capture(*args, **kwargs):
        proc, tree = spawn(*args, **kwargs)
        processes.append(proc)
        return proc, tree
    def fail(*args):
        raise RuntimeError('fixture pre-binding failure')
    monkeypatch.setattr(proctree, 'spawn', capture)
    monkeypatch.setattr(manager, method, fail)
    try:
        with pytest.raises(RuntimeError, match='fixture'):
            helpers.run_helper([sys.executable, '-c', 'import time;time.sleep(30)'], 5, manager=manager, cleanup=.5)
        assert processes and all(p.poll() is not None for p in processes)
        assert not any(t.name.startswith('helper-') for t in threading.enumerate())
        assert all(h.resolved.is_set() for h in manager.live())
    finally:
        for proc in processes:
            stop(proc)
        for helper in manager.live():
            manager.fail_start(helper)


def test_output_failure_is_retained_as_a_terminal_diagnostic(monkeypatch):
    manager = helpers.HelperManager()
    def fail(reader):
        reader.error = OSError('fixture read failure')
    monkeypatch.setattr(helper_streams._Reader, '_pump', fail)
    with pytest.raises(helpers.HelperFailed):
        helpers.run_helper([sys.executable, '-c', 'pass'], 5, manager=manager, cleanup=.5)
    live = manager.live()
    assert live and all(h.state == helpers.STRANDED for h in live)
    for helper in live:
        assert helper.proc.poll() is not None
        manager.fail_start(helper)


@pytest.mark.parametrize('damage', ['missing', 'malformed', 'utf8'])
def test_worker_exits_after_unresolved_report_without_supervisor_rescue(tmp_path, damage):
    # The negative fixture deliberately removes ownership evidence. The product
    # must return a bounded failure, keep the diagnostic and end its readers;
    # teardown removes the independently recorded fixture child afterwards.
    receipt = tmp_path / 'child.json'
    outcome = tmp_path / 'outcome.json'
    child_code = "import time;time.sleep(30)"
    helper_code = '''
import json,os,subprocess,sys,time
from pathlib import Path
from numera import proctree
p,t=proctree._spawn_leader(subprocess.Popen,[sys.executable,'-c',sys.argv[3]],(),{})
Path(sys.argv[1]).write_text(json.dumps({'pid':t.pid,'token':t.token,'group':t.group,'kind':t.kind,'reported_at':time.monotonic()}))
path=Path(os.environ[proctree.UNITS_ENV])
if sys.argv[2]=='missing':path.unlink()
elif sys.argv[2]=='utf8':path.write_bytes(b'\\xff')
else:path.write_text('17\\n')
'''
    worker_code = '''
import json,sys,time,threading
from pathlib import Path
from numera.server import helpers
m=helpers.HelperManager();start=time.monotonic()
phases=[]
settle=m._settle_locked
def measured_settle(helper,ends_at):
    begin=time.monotonic()
    def measure(name,method):
        def observed(budget):
            entered=time.monotonic()
            row={'phase':name,'budget':budget,'remaining_enter':ends_at-entered}
            try:
                return method(budget)
            finally:
                finished=time.monotonic()
                row.update(elapsed=finished-entered,remaining_exit=ends_at-finished)
                if name=='close':
                    row['readers']=[{'name':r.thread.name,'ident':r.thread.ident,
                        'alive':r.thread.is_alive(),'cancel':r.cancel.is_set(),
                        'eof':r.ended.is_set(),'pipe_closed':r.pipe.closed,
                        'error':type(r.error).__name__ if r.error else None}
                        for r in helper.streams.readers]
                phases.append(row)
        return observed
    if helper.streams is not None:
        for name in ('collect','close'):
            setattr(helper.streams,name,measure(name,getattr(helper.streams,name)))
    try:
        return settle(helper,ends_at)
    finally:
        phases.append({'phase':'settle','ends_at':ends_at,
                       'elapsed':time.monotonic()-begin})
m._settle_locked=measured_settle
try:
    helpers.run_helper([sys.executable,'-c',sys.argv[3],sys.argv[1],sys.argv[2],sys.argv[4]],5,manager=m,cleanup=.4)
    result={'error':None}
except helpers.HelperFailed as e:
    result={'error':type(e).__name__}
reported=json.loads(Path(sys.argv[1]).read_text())['reported_at']
result.update(elapsed=time.monotonic()-start,cleanup_elapsed=time.monotonic()-reported,retained=len(m.live()),readers=[t.name for t in threading.enumerate() if t.name.startswith('helper-')])
result['phases']=phases
Path(sys.argv[5]).write_text(json.dumps(result),encoding='utf-8')
'''
    worker = subprocess.Popen([sys.executable, '-c', worker_code, str(receipt), damage,
                               helper_code, child_code, str(outcome)], stdout=subprocess.PIPE,
                              stderr=subprocess.PIPE, env=dict(os.environ))
    rescued = False
    try:
        try:
            worker.wait(timeout=11)
        except subprocess.TimeoutExpired:
            rescued = True
        observed = json.loads(outcome.read_text()) if outcome.exists() else {}
    finally:
        if receipt.exists():
            record = json.loads(receipt.read_text())
            record.pop('reported_at', None)
            tree = proctree.Tree(**record)
            proctree.end_tree(tree, grace=.1, kill_grace=1)
        stop(worker)
    assert not rescued, 'the caller/worker could not exit without fixture rescue'
    assert observed.get('error') in ('HelperUnsettled', 'HelperOutputIncomplete')
    assert observed['elapsed'] < 6.4, observed
    assert observed['cleanup_elapsed'] < 1.0, observed
    assert observed['retained'] == 1, observed
    if observed['readers']:
        print(json.dumps({'readers': observed['readers'], 'phases': observed.get('phases', [])},
                         sort_keys=True), flush=True)
    assert observed['readers'] == [], observed


@pytest.mark.parametrize('failure', ['missing', 'malformed', 'utf8', 'tree', 'leader'])
def test_failed_settlement_preserves_cancellation_budget(monkeypatch, failure):
    from types import SimpleNamespace

    def check(manager):
        _assert_failed_settlement_budget(monkeypatch, failure, manager)

    import hashlib

    historical = (Path(__file__).parent / 'fixtures' /
                  'helper-settlement-before-cancellation.py.txt').read_bytes()
    assert hashlib.sha256(historical).hexdigest() == \
        '86e8bb8ade0f8dc57b44c7241ff8f13db7e7bd2b2416d68125e67595dcd17232'
    namespace = dict(vars(helpers))
    namespace['time'] = SimpleNamespace(monotonic=lambda: helpers.time.monotonic())
    exec(compile(historical, 'historical-helper-settlement', 'exec'), namespace)
    old = helpers.HelperManager()
    old._settle_locked = namespace['_settle_locked'].__get__(old)
    with pytest.raises(AssertionError, match='Failed settlement spent cancellation budget'):
        check(old)
    check(helpers.HelperManager())


def _assert_failed_settlement_budget(monkeypatch, failure, manager):
    from types import SimpleNamespace

    helper = manager.admit(('fixture',))
    manager.child_env(helper)
    report = helper.units_path
    if failure == 'missing':
        report.unlink()
    elif failure == 'malformed':
        report.write_text('17\n', encoding='utf-8')
    elif failure == 'utf8':
        report.write_bytes(b'\xff')
    if failure in ('tree', 'leader'):
        helper.tree = proctree.Tree(pid=123, token=456)
        helper.proc = SimpleNamespace(poll=lambda: None if failure == 'leader' else 0)
        monkeypatch.setattr(proctree, 'end_session', lambda *args, **kwargs: 0)
        monkeypatch.setattr(proctree, 'end_tree', lambda *args, **kwargs: failure != 'tree')
    writers = []
    pipes = []
    clock = [0.0]
    close_budgets = []
    join = threading.Thread.join

    def scheduled_join(thread, timeout=None):
        if (thread.name.startswith('helper-') and timeout and
                not all(reader.cancel.is_set() for reader in helper.streams.readers)):
            # Model the hosted timed-join overrun, not a successful read or exit.
            clock[0] += timeout + .125
            return join(thread, 0)
        return join(thread, timeout)

    try:
        for _ in range(2):
            read_fd, write_fd = os.pipe()
            writers.append(write_fd)
            pipes.append(os.fdopen(read_fd, 'rb'))
        helper.streams = helper_streams.Streams(
            SimpleNamespace(stdout=pipes[0], stderr=pipes[1]))
        manager.attach(helper, helper.proc, helper.tree)
        close = helper.streams.close

        def observed_close(budget):
            close_budgets.append(budget)
            return close(budget)

        helper.streams.close = observed_close
        with monkeypatch.context() as patch:
            patch.setattr(helpers, 'time', SimpleNamespace(monotonic=lambda: clock[0]))
            patch.setattr(threading.Thread, 'join', scheduled_join)
            assert manager.settle(helper, deadline=.4) is False
        assert close_budgets == pytest.approx([.4]), (
            'Failed settlement spent cancellation budget waiting for unprovable EOF')
        assert helper in manager.live() and helper.state == helpers.STRANDED
        assert helper.units_path == report and helper.straggler
        assert helper.output == ('', '', False), 'cancellation is not EOF'
        assert all(not reader.thread.is_alive() and reader.pipe.closed
                   for reader in helper.streams.readers)
        if failure not in ('tree', 'leader'):
            assert helper.units_problem
        else:
            assert 'owned processes remain' in helper.straggler
    finally:
        for writer in writers:
            os.close(writer)
        if helper.streams is not None:
            helper.streams.close(1)
        for pipe in pipes:
            if not pipe.closed:
                pipe.close()
        report.unlink(missing_ok=True)


@pytest.mark.parametrize('bad', [
    {'pid': 123, 'pidToken': True, 'tree': {'kind': 'pgid', 'group': 123}},
    {'pid': 123, 'pidToken': [], 'tree': {'kind': 'pgid', 'group': 123}},
    {'pid': 123, 'pidToken': 1, 'tree': {'kind': 'pgid', 'group': '123'}},
    {'pid': 123, 'pidToken': 1, 'tree': {'kind': 'pgid', 'group': -123}},
    {'pid': 123, 'pidToken': 1, 'tree': {'kind': 'pgid', 'group': 456}},
    {'pid': 123, 'pidToken': 1, 'tree': {'kind': 'job', 'group': {'name':'x'}}},
    {'pid': 123, 'pidToken': 1, 'tree': {'kind': '', 'group': None}},
])
def test_report_requires_valid_tree_identity(tmp_path, bad):
    path = tmp_path / 'units.jsonl'
    path.write_text(json.dumps(bad) + '\n')
    result = unit_report.read(path)
    assert not result.complete
    assert not result.trees


def test_invalid_utf8_tail_does_not_discard_a_valid_prior_unit(tmp_path):
    path = tmp_path / 'units.jsonl'
    record = {'pid': 123, 'pidToken': 1, 'tree': {'kind': 'pgid', 'group': 123}}
    path.write_bytes((json.dumps(record) + '\n').encode() + b'\xff\n')
    result = unit_report.read(path)
    assert not result.complete
    assert len(result.trees) == 1
