"""Ending a child this process has decided it cannot keep.

Two things can go wrong between creating a browser driver and owning it, and
both end the same way: the driver could not be put in its job object, or its
ownership could not be written to the report its owner reads. Either leaves a
process running that nothing will ever be able to find again, so the rule is
fail-closed - it is stopped rather than left.

Split out of `proctree` because it is a policy, not a mechanism. `proctree`
knows how to create a tree, signal it and ask whether it is empty; this knows
what to do when the answer is that we should never have had one. Keeping them
together is also what let the policy be wrong in a way that read as correct:
`_abandon` kills the immediate process, and after `_spawn_leader` the leader is
already in its own job or session, so its browser is not a descendant of
anything still held. A real grandchild survived a report-write failure while
the error said "It was not left running".

The import of `proctree` is local, for the same reason `unit_report` does it:
the two modules are mutually dependent by nature - disposal ends trees,
spawning disposes of what it cannot own - and one of the two edges has to be
deferred.
"""
from __future__ import annotations

from .unit_report import UnitReportFailed

#: What ending an unkeepable tree may take. Short on purpose: this runs on a
#: launch path a caller is waiting on, and the process was created seconds ago.
ABANDON_GRACE = 5.0


def abandon(proc) -> None:
    """End a child we have decided not to keep, quietly.

    The leader only. Correct where no tree was ever created - the job object
    failed, so there is nothing else to reach - and never sufficient once one
    has been.
    """
    try:
        proc.kill()
        proc.wait(timeout=10)
    except Exception:      # noqa: BLE001 - we are already failing
        pass


def reported(proc, tree) -> object:
    """Record the tree, or end it. Returns the tree when the record was written.

    A child whose ownership cannot be *communicated* is in the same position as
    one that cannot be *owned*, so the same fail-closed rule applies. What is
    ended is the tree, because that is what exists by now.

    When the tree cannot be proved empty either, the failure says so rather
    than claiming a stop that did not happen, and carries the identity - job
    name or process group - that the missing report would have carried. That
    identity is the only thing left that can reach the process.
    """
    from . import proctree           # local: see the module docstring

    try:
        proctree.report_unit(tree)
    except UnitReportFailed as unreported:
        try:
            gone = proctree.end_tree(tree, grace=ABANDON_GRACE,
                                     kill_grace=ABANDON_GRACE)
        except Exception as e:                            # noqa: BLE001
            gone, why = False, f"{type(e).__name__}: {e}"
        else:
            why = None
        abandon(proc)          # the leader too, in case it left the tree
        if not gone:
            raise UnitReportFailed(
                f"{unreported} Ending it failed as well"
                f"{f' ({why})' if why else ''}, so {tree.kind or 'the'} tree "
                f"{tree.group!r} (leader pid {tree.pid}) may still be running "
                f"and is not recorded anywhere: end it by that identity."
            ) from unreported
        raise
    return tree
