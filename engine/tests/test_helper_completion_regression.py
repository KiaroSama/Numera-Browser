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


def test_end_all_uses_one_budget_across_all_helpers():
    manager = helpers.HelperManager()
    pending = [manager.admit(('fixture',)) for _ in range(3)]
    for helper in pending:
        manager.child_env(helper)
        manager.attach(helper, None, None)
        helper.finish.acquire()
    try:
        started = time.monotonic()
        left = manager.end_all(deadline=.08)
        elapsed = time.monotonic() - started
        assert len(left) == 3
        assert elapsed < .17, f'one .08s budget became {elapsed:.3f}s'
    finally:
        for helper in pending:
            helper.finish.release()
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
try:
    helpers.run_helper([sys.executable,'-c',sys.argv[3],sys.argv[1],sys.argv[2],sys.argv[4]],5,manager=m,cleanup=.4)
    result={'error':None}
except helpers.HelperFailed as e:
    result={'error':type(e).__name__}
reported=json.loads(Path(sys.argv[1]).read_text())['reported_at']
result.update(elapsed=time.monotonic()-start,cleanup_elapsed=time.monotonic()-reported,retained=len(m.live()),readers=[t.name for t in threading.enumerate() if t.name.startswith('helper-')])
Path(sys.argv[5]).write_text(json.dumps(result))
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
    assert observed['readers'] == [], observed


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
