"""Commands that launch a real browser, or read something only a real
browser can answer.

These are the slow ones, and the ones worth running with --detach.
Cookies, TLS, DNS, trust and align all open a session because the whole
point of them is what the browser actually presents, not what the
profile claims."""

from __future__ import annotations

import json
import os
import sys
from pathlib import Path
from .. import jobs
from ..fingerprint import validate
from ..proxypool import proxy_warnings
from ..store import ProfileStore
from .out import C, _c, _no_profile, _print_checks, _resolve_or_fail, emit, json_mode, say


def cmd_launch(args, store: ProfileStore) -> int:
    prof = store.resolve(args.ref)
    if not prof:
        return _no_profile(args.ref)
    from ..drivers import open_session
    from ..launcher import launch  # imported lazily (browser deps optional)
    # This process *is* the session - the server spawns exactly this command as
    # a child - so the lease lives for as long as the command does, and the OS
    # drops it if the process is killed.
    with open_session(prof, store) as prof:
        # Everything about *what* to launch is read from the profile the gate
        # yielded, never from the copy resolved before the lease. The engine
        # and the warnings used to be derived up there: another process
        # switching the engine in that window sent the browser out on the old
        # backend while every stored copy already said the new one, and the
        # warnings graded a fingerprint that had been edited since. An explicit
        # --engine is a decision the user just made and still wins.
        engine = args.engine or prof.engine or "playwright"
        warnings = list(validate(prof.fingerprint))
        if warnings:
            say(_c("Warning: fingerprint has consistency issues:", C.YEL))
            for problem in warnings:
                say(f"  - {problem}")
        for warning in proxy_warnings(prof):
            warnings.append(warning)
            say(_c(f"Warning: {warning}", C.YEL))
        say(_c(f"Launching '{prof.name}' via {engine}...", C.CYN))
        launch(prof, store, headless=args.headless, keep_open=False, engine=engine)
    return emit({"ok": True, "id": prof.id, "name": prof.name, "engine": engine,
                 "headless": bool(args.headless), "warnings": warnings})


def cmd_stop(args, store: ProfileStore) -> int:
    """End the sessions started for a profile with `launch --detach` / `attach --detach`.

    The API's POST /stop terminates the subprocess it launched; the CLI's
    equivalent is the job registry, which is the only place a *different*
    process can find out what is running.
    """
    prof = store.resolve(args.ref)
    if not prof:
        return _no_profile(args.ref)
    stopped = jobs.stop_profile(store.root, prof.id)
    if stopped:
        say(_c(f"Stopped {len(stopped)} session(s) for '{prof.name}'.", C.GRN))
        return emit({"ok": True, "id": prof.id, "name": prof.name,
                     "stopped": [j["id"] for j in stopped]})

    # No job of ours - which is not the same as nothing running. The job
    # registry only knows what this machine's CLI detached; a server, a desktop
    # or another CLI can hold the profile, and "nothing running" was simply
    # false in that state. There is no cross-process stop, so the honest answer
    # is that this command does not own the session.
    from ..paths import session_lease

    if session_lease(store.root, prof.id).held_by_other_process():
        say(_c(f"'{prof.name}' is open in another process, which this command "
               f"cannot stop.", C.YEL))
        return emit({"ok": False, "id": prof.id, "name": prof.name,
                     "stopped": [], "error": "the session belongs to another "
                     "process; stop it where it was started"}, 1)
    say(_c(f"'{prof.name}' has nothing running.", C.DIM))
    return emit({"ok": True, "id": prof.id, "name": prof.name, "stopped": []})


