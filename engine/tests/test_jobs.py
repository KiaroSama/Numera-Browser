

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


def test_terminate_refuses_a_pid_that_is_no_longer_ours():
    import os

    from numera import jobs

    me = os.getpid()
    assert jobs._terminate_tree(me, (jobs.started_at(me) or 0) + 1) is False


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
