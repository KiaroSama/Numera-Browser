"""
Command-line interface for Numera.

    numera create <name> [--os] [--locale] [--proxy] [--seed] [--mobile]
    numera list [--tag TAG]
    numera show <name|id>
    numera launch <name|id> [--headless]
    numera stop <name|id>                        (end its running session)
    numera edit <name|id> [--notes] [--add-tag] [--proxy]
    numera regen <name|id> [--os] [--locale]   (new fingerprint, same identity slot)
    numera delete <name|id>
    numera export <name|id> <file>
    numera import <file>
    numera backup <name|id> <file.zip>           (profile + whole session, portable)
    numera restore <file.zip> [--new-id]         (recreate a profile from a backup)
    numera collisions [--json]                   (profiles that share a fingerprint)
    numera check <name|id>                       (validate fingerprint coherence)
    numera health                                (one row per profile: the whole fleet)
    numera trust <name|id>                       (score it like a fingerprinter would)
    numera proxy test <name|id>                  (exit IP, country + WebRTC leaks)
    numera dns <name|id>                         (do DNS lookups leak past the proxy?)
    numera tls <name|id>                         (TLS/JA3 handshake — the pre-JS layer)
    numera cookies list|export|import <name|id>  (move a session in or out)
    numera ext list|add|remove <name|id> [path]  (browser extensions)
    numera warmup <name|id> [--minutes] [--preset]  (age a fresh profile by browsing)
    numera schedule add|list|update|remove|run-due  (recurring unattended warm-up)
    numera pool list|add|remove|test|assign      (the shared proxy pool)
    numera jobs list|show|stop|clear             (background work started with --detach)
    numera attach <name|id> [--port]             (open it for Selenium/Puppeteer/Playwright)
    numera apply [refs] [--tag|--all] --engine …  (one change, many profiles)
    numera config [--default-engine] [--headless-launch]   (show/set settings)
    numera engines                               (list launch engines + install status)
    numera default-engine [name]                 (show/set the engine new profiles use)
    numera import-from <file>                    (import from GoLogin/AdsPower/Multilogin)
    numera vault enable|status                   (encrypt proxy passwords at rest)
    numera serve [--host] [--port]               (run the HTTP API for the dashboard)
    numera mcp                                   (speak MCP on stdio; every command a tool)

Two flags are on every command, for callers that are not people:

``--json``   prints the result as one JSON object on stdout and nothing else.
             The prose a human reads is suppressed, not merely decorated, so a
             script never has to parse around it.
``--detach`` runs the command in the background and prints a job handle instead
             of blocking. Poll it with ``numera jobs show <id>``, end it with
             ``numera jobs stop <id>``. See jobs.py.
"""

from __future__ import annotations

import json
import os
import sys
from typing import Optional
from .. import jobs
from ..store import ProfileStore
from .out import C, _c, emit, say, set_json_mode
from .parser import build_parser
# Re-exported so `from numera.cli import main` and `cli.build_parser()`
# keep working: the console script, __main__.py, mcp.py and the parity
# test all reach for this module by name.
from .parser import build_parser
from .out import C, _c, emit, say


def main(argv: Optional[list[str]] = None) -> int:
    argv = list(sys.argv[1:] if argv is None else argv)
    parser = build_parser()
    args = parser.parse_args(argv)
    set_json_mode(getattr(args, "json", False))
    try:
        store = ProfileStore(args.data_dir, password=args.password)
    except OSError as e:
        # Constructing the store is where the data directory is created, so this
        # is the first thing that fails on an unwritable one - and it happens
        # before the try block below could catch it.
        say(_c(str(e), C.RED))
        return emit({"ok": False, "error": str(e)}, 1)

    code = 0
    try:
        if getattr(args, "detach", False):
            # Inside the handler, not before it. `_detach_target` resolves the
            # profile, and resolution can fail for a reason the user needs
            # stated - an ambiguous legacy name raises `AmbiguousProfile` - so
            # running it out here meant the foreground command printed a clean
            # JSON error while the same command with `--detach` printed a
            # traceback and spawned nothing anyone could ask about.
            rec = jobs.spawn(store.root, jobs.child_argv(argv, store.root),
                             command=args.command,
                             profile=_detach_target(args, store),
                             password=store._password)
            say(_c(f"Started job {rec['id']}", C.GRN)
                + _c(f"  (pid {rec['pid']}, log {rec['log']})", C.DIM))
            say(_c(f"  poll it with:  numera jobs show {rec['id']}", C.DIM))
            # Through `code`, because the `finally` below reports it.
            code = emit(rec)
            return code

        code = args.func(args, store)
    # OSError alongside RuntimeError: a data directory that cannot be written,
    # a disk that is full, a file another process holds. Those carry a message
    # written for the user, and printing forty lines of `pathlib` stack above it
    # buries the one line that says what to do - which is how an unwritable
    # `/data` in a container read as "the app is broken".
    except (RuntimeError, OSError) as e:
        say(_c(str(e), C.RED))
        code = emit({"ok": False, "error": str(e)}, 1)
    except KeyboardInterrupt:
        say("\nAborted.")
        code = 130
    finally:
        # Started by `--detach`? Record how it went, so `numera jobs show` can
        # report an outcome instead of only "the process is gone".
        job_file = os.environ.get("NUMERA_JOB_FILE")
        if job_file:
            jobs.record_exit(job_file, code)
    return code


def _detach_target(args, store: ProfileStore) -> Optional[str]:
    """The profile id a detached job belongs to, so `numera stop <ref>` can find it."""
    ref = getattr(args, "ref", None)
    if not isinstance(ref, str):
        return None
    prof = store.resolve(ref)
    return prof.id if prof else None


if __name__ == "__main__":
    raise SystemExit(main())