def cmd_jobs(args, store: ProfileStore) -> int:
    sub = args.jobs_command
    if sub == "list":
        rows = jobs.listing(store.root)
        if not rows:
            say(_c("No jobs. Start one by adding --detach to any command.", C.DIM))
            return emit({"jobs": []})
        say(_c(f"{'ID':<14}{'STATE':<10}{'COMMAND':<14}PROFILE", C.B))
        for r in rows:
            colour = C.CYN if r["running"] else (C.GRN if r.get("exit") == 0 else C.YEL)
            say(f"{r['id']:<14}{_c(r['state'], colour):<19}{r.get('command', ''):<14}"
                f"{r.get('profile') or '-'}")
        return emit({"jobs": rows})

    if sub == "clear":
        n = jobs.prune(store.root)
        say(_c(f"Cleared {n} finished job(s).", C.GRN))
        return emit({"cleared": n})

    rec = jobs.get(store.root, args.id)
    if rec is None:
        say(_c(f"No job with id '{args.id}'.", C.RED))
        return emit({"ok": False, "error": f"no job with id '{args.id}'"}, 1)

    if sub == "stop":
        rec = jobs.stop(store.root, args.id)
        say(_c(f"Job {args.id} is now {rec['state']}.", C.GRN))
        return emit(rec)

    lines = jobs.tail(rec, args.lines)
    say(_c(f"job {rec['id']}  ", C.B) + _c(rec["state"], C.CYN))
    say(f"  command      numera {' '.join(rec.get('argv', []))}")
    say(f"  profile      {rec.get('profile') or '-'}")
    say(f"  pid          {rec.get('pid')}")
    if rec.get("exit") is not None:
        say(f"  exit code    {rec['exit']}")
    if lines:
        say(_c(f"\n  last {len(lines)} line(s) of {rec.get('log')}:", C.DIM))
        for line in lines:
            say(f"    {line}")
    return emit({**rec, "tail": lines})


def cmd_attach(args, store: ProfileStore) -> int:
    prof = _resolve_or_fail(args, store)
    if not prof:
        return emit({"ok": False, "error": f"no profile matching '{args.ref}'"}, 1)
    from ..automation import attach, snippets
    from ..drivers import open_session
    say(_c(f"Launching '{prof.name}' with DevTools on port {args.port}...", C.CYN))
    # Held for the whole attached session, same as `launch`.
    with open_session(prof, store) as prof:
        return _attached(args, store, prof, attach, snippets)


def _attached(args, store, prof, attach, snippets) -> int:
    (pw, context, page), info = attach(prof, store, port=args.port, engine=args.engine,
                                       headless=args.headless)
    ws = info.get("webSocketDebuggerUrl", "")
    say(_c(f"\n  {info.get('Browser', 'Browser')} ready — attach your automation:", C.B))
    say(snippets(args.port, ws))
    say(_c("Close the browser window (or press Ctrl+C) to end the session.", C.DIM))
    # Emitted before the wait, not after: the endpoint is the whole point of the
    # command and a caller needs it while the session is still open.
    emit({"ok": True, "id": prof.id, "name": prof.name, "port": args.port,
          "wsUrl": ws, "browser": info.get("Browser", ""),
          "headless": bool(args.headless)})
    sys.stdout.flush()
    try:
        page.wait_for_event("close", timeout=0)
    except KeyboardInterrupt:
        say("\nClosing.")
    finally:
        try:
            context.close()
        finally:
            if pw:
                pw.stop()
    return 0


def cmd_warmup(args, store: ProfileStore) -> int:
    prof = _resolve_or_fail(args, store)
    if not prof:
        return emit({"ok": False, "error": f"no profile matching '{args.ref}'"}, 1)
    from ..warmup import warm_up
    where = f" ({args.preset})" if args.preset and not args.site else ""
    say(_c(f"Warming up '{prof.name}'{where} for {args.minutes} minute(s)...", C.CYN))
    res = warm_up(prof, store, minutes=args.minutes, sites=args.site or None,
                  engine=args.engine, headless=args.headless, preset=args.preset,
                  on_event=say)
    say(_c(f"Done — visited {res['pages']} page(s)"
           + (f", {res['errors']} unreachable" if res["errors"] else "") + ".", C.GRN))
    return emit({"ok": True, "id": prof.id, "name": prof.name, **res})


