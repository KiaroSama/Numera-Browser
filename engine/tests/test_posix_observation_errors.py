"""Incomplete procfs/ps observations do not prove an owned group empty."""
import errno
import os
import subprocess
import sys
from pathlib import Path

import pytest
from numera import posix_session, proctree

pytestmark = pytest.mark.skipif(os.name == "nt", reason="POSIX process-group observation")
requires_procfs = pytest.mark.skipif(os.name != "nt" and not Path("/proc/self/stat").is_file(),
                                     reason="requires Linux procfs")


@pytest.mark.parametrize('error', [errno.EACCES, errno.EIO])
@requires_procfs
def test_real_running_child_is_not_declared_gone_when_its_stat_is_unreadable(monkeypatch, error):
    proc = subprocess.Popen([sys.executable, '-c', 'import time;time.sleep(30)'],
                            start_new_session=True)
    tree = proctree.Tree(proc.pid, proctree.identify(proc), proc.pid, 'pgid')
    stat = Path(f'/proc/{proc.pid}/stat')
    read_text = Path.read_text
    def read(path, *args, **kwargs):
        if path == stat:
            raise OSError(error, 'injected failure for the fixture child only')
        return read_text(path, *args, **kwargs)
    try:
        assert proc.poll() is None
        with monkeypatch.context() as patch:
            patch.setattr(Path, 'read_text', read)
            assert proctree.alive_tree(tree) is True
        assert proc.poll() is None
    finally:
        proc.kill()
        proc.wait(timeout=3)


@requires_procfs
def test_bad_stat_text_is_unknown_not_a_zombie(monkeypatch):
    proc = subprocess.Popen([sys.executable, '-c', 'import time;time.sleep(30)'],
                            start_new_session=True)
    tree = proctree.Tree(proc.pid, proctree.identify(proc), proc.pid, 'pgid')
    stat = Path(f'/proc/{proc.pid}/stat')
    original = Path.read_text
    try:
        with monkeypatch.context() as patch:
            patch.setattr(Path, 'read_text', lambda path, *a, **kw:
                          'incomplete stat' if path == stat else original(path, *a, **kw))
            assert proctree.alive_tree(tree) is True
        assert proc.poll() is None
    finally:
        proc.kill()
        proc.wait(timeout=3)




@requires_procfs
def test_process_table_listing_failure_is_unknown(monkeypatch):
    original = Path.iterdir
    def iterdir(path):
        if path == Path('/proc'):
            raise PermissionError('injected table observation failure')
        return original(path)
    monkeypatch.setattr(Path, 'iterdir', iterdir)
    assert posix_session._group_is_running(123456789) is True


@pytest.mark.parametrize('code,stdout,expected', [
    (1, '123 S 123\n', True),
    (0, 'invalid-row\n', True),
    (0, '123 Z 123\n', False),
    (0, '123 Z+ 123\n', False),
    (0, '123 S 123\n', True),
    (0, '123 S 0\n', True),
    (0, '987 S 987\n', False),
    (0, '', True),
])
def test_ps_fallback_checks_exit_and_row_integrity(monkeypatch, code, stdout, expected):
    original = Path.is_dir
    monkeypatch.setattr(Path, 'is_dir', lambda path: False if path == Path('/proc') else original(path))
    monkeypatch.setattr(subprocess, 'run', lambda *a, **kw:
                        subprocess.CompletedProcess(a, code, stdout=stdout, stderr=''))
    # A failed partial observation for a different group must not prove that
    # the requested group is absent.
    pgid = 789 if code else 123
    assert posix_session._group_is_running(pgid) is expected


def test_ps_failure_diagnostic_is_bounded_numeric_and_does_not_query_again(monkeypatch, capsys):
    original = Path.is_dir
    monkeypatch.setattr(Path, 'is_dir', lambda path: False if path == Path('/proc') else original(path))
    calls = []

    def query(*args, **kwargs):
        calls.append(args)
        return subprocess.CompletedProcess(args, 0, stdout='123 S 456\n', stderr='private fixture text')

    monkeypatch.setattr(subprocess, 'run', query)
    assert posix_session._group_is_running(123)
    assert not capsys.readouterr().err, 'ordinary observation must remain silent'
    posix_session.report_group_failure(123, graceful=True, forced=False, verified=True)
    output = capsys.readouterr().err
    assert len(calls) == 1, 'failure diagnostics must not extend the cleanup deadline'
    assert 'group=123' in output and 'pid=456' in output and 'state_codes=(83,)' in output
    assert 'graceful_signal=True force_signal=False' in output
    assert 'private fixture text' not in output


