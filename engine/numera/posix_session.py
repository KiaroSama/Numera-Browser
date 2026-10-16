"""The POSIX half of tree ownership: process groups and the terminal.

Split out of `proctree` for the same reason as `winjob`: reading `/proc` for a
group's survivors, and the `setpgrp` + `tcsetpgrp` dance that gives this
process a group of its own without going deaf to Ctrl+C, are one platform's
mechanics rather than anything about trees. `proctree` imports these names
unchanged.

Nothing here imports `proctree`, so the dependency runs one way only.
"""
from __future__ import annotations

import atexit
import errno
import os
import signal
import subprocess
import sys
import threading
from pathlib import Path
from typing import Optional


_OBSERVATION = threading.local()


def _group_is_running(pgid: int, ignore: int = 0) -> bool:
    """Is anything in this group more than a corpse?

    A process that has exited but has not been reaped stays in its process
    group and goes on answering `killpg(pgid, 0)`, so a group holding nothing
    but zombies read as a live browser - and `end_tree` could never prove
    otherwise, however many signals it sent. A zombie holds no files, no port
    and no profile directory; it is gone in every sense this module is about.

    Conservative when it cannot tell: an unreadable process table answers
    "still running", so a stop reports failure rather than a success it has not
    established.
    """
    procfs = Path("/proc")
    if procfs.is_dir():
        uncertain = False
        try:
            for entry in procfs.iterdir():
                if not entry.name.isdigit():
                    continue
                try:
                    pid = int(entry.name)
                    if pid == ignore:
                        continue
                    # After the comm field: state, ppid, pgrp.
                    text = (entry / "stat").read_text(encoding="utf-8", errors="replace")
                    fields = text[text.rindex(")") + 2:].split()
                    if int(fields[2]) == pgid and fields[0] != "Z":
                        return True
                except OSError as error:
                    if error.errno not in (errno.ENOENT, errno.ESRCH):
                        uncertain = True
                except (ValueError, IndexError):
                    uncertain = True
        except OSError:
            return True
        # A disappearing entry is normal; an unreadable one could belong to
        # this group. Do not turn incomplete observation into an empty tree.
        return uncertain
    running, diagnostic = _ps_group(pgid, ignore)
    _OBSERVATION.last = (pgid, diagnostic)
    return running


def _ps_group(pgid: int, ignore: int = 0):
    """Return fail-closed state plus bounded numeric-only failure evidence."""
    try:
        out = subprocess.run(["ps", "-A", "-o", "pgid=,state=,pid="],
                             capture_output=True, text=True, encoding='utf-8', errors='strict', timeout=5)
        if out.returncode != 0 or not out.stdout.strip():
            return True, f'query_exit={out.returncode} empty={not out.stdout.strip()}'
        for index, line in enumerate(out.stdout.splitlines()):
            if not line.strip():
                continue
            parts = line.split()
            if len(parts) != 3:
                return True, f'malformed_row={index} fields={len(parts)}'
            group, state, pid = int(parts[0]), parts[1], int(parts[2])
            if group < 0 or pid <= 0:
                return True, f'invalid_identity_row={index} group={group} pid={pid}'
            if group == pgid and not state.startswith("Z") and pid != ignore:
                return True, f'group={group} pid={pid} state_codes={tuple(map(ord, state[:8]))}'
        return False, 'no_live_members'
    except Exception as error:  # An unavailable or incomplete table is unknown.
        return True, f'query_error={type(error).__name__}'


def report_group_failure(pgid: int, *, graceful: bool, forced: bool, verified: bool):
    if Path('/proc').is_dir():
        return
    observed_group, evidence = getattr(_OBSERVATION, 'last', (None, 'no_ps_observation'))
    if observed_group != pgid:
        evidence = 'no_matching_ps_observation'
    try:
        print(f'POSIX cleanup unproved group={pgid} verified={verified} '
              f'graceful_signal={graceful} force_signal={forced} {evidence}', file=sys.stderr)
    except OSError:
        pass  # Diagnostics never turn an unproved cleanup into a new exception.


def _adopt_group() -> bool:
    """Put this process in a process group of its own. True if it now leads one.

    `setpgrp()` alone is not enough when there is a terminal: the kernel
    delivers Ctrl+C to the terminal's *foreground* group, so a process that
    leaves that group silently stops being interruptible. So the group is made
    and then handed the terminal, which is the same dance a shell does for
    every job it starts - and the old foreground group is put back on the way
    out, or the shell is left talking to a group that no longer exists.

    `tcsetpgrp` from a process that is not currently the foreground group
    raises SIGTTOU at itself, which would stop us; it is ignored across the
    call and restored afterwards.

    **All of it or none of it.** This moves the *embedding* process, so a half
    finished adoption is not a failed launch, it is a host left somewhere it
    never asked to be. `signal.signal` raises `ValueError` off the main thread,
    and it did so after `setpgrp()` had already moved the process: an
    application opening its first browser from a worker thread ended up in a
    group of its own, out of the terminal's foreground group and therefore deaf
    to Ctrl+C, and the `ValueError` was not even the refusal `own_tree` knows
    how to report. Anything that cannot be finished is now put back, and the
    caller is told it has no tree.
    """
    was = os.getpgrp()
    try:
        os.setpgrp()
    except OSError:                                          # pragma: no cover
        return False
    fd = _terminal()
    if fd is None:
        return True                     # nothing to hand over; nothing to lose
    try:
        previous = os.tcgetpgrp(fd)
        handler = signal.signal(signal.SIGTTOU, signal.SIG_IGN)
    except OSError:                                          # pragma: no cover
        return True                     # no controlling terminal after all
    except ValueError:
        # Not the main thread, so signals cannot be touched here and the
        # terminal cannot be taken safely. Leaving the group change standing
        # would silence Ctrl+C for the whole host.
        _rejoin(was)
        return False
    try:
        os.tcsetpgrp(fd, os.getpgrp())
    except OSError:                                          # pragma: no cover
        _rejoin(was)
        return False
    finally:
        signal.signal(signal.SIGTTOU, handler)
    atexit.register(_restore_terminal, fd, previous)
    return True


def _rejoin(group: int) -> None:
    """Undo `setpgrp`, back into the group this process started in."""
    try:
        os.setpgid(0, group)
    except OSError:                                          # pragma: no cover
        pass


def _terminal() -> "Optional[int]":
    """A descriptor for the controlling terminal, or None if there is none."""
    for fd in (0, 1, 2):
        try:
            if os.isatty(fd) and os.tcgetpgrp(fd) >= 0:
                return fd
        except OSError:
            continue
    return None


def _restore_terminal(fd: int, group: int) -> None:            # pragma: no cover
    """Give the terminal back to whoever had it before we took it."""
    try:
        handler = signal.signal(signal.SIGTTOU, signal.SIG_IGN)
        try:
            os.tcsetpgrp(fd, group)
        finally:
            signal.signal(signal.SIGTTOU, handler)
    except (OSError, ValueError):
        pass