def cmd_cookies_list(args, store: ProfileStore) -> int:
    prof = _resolve_or_fail(args, store)
    if not prof:
        return emit({"ok": False, "error": f"no profile matching '{args.ref}'"}, 1)
    from .. import cookies as ck
    jar = ck.read(prof, store, engine=args.engine)
    by_domain: dict[str, int] = {}
    for c in jar:
        by_domain[c["domain"]] = by_domain.get(c["domain"], 0) + 1
    if not jar:
        say(_c(f"'{prof.name}' has no cookies yet.", C.DIM))
        return emit({"total": 0, "domains": {}})
    say(_c(f"{len(jar)} cookie(s) across {len(by_domain)} domain(s):", C.B))
    for domain, n in sorted(by_domain.items(), key=lambda kv: -kv[1]):
        say(f"  {domain:<40}{_c(str(n), C.DIM)}")
    return emit({"total": len(jar), "domains": by_domain})


def cmd_cookies_export(args, store: ProfileStore) -> int:
    prof = _resolve_or_fail(args, store)
    if not prof:
        return emit({"ok": False, "error": f"no profile matching '{args.ref}'"}, 1)
    from .. import cookies as ck
    say(_c(f"Reading cookies from '{prof.name}'...", C.CYN))
    jar = ck.read(prof, store, engine=args.engine)
    # A .txt destination almost always means "give me curl's cookies.txt".
    fmt = args.format or ("netscape" if str(args.file).lower().endswith(".txt") else "json")
    Path(args.file).write_text(ck.dumps(jar, fmt), encoding="utf-8")
    say(_c(f"Exported {len(jar)} cookie(s) to {args.file}  ", C.GRN) + _c(f"({fmt})", C.DIM))
    return emit({"ok": True, "count": len(jar), "file": str(args.file), "format": fmt})


def cmd_cookies_import(args, store: ProfileStore) -> int:
    prof = _resolve_or_fail(args, store)
    if not prof:
        return emit({"ok": False, "error": f"no profile matching '{args.ref}'"}, 1)
    from .. import cookies as ck
    jar = ck.read_file(args.file)
    if not jar:
        say(_c(f"No usable cookies found in {args.file}.", C.RED))
        return emit({"ok": False, "error": f"no usable cookies found in {args.file}"}, 1)
    say(_c(f"Importing {len(jar)} cookie(s) into '{prof.name}'...", C.CYN))
    n = ck.write(prof, store, jar, clear=args.clear, engine=args.engine)
    note = _c("  (existing cookies were cleared first)", C.DIM) if args.clear else ""
    say(_c(f"Imported {n} cookie(s).", C.GRN) + note)
    return emit({"ok": True, "imported": n, "cleared": bool(args.clear)})


def cmd_proxy_test(args, store: ProfileStore) -> int:
    prof = _resolve_or_fail(args, store)
    if not prof:
        return emit({"ok": False, "error": f"no profile matching '{args.ref}'"}, 1)
    from ..probe import check_proxy
    where = prof.proxy.server if prof.proxy else "no proxy (direct connection)"
    say(_c(f"Testing '{prof.name}' via {where}...", C.CYN))
    res = check_proxy(prof, store, engine=args.engine)
    if res.get("error"):
        say(_c(f"Failed: {res['error']}", C.RED))
        return emit(res, 1)
    say(f"  exit IP      {res['ip']}"
        + (f"  ({res['city']}, {res['country']})" if res.get("city") else ""))
    say(f"  round trip   {res['latencyMs']} ms")
    _print_checks(res["checks"])
    return emit(res, 0 if res["ok"] else 1)


