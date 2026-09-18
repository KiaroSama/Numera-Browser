"""A helper's stdout and stderr, owned as a unit and drained without lying.

Reading a child's output looks like plumbing until a second process is holding
the write end. Then every shortcut becomes a wrong answer:

* `communicate()` waits for end-of-file, and EOF is not the helper exiting. A
  driver started inside the helper inherits its pipe writers - on Windows
  `_spawn_leader` passes `close_fds=False` so the child keeps its own job
  handle, and the std handles come with it - so the wait became the *browser's*
  lifetime rather than the probe's.
* iterating the pipe by line drops a trailing write that has no newline. The
  measured symptom was a helper that printed `TRAILING-NO-NEWLINE`, exited 0,
  and handed the caller `stdout=''` with `returncode=0`: a confident wrong
  answer, which is worse than the hang it replaced.
* daemon reader threads that are never joined turn that truncation into
  something nobody can even detect. A reader still holding data when the caller
  returns is not a tidy loose end; it is the missing half of the result.

So the readers are owned: started with the process, joined against a budget the
caller declares, and asked afterwards whether they actually reached the end.
`complete` is the whole point - a result may be returned only when it is true,
and an incomplete drain is an error the caller raises rather than a string it
silently shortens.

Closing is deliberately **not** done here at the end of the wait. The writers
must be gone first; closing a pipe while a live separately sessioned process
still holds the other end is the blocking close this module exists to avoid.
`run_helper` ends every owned unit, and only then collects and closes.
"""
from __future__ import annotations

import threading

#: Read size. Big enough that a probe's JSON arrives in one or two reads, small
#: enough to be a bounded allocation on a chatty child.
CHUNK = 65536

#: Pipes are opened binary and decoded once at the end. A text wrapper offers no
#: non-greedy read - `read(n)` can block for `n` characters - so draining "what
#: is available right now" needs `BufferedReader.read1`, which is only on the
#: binary object. `errors="replace"` because a truncated multi-byte sequence at
#: a kill boundary is a diagnostic, not a reason to lose the rest of the output.
ENCODING, ERRORS = "utf-8", "replace"


class _Reader:
    """One pipe, one thread, and whether it reached the end."""

    def __init__(self, name: str, pipe) -> None:
        self.name = name
        self.pipe = pipe
        self.chunks: list = []
        #: Set **only** on a clean end-of-file. Not in a `finally`, and not when
        #: the loop stops for any other reason: "the thread is no longer
        #: running" and "the output was read to the end" are different facts,
        #: and conflating them is the truncation bug one level down. A reader
        #: that dies on an unexpected exception leaves this clear, so the drain
        #: is reported incomplete and the caller raises instead of returning
        #: whatever happened to arrive first.
        self.ended = threading.Event()
        self.error: "BaseException | None" = None
        self.thread = threading.Thread(
            target=self._pump, name=f"helper-{name}", daemon=False)

    def _pump(self) -> None:
        try:
            while True:
                chunk = self.pipe.read1(CHUNK)
                if not chunk:
                    self.ended.set()            # EOF: every writer has gone
                    return
                self.chunks.append(chunk)
        except BaseException as e:              # noqa: BLE001
            # Closed under us mid-read, or anything else. Recorded rather than
            # raised into a thread nobody is watching, and deliberately without
            # setting `ended`.
            self.error = e

    def text(self) -> str:
        """The decoded output, with newlines translated as text mode would.

        `text=True` used to give this for free: a `TextIOWrapper` opened with
        `newline=None` turns `\\r\\n` and a lone `\\r` into `\\n`. Draining the
        binary object is what makes a non-greedy read possible, so the
        translation has to be done here instead - and it is not cosmetic. On
        Windows every `print()` in a helper emits `\\r\\n`, so dropping it would
        change the bytes of every probe's answer and quietly break any caller
        comparing or splitting on `\\n`.
        """
        raw = b"".join(self.chunks).decode(ENCODING, ERRORS)
        return raw.replace("\r\n", "\n").replace("\r", "\n")


