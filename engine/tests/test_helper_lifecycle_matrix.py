"""Real-process lifecycle controls and deterministic concurrent interleavings."""
import json
import os
import subprocess
import sys
import threading
import time
from pathlib import Path

import pytest
from numera import proctree, unit_report
from numera.server import helpers, helper_streams


@pytest.mark.parametrize('exit_code', [0, 3])
@pytest.mark.parametrize('payload', ['', 'tail', 'سلام\nΩ\r\nZ\rW', 'x' * 200000], ids=['empty', 'tail', 'unicode-newlines', 'large'])
def test_normal_and_nonzero_exit_preserves_both_streams(exit_code, payload):
    # stdin is intentionally not used by the helper; argument size stays below
    # OS command-line limits even for the large output case.
    text = repr(payload) if len(payload) < 100 else "'x' * 200000"
    program = f'import sys;v={text};sys.stdout.buffer.write(v.encode());sys.stderr.buffer.write(v.encode());sys.exit({exit_code})'
    manager = helpers.HelperManager()
    result = helpers.run_helper([sys.executable, '-c', program], 5, manager=manager, cleanup=1)
    expected = payload.replace('\r\n', '\n').replace('\r', '\n')
    assert (result.returncode, result.stdout, result.stderr) == (exit_code, expected, expected)
    assert not manager.live()
    assert not [t for t in threading.enumerate() if t.name.startswith('helper-')]


@pytest.mark.parametrize('separate', [False, True])
@pytest.mark.parametrize('timed_out', [False, True])
def test_same_or_separate_tree_holding_writers_is_ended(tmp_path, separate, timed_out):
    receipt = tmp_path / 'child.json'
    program = '''
import json,subprocess,sys,time
from pathlib import Path
from numera import proctree
argv=[sys.executable,'-c','import time;time.sleep(30)']
if sys.argv[2]=='True':
    p,t=proctree._spawn_leader(subprocess.Popen,argv,(),{})
else:
    p=subprocess.Popen(argv);t=proctree.Tree(pid=p.pid,token=proctree.identify(p))
Path(sys.argv[1]).write_text(json.dumps({'pid':p.pid,'token':t.token}))
sys.stdout.write('TAIL');sys.stdout.flush()
if sys.argv[3]=='True':time.sleep(30)
'''
    manager = helpers.HelperManager()
    # The fixture must start Python, import `numera` and spawn its child before
    # the helper deadline. That takes ~0.6 s idle on Windows; 1.2 s left 2x
    # headroom and failed in CI whenever the WSL jobs shared the machine. The
    # deadline stays far below the fixture's 30 s sleep, so the timeout path is
    # still what is proved, and the bound below is relative to it.
    timeout = 4 if timed_out else 5
    start = time.monotonic()
    try:
        if timed_out:
            with pytest.raises(helpers.HelperTimeout):
                helpers.run_helper([sys.executable, '-c', program, str(receipt), str(separate), str(timed_out)],
                                   timeout, manager=manager, cleanup=1)
        else:
            result = helpers.run_helper([sys.executable, '-c', program, str(receipt), str(separate), str(timed_out)],
                                        timeout, manager=manager, cleanup=1)
            assert result.stdout == 'TAIL'
        assert time.monotonic() - start < timeout + 2, 'cleanup overran the deadline plus its grace'
        assert receipt.exists(), 'fixture never created the relevant child'
        child = json.loads(receipt.read_text())
        stat = Path(f"/proc/{child['pid']}/stat")
        if os.name != 'nt' and stat.exists():
            state = stat.read_text().rsplit(')', 1)[1].split()[0]
            assert state == 'Z', 'a non-zombie fixture child survived'
        else:
            assert not proctree.alive(child['pid'], child['token'])
        assert not manager.live()
    finally:
        for h in manager.live():
            manager.settle(h, deadline=1)


@pytest.mark.parametrize('method', ['child_env', 'check_open', 'attach'])
def test_twenty_controlled_start_shutdown_interleavings(method):
    for attempt in range(20):
        manager = helpers.HelperManager()
        reached, release, closed = (threading.Event() for _ in range(3))
        original = getattr(manager, method)
        original_settle = manager.settle
        def barrier(*args, **kwargs):
            reached.set()
            assert release.wait(5)
            return original(*args, **kwargs)
        def observed_settle(*args, **kwargs):
            if manager.closed:
                closed.set()
            return original_settle(*args, **kwargs)
        setattr(manager, method, barrier)
        manager.settle = observed_settle
        result = {}
        def start():
            try:
                result['value'] = helpers.run_helper([sys.executable, '-c', 'pass'], 3,
                                                     manager=manager, cleanup=.5)
            except helpers.HelperShuttingDown:
                result['refused'] = True
            except BaseException as error:
                result['error'] = repr(error)
        worker = threading.Thread(target=start)
        sweeper = threading.Thread(target=lambda: result.update(left=manager.end_all(deadline=1)))
        worker.start()
        assert reached.wait(5)
        sweeper.start()
        assert closed.wait(5), 'shutdown never reached the controlled starting reservation'
        release.set()
        worker.join(5)
        sweeper.join(5)
        # Observe before any fixture cleanup; teardown cannot make these pass.
        assert not worker.is_alive() and not sweeper.is_alive()
        assert 'error' not in result, (attempt, method, result)
        assert result.get('refused') or 'value' in result
        assert not manager.live(), (attempt, method, manager.stragglers())


