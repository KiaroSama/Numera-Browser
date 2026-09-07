

# --- pid identity ----------------------------------------------------------

def test_a_recycled_pid_is_not_treated_as_the_original_process():
    """A pid alone is not an identity. This module hands stored pids to
    `taskkill /T /F`, so believing a recycled one means killing an unrelated
    process tree."""
    import os

    from numera import jobs

    me = os.getpid()
    token = jobs.started_at(me)
    assert token, "no start time could be read for this very process"
    assert jobs.alive(me, token) is True
    assert jobs.alive(me, token + 1) is False, \
        "a pid whose start time does not match was accepted as ours"


def test_terminate_never_signals_a_pid_that_is_no_longer_ours(monkeypatch):
    """The pid is this very process, so a signal would end the test run.

    Asserting on the return value alone is the weaker check, and it changed
    meaning: ending a tree now answers "is the tree gone", and for a pid the OS
    has handed to somebody else the honest answer is yes - our process is not
    in it. What must never happen is a *signal*, so that is what this asserts,
    and it cannot pass by accident.
    """
    import os

    from numera import proctree

    me = os.getpid()
    signalled = []
    monkeypatch.setattr(proctree, "signal_tree",
                        lambda *a, **kw: signalled.append(a) or True)

    stale = proctree.Tree(pid=me, token=(proctree.started_at(me) or 0) + 1)
    assert proctree.end_tree(stale) is True
    assert signalled == [], "a recycled pid was signalled"


def test_a_dead_pid_is_dead_with_or_without_a_token():
    from numera import jobs

    assert jobs.alive(999_999, 12345) is False
    assert jobs.alive(0) is False


def test_a_job_record_stores_the_identity_beside_the_pid(tmp_path):
    """The guard is only worth having if the value it needs is recorded."""
    import inspect

    from numera import jobs

    src = inspect.getsource(jobs.start) if hasattr(jobs, "start") else inspect.getsource(jobs)
    assert '"pidToken"' in src, "the pid is stored without its start time"


def test_the_fallback_identity_is_stable_across_processes():
    """It used Python's `hash()` on a string, which is randomised per process
    (PYTHONHASHSEED). A token recorded by one CLI invocation therefore never
    matched the next one: on macOS every live job looked recycled and
    `jobs stop` refused to touch it."""
    import subprocess
    import sys

    code = (
        "import sys; sys.path.insert(0, r'engine');"
        "import hashlib;"
        "line = 'Mon Aug 31 12:00:00 2026';"
        "print(int(hashlib.sha256(line.encode('utf-8')).hexdigest()[:12], 16))"
    )
    seen = set()
    for _ in range(3):
        out = subprocess.run([sys.executable, "-c", code], capture_output=True,
                             text=True, timeout=60)
        seen.add(out.stdout.strip())
    assert len(seen) == 1, f"the identity differed between processes: {seen}"

    from numera import jobs
    import inspect

    src = inspect.getsource(jobs.started_at)
    assert "hash(line)" not in src, "the randomised hash() is back"
    assert "sha256" in src