def cmd_tls(args, store: ProfileStore) -> int:
    prof = _resolve_or_fail(args, store)
    if not prof:
        return emit({"ok": False, "error": f"no profile matching '{args.ref}'"}, 1)
    from ..probe import check_tls
    say(_c(f"Reading '{prof.name}' TLS/HTTP2 handshake...", C.CYN))
    res = check_tls(prof, store, engine=args.engine)
    if res.get("error"):
        say(_c(f"Failed: {res['error']}", C.RED))
        return emit(res, 1)
    say(f"  JA3 hash     {res.get('ja3Hash') or '-'}")
    say(f"  JA4          {res.get('ja4') or '-'}")
    say(f"  HTTP/2       {res.get('http2') or '-'}")
    _print_checks(res["checks"])
    say(_c("\n  Tip: run this on two engines and compare — TLS is the one layer "
           "JS can't touch.", C.DIM))
    return emit(res, 0 if res["ok"] else 1)


def cmd_dns(args, store: ProfileStore) -> int:
    prof = _resolve_or_fail(args, store)
    if not prof:
        return emit({"ok": False, "error": f"no profile matching '{args.ref}'"}, 1)
    from ..probe import check_dns
    say(_c(f"Checking '{prof.name}' for DNS leaks...", C.CYN))
    res = check_dns(prof, store, engine=args.engine)
    if res.get("error"):
        say(_c(f"Failed: {res['error']}", C.RED))
        return emit(res, 1)
    if res.get("countries"):
        say(f"  resolvers    {len(res['resolvers'])} in {', '.join(res['countries'])}")
    _print_checks(res["checks"])
    return emit(res, 0 if res["ok"] else 1)


def cmd_trust(args, store: ProfileStore) -> int:
    prof = _resolve_or_fail(args, store)
    if not prof:
        return emit({"ok": False, "error": f"no profile matching '{args.ref}'"}, 1)
    from ..probe import trust_report
    say(_c(f"Inspecting '{prof.name}' the way a fingerprinting script would...", C.CYN))
    rep = trust_report(prof, store, engine=args.engine)
    colour = C.GRN if rep["score"] >= 95 else C.YEL if rep["score"] >= 70 else C.RED
    say(_c(f"\n  Trust score: {rep['score']}%  (grade {rep['grade']}, "
           f"{rep['passed']}/{rep['total']} checks)\n", colour))
    _print_checks(rep["checks"])
    if json_mode():
        # Zero even for a poor grade. server.py runs `trust --json` *without*
        # allow_fail, so a non-zero exit here turns every profile scoring under
        # 100% into a 500 behind the dashboard's Trust button. The grade is in
        # the payload; a bad grade is an answer, not a failed command.
        return emit(rep)
    return 0 if rep["score"] == 100 else 1


def cmd_align(args, store: ProfileStore) -> int:
    prof = _resolve_or_fail(args, store)
    if not prof:
        return emit({"ok": False, "error": f"no profile matching '{args.ref}'"}, 1)
    from ..probe import align_to_reality
    say(_c(f"Aligning '{prof.name}' to what its browser actually presents...", C.CYN))
    res = align_to_reality(prof, store, engine=args.engine)
    b, a = res["before"], res["after"]
    colour = C.GRN if a["score"] >= 95 else C.YEL if a["score"] >= 70 else C.RED
    say(_c(f"\n  grade {b['grade']} ({b['score']}%)  ->  grade {a['grade']} ({a['score']}%)"
           f"   {a['passed']}/{a['total']} checks\n", colour))
    if res["changed"]:
        say(_c("  Adjusted: ", C.B) + _c(", ".join(res["changed"]), C.DIM))
        say(_c(f"  Identity now reads as: {res['os']}", C.DIM))
    else:
        say(_c("  Nothing to change — the profile already matches its browser.", C.DIM))
    say("")
    _print_checks(res["checks"])
    if json_mode():
        return emit(res)          # same contract as `trust --json` above
    return 0 if a["score"] == 100 else 1
