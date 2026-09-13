"""An outer supervisor for tests about things that can block.

A test that asserts "this call is bounded" cannot use the call's own deadline
as its evidence: if the bound is broken, the test hangs with it and the suite
reports nothing at all. So the budget is declared *before* the call, watched
from outside it, and a supervisor firing is always a failed acceptance - even
when the work later finishes with a zero exit.

Not a plugin and not a rerun helper. One thread, one deadline, and a stack dump
of every thread when it fires, because "where was it stuck" is the only useful
thing to know afterwards.
"""
from __future__ import annotations

import faulthandler
import io
import threading
import time


class Supervised:
    """The outcome of a supervised call."""

    def __init__(self) -> None:
        self.value = None
        self.error: "BaseException | None" = None
        self.elapsed = 0.0
        self.overran = False
        self.stacks = ""


def supervise(call, *, budget: float, allowance: float = 6.0) -> Supervised:
    """Run `call()` under a declared total deadline. Never raises from the wait.

    `budget` is what the operation and its cleanup are allowed between them;
    `allowance` is scheduling slack on a loaded machine, stated rather than
    hidden inside the budget. The worker is a daemon so an overrun cannot wedge
    the whole suite - the test fails on `overran`, and the fixture cleans up
    whatever is left.
    """
    outcome = Supervised()
    finished = threading.Event()

    def run():
        started = time.monotonic()
        try:
            outcome.value = call()
        except BaseException as e:                       # noqa: BLE001
            outcome.error = e
        finally:
            outcome.elapsed = time.monotonic() - started
            finished.set()

    worker = threading.Thread(target=run, name="supervised", daemon=True)
    worker.start()
    if not finished.wait(budget + allowance):
        outcome.overran = True
        buffer = io.StringIO()
        faulthandler.dump_traceback(file=buffer)
        outcome.stacks = buffer.getvalue()
        # Give it a moment to land anyway, so the diagnosis names a real state
        # rather than one the dump interrupted.
        finished.wait(2.0)
    return outcome


def assert_bounded(outcome: Supervised, what: str) -> None:
    """A supervisor kill is a failure, whatever the worker eventually returned."""
    assert not outcome.overran, (
        f"{what} exceeded its declared deadline - the bound is broken, not "
        f"slow. Stacks at the moment it fired:\n{outcome.stacks}")