class Streams:
    """The output of one helper, owned from spawn to a proved end.

    Start the readers as soon as the process exists - not after the wait.
    A child that fills the pipe buffer while nobody is reading blocks on its
    own write, which turns a deadline into a self-inflicted hang.
    """

    def __init__(self, proc) -> None:
        self.readers = [_Reader(name, pipe) for name, pipe in
                        (("stdout", proc.stdout), ("stderr", proc.stderr))
                        if pipe is not None]
        for reader in self.readers:
            reader.thread.start()

    def collect(self, budget: float) -> tuple:
        """`(stdout, stderr, complete)` - joined against one shared budget.

        Shared, not per reader: two pipes held open by the same process would
        otherwise cost the budget twice to learn the same thing. `complete` is
        false when any reader was still running when the budget ran out, and
        the caller must not present a partial result as an answer.
        """
        deadline = _deadline(budget)
        for reader in self.readers:
            reader.thread.join(max(0.0, deadline()))
        complete = all(r.ended.is_set() for r in self.readers)
        out = self._of("stdout")
        err = self._of("stderr")
        return out, err, complete

    def _of(self, name: str) -> str:
        for reader in self.readers:
            if reader.name == name:
                return reader.text()
        return ""

    def unfinished(self) -> list:
        """Which readers had not reached the end. For the error message."""
        return [r.name for r in self.readers if not r.ended.is_set()]

    def close(self, budget: float = 2.0) -> list:
        """Close what is safe to close. Returns the readers still running.

        Only a reader that has **already ended** gets its pipe closed. That
        condition is the whole of this method, and it is not caution: a reader
        blocked in `read1` holds the `BufferedReader`'s own lock, and
        `close()` flushes under that same lock, so closing beneath a live
        reader waits for the writer to go - the exact blocking close this
        module exists to avoid, reached from the other side.

        It used to close unconditionally, which was safe only while settlement
        was guaranteed to have ended every writer first. When settlement
        *fails* - a separately sessioned child that could not be terminated,
        still holding both writers - that guarantee is gone, and the close
        blocked past the caller's deadline until an outer supervisor
        intervened. A bound that only holds when nothing went wrong is not a
        bound.

        So a reader that cannot be closed is **retained and named** rather than
        waited on. Its pipe stays open and its thread stays alive, which is a
        real cost and the honest one: the helper is unsettled, its output is
        incomplete, and both facts stay visible on the record instead of being
        traded for a tidy return.
        """
        # Join first, then close what ended. The order matters both ways: a
        # reader whose writers have gone is already finishing, and asking
        # `is_alive()` before giving it the budget reports a race as a
        # retention - which made an ordinary clean run claim it had leaked a
        # pipe. Joining cannot block past the budget, because that is what a
        # budgeted join is.
        deadline = _deadline(budget)
        for reader in self.readers:
            reader.thread.join(max(0.0, deadline()))
        for reader in self.readers:
            if reader.thread.is_alive():
                continue
            try:
                if not reader.pipe.closed:
                    reader.pipe.close()
            except OSError:                              # pragma: no cover
                pass
        return [r.name for r in self.readers if r.thread.is_alive()]

    def retained(self) -> list:
        """Readers still holding a pipe, as `name(fd)` for the record.

        Named rather than counted so an unresolved helper carries identities a
        person can act on - which file descriptor is still open, on which
        stream - rather than the fact that something is.
        """
        out = []
        for reader in self.readers:
            if not reader.thread.is_alive():
                continue
            try:
                fd = reader.pipe.fileno()
            except (OSError, ValueError):                # pragma: no cover
                fd = "?"
            out.append(f"{reader.name}(fd {fd})")
        return out


def _deadline(budget: float):
    """A callable returning the seconds left of one shared budget."""
    import time

    end = time.monotonic() + budget
    return lambda: end - time.monotonic()