@pytest.fixture(autouse=True)
def tested_checkout_for_children(monkeypatch):
    """Child imports must resolve to the same checkout being tested."""
    engine = str(Path(proctree.__file__).resolve().parents[1])
    prior = os.environ.get("PYTHONPATH")
    monkeypatch.setenv("PYTHONPATH", engine + (os.pathsep + prior if prior else ""))


def test_partial_start_failure_inside_real_helper_is_joined(monkeypatch):
    manager = helpers.HelperManager()
    real_start = threading.Thread.start
    def start(thread):
        if thread.name == 'helper-stderr':
            raise RuntimeError('second reader not started')
        return real_start(thread)
    with monkeypatch.context() as patch:
        patch.setattr(threading.Thread, 'start', start)
        with pytest.raises(RuntimeError, match='second reader'):
            helpers.run_helper([sys.executable, '-c', 'import time;time.sleep(30)'], 5,
                               manager=manager, cleanup=.5)
    assert not any(t.name.startswith('helper-') for t in threading.enumerate())
    for h in manager.live():
        assert h.proc.poll() is not None
        assert h.resolved.is_set()
        assert all(r.pipe.closed for r in h.streams.readers)


def test_settlement_overlap_has_one_terminal_result():
    manager = helpers.HelperManager()
    h = manager.admit(('fixture',))
    env = manager.child_env(h)
    p,t = proctree.spawn([sys.executable,'-c','pass'], env=env,
                         stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    manager.adopt(h,p,t)
    h.streams = helper_streams.Streams(p)
    manager.attach(h,p,t)
    p.wait(timeout=3)
    outcomes = []
    barrier = threading.Barrier(3)
    def settle():
        barrier.wait()
        outcomes.append(manager.settle(h, deadline=1))
    threads = [threading.Thread(target=settle) for _ in range(2)]
    for thread in threads:thread.start()
    barrier.wait()
    for thread in threads:thread.join(3)
    assert outcomes == [True, True]
    assert not manager.live()
    assert h.state == helpers.SETTLED and not h.straggler


def test_empty_file_does_not_prove_zero_units_but_initialized_report_does(tmp_path):
    path=tmp_path/'units'
    path.write_bytes(b'')
    assert not unit_report.read(path).complete
    unit_report.initialize(path)
    assert unit_report.read(path).complete


def test_pending_transfer_is_not_zero_ownership(tmp_path, monkeypatch):
    path=tmp_path/'units'
    path.touch();unit_report.initialize(path)
    monkeypatch.setenv(unit_report.UNITS_ENV,str(path))
    attempt=unit_report.begin_spawn()
    assert not unit_report.read(path).complete
    unit_report.cancel_spawn(attempt)
    assert unit_report.read(path).complete


def test_report_failure_precedes_any_driver_spawn(tmp_path, monkeypatch):
    path=tmp_path/'absent'
    monkeypatch.setenv(unit_report.UNITS_ENV,str(path))
    spawned=[]
    def unexpected(*a,**kw):spawned.append(True);raise AssertionError('spawned')
    with pytest.raises(unit_report.UnitReportFailed):
        proctree._spawn_leader(unexpected,['fixture'],(),{})
    assert not spawned


def test_interrupted_unit_line_keeps_pending_receipt(tmp_path, monkeypatch):
    path=tmp_path/'units';path.touch();unit_report.initialize(path)
    monkeypatch.setenv(unit_report.UNITS_ENV,str(path))
    attempt=unit_report.begin_spawn()
    with path.open('ab') as file:file.write(b'{"event":"unit"')
    result=unit_report.read(path)
    assert not result.complete and 'did not finish' in result.problem


def test_concurrent_managers_do_not_share_admission():
    a,b=helpers.HelperManager(),helpers.HelperManager()
    assert a.end_all(deadline=.1)==[]
    with pytest.raises(helpers.HelperShuttingDown):
        helpers.run_helper([sys.executable,'-c','pass'],3,manager=a)
    assert helpers.run_helper([sys.executable,'-c','print(42)'],3,manager=b).stdout=='42\n'


def test_cancelled_start_is_retained_across_repeated_shutdown():
    manager=helpers.HelperManager(); h=manager.admit(('pending',))
    manager.child_env(h)
    try:
        for _ in range(3):
            assert manager.end_all(deadline=0)==[h]
            assert h in manager.live() and not h.resolved.is_set()
            assert h.cancelled
    finally:
        manager.fail_start(h)


def test_failed_report_cleanup_is_not_released(tmp_path, monkeypatch):
    manager=helpers.HelperManager(); h=manager.admit(('pending',))
    manager.child_env(h)
    report=h.units_path
    original=Path.unlink
    def refuse(path,*a,**kw):
        if path==report:raise PermissionError('fixture unlink refusal')
        return original(path,*a,**kw)
    with monkeypatch.context() as patch:
        patch.setattr(Path,'unlink',refuse)
        manager.fail_start(h)
        assert h in manager.live() and h.units_path==report
        assert h.state==helpers.STRANDED and h.resolved.is_set()
    manager.fail_start(h)
    assert not manager.live()


def test_no_unbounded_wait_for_settlement_owner():
    manager=helpers.HelperManager(); h=manager.admit(('locked',));h.finish.acquire()
    try:
        start=time.monotonic()
        assert not manager.settle(h,deadline=.05)
        assert time.monotonic()-start < .15
        assert h in manager.live()
    finally:
        h.finish.release();manager.fail_start(h)


def _assert_fixture_process_ended(pid, token, deadline=5.0):
    """The process is gone - polled to a deadline, not sampled once.

    Two different questions are being asked and only one of them is answered
    instantly. `end_tree` proves the *tree* empty through its ownership
    primitive; this asks whether one *pid* still answers. On Windows a
    terminated process keeps answering for a short interval after its job
    reports empty - measured at 10-11 ms on an idle workstation - so a single
    sample taken the moment `reported()` returns sees a live descendant and
    fails a path that worked.

    Polled rather than slept: the product's own `prove_empty` polls to a
    deadline for the same reason, and a hard-coded delay measured on an idle
    machine is not a bound for a loaded runner. The deadline is generous
    against the measurement precisely so that a real survivor - one that never
    goes away - still fails this assertion instead of being waited out.
    """
    path = Path(f'/proc/{pid}/stat')
    if os.name != 'nt' and path.exists():
        assert path.read_text().rsplit(')', 1)[1].split()[0] == 'Z'
        return
    end = time.monotonic() + deadline
    while proctree.alive(pid, token) and time.monotonic() < end:
        time.sleep(.01)
    assert not proctree.alive(pid, token)


def test_failed_report_after_a_descendant_exists_ends_the_whole_tree(tmp_path, monkeypatch):
    from numera import disposal
    receipt = tmp_path / 'descendant.json'
    program = '''
import subprocess,sys,time,json
from pathlib import Path
from numera import proctree
p=subprocess.Popen([sys.executable,'-c','import time;time.sleep(30)'])
Path(sys.argv[1]).write_text(json.dumps({'pid':p.pid,'token':proctree.identify(p)}))
time.sleep(30)
'''
    proc, tree = proctree.spawn([sys.executable, '-c', program, str(receipt)],
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    def refuse(*args, **kwargs):
        raise unit_report.UnitReportFailed('fixture record failure')
    try:
        end = time.monotonic() + 5
        while not receipt.exists() and time.monotonic() < end:
            time.sleep(.01)
        assert receipt.exists(), 'descendant fixture was not created'
        child = json.loads(receipt.read_text())
        with monkeypatch.context() as patch:
            patch.setattr(proctree, 'report_unit', refuse)
            with pytest.raises(unit_report.UnitReportFailed):
                disposal.reported(proc, tree)
        assert proc.poll() is not None
        _assert_fixture_process_ended(child['pid'], child['token'])
    finally:
        proctree.end_tree(tree, grace=.1, kill_grace=1)
        if proc.poll() is None:proc.kill()
        proc.wait(timeout=3)


def test_failed_claim_callback_ends_the_captured_tree(tmp_path):
    from numera import driver_capture
    receipt = tmp_path / 'descendant.json'
    program = '''
import subprocess,sys,time,json
from pathlib import Path
from numera import proctree
p=subprocess.Popen([sys.executable,'-c','import time;time.sleep(30)'])
Path(sys.argv[1]).write_text(json.dumps({'pid':p.pid,'token':proctree.identify(p)}))
time.sleep(30)
'''
    def refuse(tree):
        end=time.monotonic()+5
        while not receipt.exists() and time.monotonic()<end:time.sleep(.01)
        assert receipt.exists(), 'descendant fixture was not created'
        raise RuntimeError('fixture failed claim')
    with pytest.raises(RuntimeError, match='fixture failed claim'):
        with driver_capture.spawning_own_tree(sys.executable,on_tree=refuse) as holder:
            subprocess.Popen([sys.executable,'-c',program,str(receipt)],
                             stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    assert holder.proc.poll() is not None
    child=json.loads(receipt.read_text())
    _assert_fixture_process_ended(child['pid'],child['token'])