def test_ps_failed_query_diagnostic_never_emits_raw_output(monkeypatch, capsys):
    original = Path.is_dir
    monkeypatch.setattr(Path, 'is_dir', lambda path: False if path == Path('/proc') else original(path))
    monkeypatch.setattr(subprocess, 'run', lambda *a, **kw:
                        subprocess.CompletedProcess(a, 1, stdout='private fixture text', stderr='secret data'))
    assert posix_session._group_is_running(123)
    posix_session.report_group_failure(123, graceful=False, forced=False, verified=True)
    output = capsys.readouterr().err
    assert 'query_exit=1' in output and 'private fixture' not in output and 'secret data' not in output


@pytest.mark.parametrize('refusal,reason', [
    ('identity', 'ownership_mismatch'), ('own-group', 'caller_group'),
    (errno.ESRCH, 'signal_errno=3'), (errno.EPERM, 'signal_errno=1'),
])
def test_signal_refusal_diagnostic_reuses_the_attempt_without_extra_queries(
        monkeypatch, capsys, refusal, reason):
    original = Path.is_dir
    monkeypatch.setattr(Path, 'is_dir', lambda path: False if path == Path('/proc') else original(path))
    tree = proctree.Tree(123456789, 7, 123456789, 'pgid')
    queries, signals = [], []

    def birth(pid):
        queries.append(pid)
        return 8 if refusal == 'identity' else 7

    def signal_group(pgid, sig):
        signals.append((pgid, sig))
        raise OSError(refusal, 'private fixture text')

    monkeypatch.setattr(proctree, 'started_at', birth)
    monkeypatch.setattr(os, 'getpgrp', lambda: tree.group if refusal == 'own-group' else 1)
    monkeypatch.setattr(os, 'killpg', signal_group)
    assert proctree.signal_tree(tree, force=True) is False
    assert queries == [tree.pid]
    assert len(signals) == (0 if isinstance(refusal, str) else 1)
    assert not capsys.readouterr().err
    posix_session.report_group_failure(tree.group, graceful=True, forced=False, verified=True)
    output = capsys.readouterr().err
    assert reason in output and 'force=True' in output
    assert 'private fixture text' not in output
    assert queries == [tree.pid], 'reporting must not repeat the birth query'


def test_delivered_group_signal_is_recorded_without_changing_ownership(monkeypatch, capsys):
    original = Path.is_dir
    monkeypatch.setattr(Path, 'is_dir', lambda path: False if path == Path('/proc') else original(path))
    tree = proctree.Tree(123456789, 7, 123456789, 'pgid')
    calls = []
    monkeypatch.setattr(proctree, 'started_at', lambda pid: 7)
    monkeypatch.setattr(os, 'getpgrp', lambda: 1)
    monkeypatch.setattr(os, 'killpg', lambda *args: calls.append(args))
    assert proctree.signal_tree(tree, force=True) is True
    posix_session.report_group_failure(tree.group, graceful=True, forced=True, verified=True)
    assert 'reason=delivered' in capsys.readouterr().err
    assert calls == [(tree.group, __import__('signal').SIGKILL)]


def test_healthy_running_and_reaped_child_controls():
    proc = subprocess.Popen([sys.executable, '-c', 'import time;time.sleep(30)'],
                            start_new_session=True)
    tree = proctree.Tree(proc.pid, proctree.identify(proc), proc.pid, 'pgid')
    try:
        assert proctree.alive_tree(tree) is True
    finally:
        proc.kill()
        proc.wait(timeout=3)
    assert proctree.alive_tree(tree) is False


@requires_procfs
def test_end_tree_cannot_report_a_live_child_stopped_from_failed_observation(monkeypatch):
    proc = subprocess.Popen([sys.executable, '-c', 'import time;time.sleep(30)'],
                            start_new_session=True)
    tree = proctree.Tree(proc.pid, proctree.identify(proc), proc.pid, 'pgid')
    target = Path(f'/proc/{proc.pid}/stat')
    original = Path.read_text
    def read(path, *args, **kwargs):
        if path == target:
            raise PermissionError(errno.EACCES, 'fixture observation denied')
        return original(path, *args, **kwargs)
    try:
        with monkeypatch.context() as patch:
            patch.setattr(Path, 'read_text', read)
            finished = proctree.end_tree(tree, grace=.02, kill_grace=.02)
        # Observe before fixture teardown, independently from the tree scanner.
        assert not finished or proc.poll() is not None
    finally:
        if proc.poll() is None:
            proc.kill()
        proc.wait(timeout=3)


