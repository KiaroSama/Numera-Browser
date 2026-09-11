"""How Numera re-invokes Numera.

Several things in here start another copy of this program: the API's launch,
stop, attach and warm-up routes, a detached CLI job, the scheduler, and every
MCP tool call. They all used the same shape:

    [sys.executable, "-m", "numera", *argv]

which is correct from a source checkout and **wrong in a frozen build**.
PyInstaller documents that a frozen application's `sys.executable` is the
bundled executable, not a Python interpreter, so that list becomes

    numera.exe -m numera --data-dir ... launch <id>

and `-m numera` is handed to this program's own argument parser, which has no
such option. Every self-spawn in a portable build failed on its first argument.
Nothing in a CLI smoke test or `/api/ready` reaches it, because neither of those
re-invokes the program - which is how it survived a full release verification.

One builder rather than a rule each call site remembers: the call sites are
exactly what drifted, and `test_self_command.py` asserts none of them builds the
command by hand again.

Nothing else about a self-spawn changes here. Secrets still travel in the
environment and never in argv, the data root is still passed by the caller, and
the caller still owns the process tree (see `proctree`).
"""

from __future__ import annotations

import sys


def frozen() -> bool:
    """Is this a frozen build rather than a source checkout?

    The same test `paths.app_dir()` has always used, named once so the two
    cannot answer differently.
    """
    return bool(getattr(sys, "frozen", False))


def self_command(*argv: str) -> list:
    """The command that runs this program again with `argv`.

    Returned as a list and always passed to `Popen`/`run` as a list, so a path
    with spaces needs no quoting and nothing is re-parsed by a shell.
    """
    if frozen():
        return [sys.executable, *argv]
    return [sys.executable, "-m", "numera", *argv]