@pytest.mark.parametrize('platform,error,code,stdout,expected,observations', [
    ('darwin', errno.EPERM, 0, '123 Z 123\n987 S 987\n', False, 1),
    ('darwin', errno.EPERM, 0, '123 Z+ 123\n123 Z 456\n', False, 1),
    ('darwin', errno.EPERM, 0, '123 Z 123\n123 S 456\n', True, 1),
    ('darwin', errno.EPERM, 0, '123 R<s 123\n', True, 1),
    ('darwin', errno.EPERM, 1, '987 S 987\n', True, 1),
    ('darwin', errno.EPERM, 0, 'malformed\n', True, 1),
    ('darwin', errno.EPERM, 0, '123 ? 123\n', True, 1),
    ('darwin', errno.EPERM, 0, '', True, 1),
    ('darwin', errno.EPERM, None, '', True, 1),
    ('darwin', errno.EIO, 0, '123 Z 123\n', True, 0),
    ('linux', errno.EPERM, 0, '123 Z 123\n', True, 0),
    ('darwin', errno.ESRCH, 0, '123 S 123\n', False, 0),
])
def test_darwin_denied_probe_requires_complete_zombie_aware_observation(
        monkeypatch, platform, error, code, stdout, expected, observations):
    tree = proctree.Tree(123, 7, 123, 'pgid')
    original = Path.is_dir
    monkeypatch.setattr(Path, 'is_dir', lambda path: False if path == Path('/proc') else original(path))
    monkeypatch.setattr(sys, 'platform', platform)
    monkeypatch.setattr(os, 'getpgrp', lambda: 1)
    probes, queries = [], []

    def probe(pgid, sig):
        probes.append((pgid, sig))
        raise OSError(error, 'injected group probe refusal')

    def query(*args, **kwargs):
        queries.append((args, kwargs))
        if code is None:
            raise subprocess.TimeoutExpired(args[0], 5)
        return subprocess.CompletedProcess(args, code, stdout=stdout, stderr='private fixture text')

    monkeypatch.setattr(os, 'killpg', probe)
    monkeypatch.setattr(subprocess, 'run', query)
    assert proctree.alive_tree(tree) is expected
    assert probes == [(123, 0)], 'observation must not deliver a termination signal'
    assert len(queries) == observations
    if queries:
        assert queries[0][0][0] == ['ps', '-A', '-o', 'pgid=,state=,pid=']
        assert queries[0][1]['timeout'] == 5


def test_failed_probe_invalidates_a_previous_live_process_sample(monkeypatch, capsys):
    original = Path.is_dir
    monkeypatch.setattr(Path, 'is_dir', lambda path: False if path == Path('/proc') else original(path))
    monkeypatch.setattr(subprocess, 'run', lambda *a, **kw:
                        subprocess.CompletedProcess(a, 0, stdout='123 R<s 123\n', stderr=''))
    assert posix_session._group_is_running(123)
    monkeypatch.setattr(os, 'getpgrp', lambda: 1)

    def probe(*args):
        raise OSError(errno.EIO, 'private fixture text')

    monkeypatch.setattr(os, 'killpg', probe)
    assert proctree.alive_tree(proctree.Tree(123, 7, 123, 'pgid'))
    posix_session.report_group_failure(123, graceful=True, forced=False, verified=True)
    output = capsys.readouterr().err
    assert 'probe_errno=5' in output
    assert 'state_codes=' not in output and 'private fixture text' not in output


@pytest.mark.parametrize('error,expected', [(errno.ESRCH, False), (errno.EACCES, True)])
def test_probe_disappearance_and_permission_controls(monkeypatch, error, expected):
    tree = proctree.Tree(123456789, 1, 123456789, 'pgid')
    def probe(*args):
        raise OSError(error, 'fixture probe result')
    monkeypatch.setattr(os, 'killpg', probe)
    assert proctree.alive_tree(tree) is expected


@requires_procfs
def test_disappearing_procfs_entry_is_not_an_observation_error(monkeypatch):
    target = Path('/proc/123456789')
    original_iter = Path.iterdir
    original_read = Path.read_text
    monkeypatch.setattr(Path, 'iterdir', lambda p: iter([target])
                        if p == Path('/proc') else original_iter(p))
    def read(path, *args, **kwargs):
        if path == target / 'stat':
            raise FileNotFoundError(errno.ENOENT, 'fixture entry disappeared')
        return original_read(path, *args, **kwargs)
    monkeypatch.setattr(Path, 'read_text', read)
    assert posix_session._group_is_running(123456789) is False


@requires_procfs
def test_ignored_pid_is_excluded_before_its_stat_is_read(monkeypatch):
    target = Path('/proc/123456789')
    original_iter = Path.iterdir
    original_read = Path.read_text
    monkeypatch.setattr(Path, 'iterdir', lambda p: iter([target])
                        if p == Path('/proc') else original_iter(p))
    def read(path, *args, **kwargs):
        if path == target / 'stat':
            raise AssertionError('ignored pid must not be inspected')
        return original_read(path, *args, **kwargs)
    monkeypatch.setattr(Path, 'read_text', read)
    assert posix_session._group_is_running(123456789, ignore=123456789) is False
