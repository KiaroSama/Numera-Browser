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

import argparse
import json
import os
import sys
from pathlib import Path
from typing import Optional

from . import __version__
from . import jobs
from .fingerprint import generate, validate
from .models import Profile, Proxy
from .proxypool import proxy_warnings
from .store import ProfileStore


# ---- output -------------------------------------------------------------
# One process, two audiences. `say` is for the person at the prompt; `emit` is
# the result an agent parses. Under --json only `emit` writes to stdout, so the
# stream holds exactly one JSON object and nothing else.
_JSON = False


class C:
    B = "\033[1m"; DIM = "\033[2m"; GRN = "\033[32m"; RED = "\033[31m"
    YEL = "\033[33m"; CYN = "\033[36m"; END = "\033[0m"


def _c(text: str, color: str) -> str:
    return f"{color}{text}{C.END}" if sys.stdout.isatty() else text


def say(*args, **kwargs) -> None:
    """Human narration. Silent under --json."""
    if not _JSON:
        print(*args, **kwargs)


def emit(payload, code: int = 0) -> int:
    """The command's machine-readable result. Printed only under --json."""
    if _JSON:
        print(json.dumps(payload, default=str))
    return code


def _confirm(prompt: str) -> bool:
    """Ask before something destructive.

    A --json caller has no keyboard, so a missing ``-y`` is an error there
    rather than a prompt nobody will ever answer.
    """
    if _JSON:
        raise RuntimeError("this needs confirmation — pass -y "
                           "(a --json run never prompts)")
    try:
        return input(prompt).strip().lower() == "y"
    except EOFError:
        # A detached job has no stdin either. Say so, rather than traceback.
        raise RuntimeError("nothing to read the confirmation from — pass -y") from None


def _parse_proxy(raw: Optional[str], country: Optional[str]) -> Optional[Proxy]:
    """Accept 'scheme://[user:pass@]host:port' and return a Proxy."""
    if not raw:
        return None
    server, user, pw = raw, None, None
    if "@" in raw:
        scheme, rest = raw.split("://", 1) if "://" in raw else ("http", raw)
        creds, hostport = rest.rsplit("@", 1)
        if ":" in creds:
            user, pw = creds.split(":", 1)
        server = f"{scheme}://{hostport}"
    return Proxy(server=server, username=user, password=pw, country=country)


def _row(prof: Profile) -> dict:
    """The summary of a profile that `list` and `health` report.

    Deliberately not ``to_dict()``: that carries the proxy password, and a
    fleet-wide listing is the last place it should turn up. ``show`` and
    ``export`` are the full-fidelity paths.
    """
    fp = prof.fingerprint
    return {
        "id": prof.id, "name": prof.name, "engine": prof.engine,
        "os": fp.os, "locale": fp.locale, "timezone": fp.timezone,
        "mobile": fp.is_mobile, "tags": list(prof.tags),
        "proxy": prof.proxy.server if prof.proxy else None,
        "proxyCountry": prof.proxy.country if prof.proxy else None,
    }


# ---- commands ------------------------------------------------------------
def cmd_create(args, store: ProfileStore) -> int:
    if store.find_by_name(args.name):
        say(_c(f"A profile named '{args.name}' already exists.", C.RED))
        return emit({"ok": False, "error": f"a profile named '{args.name}' already exists"}, 1)
    proxy = _parse_proxy(args.proxy, args.country)
    fp = generate(seed=args.seed, os=args.os, locale=args.locale,
                  proxy=proxy, mobile=args.mobile)
    # No --engine given? Fall back to the saved default (CloakBrowser first).
    # If one IS given, remember it as the new default ("last engine wins").
    engine = args.engine or store.get_default_engine()
    if args.engine:
        store.set_default_engine(args.engine)
    from . import drivers
    installed = engine not in drivers.names() or drivers.is_installed(engine)
    if not installed:
        say(_c(f"Note: engine '{engine}' isn't installed yet; install it before launching.", C.YEL))
    prof = Profile(name=args.name, fingerprint=fp, proxy=proxy,
                   notes=args.notes or "", tags=args.tag or [], engine=engine,
                   extensions=store.get_default_extensions())
    store.save(prof)
    say(_c(f"Created profile '{prof.name}'  ", C.GRN) + _c(f"[{prof.id}]", C.DIM))
    _print_summary(prof)
    return emit({"ok": True, "profile": prof.to_dict(), "engineInstalled": installed})


def cmd_list(args, store: ProfileStore) -> int:
    profiles = store.list()
    if args.tag:
        profiles = [p for p in profiles if args.tag in p.tags]
    if not profiles:
        say(_c("No profiles yet. Create one with:  numera create <name>", C.DIM))
        return emit({"total": 0, "profiles": []})
    say(_c(f"{'ID':<14}{'NAME':<22}{'OS':<9}{'LOCALE':<9}PROXY", C.B))
    for p in profiles:
        proxy = p.proxy.server if p.proxy else "-"
        say(f"{_c(p.id, C.DIM):<23}{p.name:<22}{p.fingerprint.os:<9}"
            f"{p.fingerprint.locale:<9}{proxy}")
    say(_c(f"\n{len(profiles)} profile(s).", C.DIM))
    return emit({"total": len(profiles), "profiles": [_row(p) for p in profiles]})


def cmd_show(args, store: ProfileStore) -> int:
    prof = store.resolve(args.ref)
    if not prof:
        return _no_profile(args.ref)
    _print_summary(prof, full=True)
    return emit(prof.to_dict())


def cmd_launch(args, store: ProfileStore) -> int:
    prof = store.resolve(args.ref)
    if not prof:
        return _no_profile(args.ref)
    warnings = list(validate(prof.fingerprint))
    if warnings:
        say(_c("Warning: fingerprint has consistency issues:", C.YEL))
        for p in warnings:
            say(f"  - {p}")
    for warning in proxy_warnings(prof):
        warnings.append(warning)
        say(_c(f"Warning: {warning}", C.YEL))
    engine = args.engine or prof.engine or "playwright"
    say(_c(f"Launching '{prof.name}' via {engine}...", C.CYN))
    from .launcher import launch  # imported lazily (browser deps optional)
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
    else:
        say(_c(f"'{prof.name}' has nothing running.", C.DIM))
    return emit({"ok": True, "id": prof.id, "name": prof.name,
                 "stopped": [j["id"] for j in stopped]})


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


def cmd_engines(args, store: ProfileStore) -> int:
    from . import drivers
    default = store.get_default_engine()
    say(_c("Launch engines:", C.B))
    for name, ok in drivers.available().items():
        mark = _c("installed", C.GRN) if ok else _c("not installed", C.DIM)
        star = _c("  <- default (new profiles)", C.CYN) if name == default else ""
        say(f"  {name:<14}{mark}{star}")
    return emit({"engines": drivers.available(), "default": default})


def cmd_default_engine(args, store: ProfileStore) -> int:
    from . import drivers
    if not args.name:
        say(_c(f"Default engine for new profiles: ", C.B) + _c(store.get_default_engine(), C.CYN))
        say(_c(f"Set it with:  numera default-engine <{'|'.join(drivers.names())}>", C.DIM))
        return emit({"default_engine": store.get_default_engine()})
    if args.name not in drivers.names():
        say(_c(f"Unknown engine '{args.name}'. Choose one of: {', '.join(drivers.names())}.", C.RED))
        return emit({"ok": False, "error": f"unknown engine '{args.name}'",
                     "known": drivers.names()}, 1)
    store.set_default_engine(args.name)
    note = "" if drivers.is_installed(args.name) else _c("  (not installed yet — install it before launching)", C.YEL)
    say(_c(f"Default engine for new profiles is now '{args.name}'.", C.GRN) + note)
    return emit({"default_engine": args.name,
                 "installed": drivers.is_installed(args.name)})


def cmd_config(args, store: ProfileStore) -> int:
    """Show or change the settings the dashboard exposes on GET/PUT /api/config."""
    changed = []
    if args.default_engine is not None:
        from . import drivers
        if args.default_engine not in drivers.names():
            say(_c(f"Unknown engine '{args.default_engine}'.", C.RED))
            return emit({"ok": False, "error": f"unknown engine '{args.default_engine}'",
                         "known": drivers.names()}, 1)
        store.set_default_engine(args.default_engine)
        changed.append("default_engine")
    if args.headless_launch is not None:
        store.set_headless_launch(args.headless_launch == "on")
        changed.append("headless_launch")
    if args.default_extension is not None:
        store.set_default_extensions([str(Path(e).expanduser()) for e in args.default_extension if e])
        changed.append("default_extensions")

    cfg = {
        "default_engine": store.get_default_engine(),
        "default_extensions": store.get_default_extensions(),
        "headless_launch": store.get_headless_launch(),
        "root": str(store.root),
        "version": __version__,
    }
    if changed:
        say(_c(f"Updated: {', '.join(changed)}", C.GRN))
    say(_c("Settings:", C.B))
    for k in ("default_engine", "headless_launch", "root", "version"):
        say(f"  {_c(k + ':', C.DIM):<28}{cfg[k]}")
    say(f"  {_c('default_extensions:', C.DIM):<28}"
        f"{', '.join(cfg['default_extensions']) or '-'}")
    return emit(cfg)


def cmd_edit(args, store: ProfileStore) -> int:
    prof = store.resolve(args.ref)
    if not prof:
        return _no_profile(args.ref)
    if args.notes is not None:
        prof.notes = args.notes
    if args.add_tag:
        for t in args.add_tag:
            if t not in prof.tags:
                prof.tags.append(t)
    if args.proxy is not None:
        prof.proxy = _parse_proxy(args.proxy, args.country) if args.proxy else None
    store.save(prof)
    say(_c(f"Updated '{prof.name}'.", C.GRN))
    return emit({"ok": True, "profile": prof.to_dict()})


def cmd_regen(args, store: ProfileStore) -> int:
    prof = store.resolve(args.ref)
    if not prof:
        return _no_profile(args.ref)
    prof.fingerprint = generate(os=args.os, locale=args.locale, proxy=prof.proxy)
    store.save(prof)
    say(_c(f"Regenerated fingerprint for '{prof.name}'.", C.GRN))
    _print_summary(prof)
    return emit({"ok": True, "profile": prof.to_dict()})


def cmd_delete(args, store: ProfileStore) -> int:
    prof = store.resolve(args.ref)
    if not prof:
        return _no_profile(args.ref)
    if not args.yes and not _confirm(
            f"Delete '{prof.name}' [{prof.id}]? This removes its session too. (y/N) "):
        say("Aborted.")
        return emit({"ok": False, "aborted": True})
    store.delete(prof.id)
    say(_c(f"Deleted '{prof.name}'.", C.GRN))
    return emit({"ok": True, "id": prof.id, "name": prof.name})


def cmd_export(args, store: ProfileStore) -> int:
    prof = store.resolve(args.ref)
    if not prof:
        return _no_profile(args.ref)
    dest = store.export(prof.id, args.file)
    say(_c(f"Exported to {dest}", C.GRN))
    return emit({"ok": True, "id": prof.id, "name": prof.name, "file": str(dest)})


def cmd_import(args, store: ProfileStore) -> int:
    prof = store.import_file(args.file)
    say(_c(f"Imported '{prof.name}'  ", C.GRN) + _c(f"[{prof.id}]", C.DIM))
    return emit({"ok": True, "profile": prof.to_dict()})


def cmd_backup(args, store: ProfileStore) -> int:
    prof = store.resolve(args.ref)
    if not prof:
        return _no_profile(args.ref)
    dest = store.backup(prof.id, args.file)
    say(_c(f"Backed up '{prof.name}' (profile + session) to {dest}", C.GRN))
    say(_c("  keep it safe — the archive holds the proxy password in plaintext.", C.DIM))
    return emit({"ok": True, "id": prof.id, "name": prof.name, "file": str(dest)})


def cmd_restore(args, store: ProfileStore) -> int:
    prof = store.restore(args.file, new_id=args.new_id)
    say(_c(f"Restored '{prof.name}'  ", C.GRN) + _c(f"[{prof.id}]", C.DIM))
    return emit({"ok": True, "profile": prof.to_dict()})


def cmd_collisions(args, store: ProfileStore) -> int:
    from .collision import find_collisions
    profiles = store.list()
    groups = find_collisions(profiles)
    if not groups:
        say(_c(f"No collisions — all {len(profiles)} profile(s) look unique.", C.GRN))
        return emit({"total": len(profiles), "groups": groups})
    clashing = sum(len(g["members"]) for g in groups)
    say(_c(f"{clashing} of {len(profiles)} profiles share a fingerprint "
           f"({len(groups)} group(s)):\n", C.YEL))
    for g in groups:
        tag = _c(" IDENTICAL (same seed too)", C.RED) if g["identical"] else ""
        names = ", ".join(m["name"] for m in g["members"])
        sig = g["signals"]
        say(_c(f"  • {names}{tag}", C.B))
        say(_c(f"    {sig['os']} · {sig['webgl_renderer']} · "
               f"{sig['screen_width']}x{sig['screen_height']} · "
               f"{sig['hardware_concurrency']} cores / {sig['device_memory']}GB", C.DIM))
    say(_c("\nRegenerate one side of each pair:  numera regen <name>", C.DIM))
    return emit({"total": len(profiles), "groups": groups}, 1)


def _no_profile(ref: str) -> int:
    say(_c(f"No profile matching '{ref}'.", C.RED))
    return emit({"ok": False, "error": f"no profile matching '{ref}'"}, 1)


def _resolve_or_fail(args, store: ProfileStore) -> Optional[Profile]:
    prof = store.resolve(args.ref)
    if not prof:
        say(_c(f"No profile matching '{args.ref}'.", C.RED))
    return prof


def cmd_cookies_list(args, store: ProfileStore) -> int:
    prof = _resolve_or_fail(args, store)
    if not prof:
        return emit({"ok": False, "error": f"no profile matching '{args.ref}'"}, 1)
    from . import cookies as ck
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
    from . import cookies as ck
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
    from . import cookies as ck
    jar = ck.read_file(args.file)
    if not jar:
        say(_c(f"No usable cookies found in {args.file}.", C.RED))
        return emit({"ok": False, "error": f"no usable cookies found in {args.file}"}, 1)
    say(_c(f"Importing {len(jar)} cookie(s) into '{prof.name}'...", C.CYN))
    n = ck.write(prof, store, jar, clear=args.clear, engine=args.engine)
    note = _c("  (existing cookies were cleared first)", C.DIM) if args.clear else ""
    say(_c(f"Imported {n} cookie(s).", C.GRN) + note)
    return emit({"ok": True, "imported": n, "cleared": bool(args.clear)})


def _print_checks(checks: list[dict]) -> None:
    for c in checks:
        mark = _c("PASS", C.GRN) if c["ok"] else _c("FAIL", C.RED)
        say(f"  [{mark}] {c['name']}")
        if c.get("detail") and not c["ok"]:
            say(f"         {_c(c['detail'], C.DIM)}")


def cmd_proxy_test(args, store: ProfileStore) -> int:
    prof = _resolve_or_fail(args, store)
    if not prof:
        return emit({"ok": False, "error": f"no profile matching '{args.ref}'"}, 1)
    from .probe import check_proxy
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
    from .probe import check_tls
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
    from .probe import check_dns
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
    from .probe import trust_report
    say(_c(f"Inspecting '{prof.name}' the way a fingerprinting script would...", C.CYN))
    rep = trust_report(prof, store, engine=args.engine)
    colour = C.GRN if rep["score"] >= 95 else C.YEL if rep["score"] >= 70 else C.RED
    say(_c(f"\n  Trust score: {rep['score']}%  (grade {rep['grade']}, "
           f"{rep['passed']}/{rep['total']} checks)\n", colour))
    _print_checks(rep["checks"])
    if _JSON:
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
    from .probe import align_to_reality
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
    if _JSON:
        return emit(res)          # same contract as `trust --json` above
    return 0 if a["score"] == 100 else 1


def cmd_bulk_create(args, store: ProfileStore) -> int:
    from .bulk import generate_profiles
    engine = args.engine or store.get_default_engine()
    profiles = generate_profiles(
        args.count,
        oses=args.os or None,
        locales=args.locale or None,
        chrome_versions=args.chrome or None,
        engine=engine,
        name_prefix=args.prefix,
        start_index=args.start,
        pad=args.pad,
        tags=args.tag or None,
        mobile=args.mobile,
        seed=args.seed,
    )
    existing = {p.name for p in store.list()}
    ids, skipped = [], 0
    for prof in profiles:
        if prof.name in existing:
            skipped += 1
            continue
        store.save(prof)
        existing.add(prof.name)
        ids.append(prof.id)
    if args.engine:
        store.set_default_engine(args.engine)
    note = _c(f", skipped {skipped} name clash(es)", C.YEL) if skipped else ""
    say(_c(f"Created {len(ids)} profile(s)", C.GRN) + note
        + _c(f"  (engine {engine})", C.DIM))
    return emit({"created": len(ids), "skipped": skipped, "engine": engine, "ids": ids})


def cmd_warmup(args, store: ProfileStore) -> int:
    prof = _resolve_or_fail(args, store)
    if not prof:
        return emit({"ok": False, "error": f"no profile matching '{args.ref}'"}, 1)
    from .warmup import warm_up
    where = f" ({args.preset})" if args.preset and not args.site else ""
    say(_c(f"Warming up '{prof.name}'{where} for {args.minutes} minute(s)...", C.CYN))
    res = warm_up(prof, store, minutes=args.minutes, sites=args.site or None,
                  engine=args.engine, headless=args.headless, preset=args.preset,
                  on_event=say)
    say(_c(f"Done — visited {res['pages']} page(s)"
           + (f", {res['errors']} unreachable" if res["errors"] else "") + ".", C.GRN))
    return emit({"ok": True, "id": prof.id, "name": prof.name, **res})


def cmd_schedule(args, store: ProfileStore) -> int:
    import time as _time

    from .scheduler import ScheduleStore, due_schedules, next_run_at
    from .warmup import warm_up

    sched = ScheduleStore(store.root)
    sub = args.schedule_command

    if sub == "list":
        rows = sched.list()
        if not rows:
            say(_c("No warm-up schedules.", C.DIM))
            return emit({"schedules": []})
        by_id = {p.id: p.name for p in store.list()}
        out = []
        for s in rows:
            name = by_id.get(s["profileId"], s["profileId"])
            state = _c("on", C.GRN) if s.get("enabled", True) else _c("off", C.DIM)
            nxt = next_run_at(s)
            when = "now" if nxt <= _time.time() else _time.strftime("%Y-%m-%d %H:%M", _time.localtime(nxt))
            say(f"  {s['id']}  {name:<20} every {s['everyHours']}h  "
                f"{s['minutes']}min  {s['preset']:<10} [{state}]  next: {when}")
            out.append({**s, "profileName": name, "nextRun": nxt,
                        "due": nxt <= _time.time()})
        return emit({"schedules": out})

    if sub == "add":
        prof = _resolve_or_fail(args, store)
        if not prof:
            return emit({"ok": False, "error": f"no profile matching '{args.ref}'"}, 1)
        entry = sched.add(profile_id=prof.id, every_hours=args.every,
                          minutes=args.minutes, preset=args.preset or "general",
                          now=_time.time())
        say(_c(f"Scheduled '{prof.name}' — warm up every {entry['everyHours']}h "
               f"for {entry['minutes']}min ({entry['preset']}).", C.GRN))
        say(_c(f"  id {entry['id']}", C.DIM))
        return emit(entry)

    if sub == "update":
        patch: dict = {}
        if args.every is not None:
            patch["everyHours"] = args.every
        if args.minutes is not None:
            patch["minutes"] = args.minutes
        if args.preset is not None:
            patch["preset"] = args.preset
        if args.enabled is not None:
            patch["enabled"] = args.enabled == "on"
        if not patch:
            say(_c("Nothing to change. Try --every, --minutes, --preset or --enabled.", C.RED))
            return emit({"ok": False, "error": "no changes given"}, 1)
        entry = sched.update(args.id, patch)
        if not entry:
            say(_c("No schedule with that id.", C.RED))
            return emit({"ok": False, "error": f"no schedule with id '{args.id}'"}, 1)
        say(_c(f"Updated schedule {entry['id']}: every {entry['everyHours']}h, "
               f"{entry['minutes']}min, {entry['preset']}, "
               f"{'on' if entry.get('enabled', True) else 'off'}.", C.GRN))
        return emit(entry)

    if sub == "remove":
        ok = sched.delete(args.id)
        say(_c("Removed.", C.GRN) if ok else _c("No schedule with that id.", C.RED))
        return emit({"ok": ok, "id": args.id}, 0 if ok else 1)

    if sub == "run-due":
        now = _time.time()
        due = due_schedules(sched.list(), now)
        if not due:
            say(_c("Nothing due.", C.DIM))
            return emit({"due": 0, "ran": 0, "skipped": []})
        by_id = {p.id: p for p in store.list()}
        ran, skipped = [], []
        for s in due:
            prof = by_id.get(s["profileId"])
            if not prof:
                say(_c(f"  skip {s['id']} — profile {s['profileId']} not in this store", C.YEL))
                skipped.append(s["id"])
                continue
            say(_c(f"  warming '{prof.name}' ({s['preset']}, {s['minutes']}min)...", C.CYN))
            warm_up(prof, store, minutes=s["minutes"], preset=s["preset"], headless=True)
            sched.mark_run(s["id"], _time.time())
            ran.append(s["id"])
        say(_c(f"Ran {len(ran)} due schedule(s).", C.GRN))
        return emit({"due": len(due), "ran": len(ran), "ranIds": ran, "skipped": skipped})

    return emit({"ok": False, "error": f"unknown schedule command '{sub}'"}, 1)


def cmd_pool(args, store: ProfileStore) -> int:
    """The shared proxy pool — the same ``<data-dir>/proxies.json`` the dashboard's
    Proxies page reads and writes, so an import from either side is visible to both."""
    from . import proxypool as pp
    from .server import ProxyPool          # a plain JSON store; needs no FastAPI

    pool = ProxyPool(store.root)
    sub = args.pool_command

    if sub == "list":
        items = pool.list()
        if not items:
            say(_c("The proxy pool is empty. Add some with:  numera pool add <file>", C.DIM))
            return emit({"total": 0, "proxies": []})
        say(_c(f"{'ID':<14}{'TYPE':<9}{'HOST':<26}{'CC':<5}STATUS", C.B))
        for p in items:
            say(f"{p['id']:<14}{p.get('type', 'HTTP'):<9}"
                f"{p['host'] + ':' + str(p['port']):<26}{(p.get('country') or '-'):<5}"
                f"{p.get('status', 'untested')}")
        say(_c(f"\n{len(items)} proxy/proxies.", C.DIM))
        return emit({"total": len(items), "proxies": items})

    if sub == "add":
        text = sys.stdin.read() if args.source == "-" else Path(args.source).read_text(encoding="utf-8")
        parsed = pp.parse_many(text)
        if not parsed:
            say(_c("No proxies could be parsed from that input.", C.RED))
            return emit({"ok": False, "error": "no proxies could be parsed from that input"}, 1)
        added = pool.add_many(parsed)
        say(_c(f"Added {len(added)} proxy/proxies", C.GRN)
            + _c(f" ({len(parsed) - len(added)} already in the pool)", C.DIM))
        return emit({"added": len(added), "parsed": len(parsed),
                     "total": len(pool.list()), "proxies": added})

    if sub == "remove":
        ok = pool.delete(args.id)
        say(_c("Removed.", C.GRN) if ok else _c("No proxy with that id.", C.RED))
        return emit({"ok": ok, "id": args.id}, 0 if ok else 1)

    if sub == "test":
        targets = pool.list() if args.all else [p for p in [pool.get(args.id)] if p]
        if not targets:
            say(_c("No proxy with that id." if not args.all else "The proxy pool is empty.", C.RED))
            return emit({"ok": False, "error": "nothing to test"}, 1)
        import time as _time
        results, bad = [], 0
        for pr in targets:
            res = pp.check(pr)
            status = "ok" if res.get("ok") else ("skipped" if res.get("ok") is None else "failed")
            pool.update(pr["id"], {"status": status, "lastCheck": res, "checkedAt": _time.time()})
            colour = C.GRN if status == "ok" else C.DIM if status == "skipped" else C.RED
            detail = res.get("ip") or res.get("error") or ""
            say(f"  {pr['host']}:{pr['port']:<8}{_c(status, colour)}  {_c(str(detail), C.DIM)}")
            results.append({**res, "id": pr["id"], "status": status})
            bad += status == "failed"
        return emit({"tested": len(results), "failed": bad, "results": results},
                    0 if not bad else 1)

    # assign: round-robin the pool across profiles, aligning each one's
    # locale/timezone to its proxy's country — the same rule as POST /api/proxies/assign,
    # applied to the profiles the CLI owns.
    from . import devices
    from .bulk import select

    proxies = [pool.get(i) for i in args.proxy_id] if args.proxy_id else pool.list()
    proxies = [p for p in proxies if p]
    if not proxies:
        say(_c("The proxy pool is empty.", C.RED))
        return emit({"ok": False, "error": "the proxy pool is empty"}, 1)
    targets = select(store.list(), refs=args.ref, tag=args.tag, everything=args.all)
    if not targets:
        say(_c("Nothing matched. Pass profile names/ids, --tag TAG, or --all.", C.RED))
        return emit({"ok": False, "error": "no profiles matched"}, 1)

    scheme_of = {"HTTP": "http", "HTTPS": "http", "SOCKS5": "socks5", "SOCKS4": "socks4"}
    assigned = []
    for i, prof in enumerate(targets):
        pr = proxies[i % len(proxies)]
        scheme = scheme_of.get(str(pr.get("type", "HTTP")).upper(), "http")
        cc = (pr.get("country") or "").upper()
        prof.proxy = Proxy(server=f"{scheme}://{pr['host']}:{pr['port']}",
                           username=pr.get("user") or None,
                           password=pr.get("pass") or None,
                           country=cc or None)
        loc = devices.COUNTRY_TO_LOCALE.get(cc)
        if loc:
            prof.fingerprint.locale = loc
            prof.fingerprint.timezone, prof.fingerprint.languages = devices.LOCALES[loc]
        store.save(prof)
        say(f"  {prof.name:<24}{_c(prof.proxy.server, C.DIM)}"
            + (_c(f"  -> {loc}", C.DIM) if loc else ""))
        assigned.append({"id": prof.id, "name": prof.name,
                         "proxy": prof.proxy.server, "locale": prof.fingerprint.locale})
    say(_c(f"Assigned {len(proxies)} proxy/proxies across {len(assigned)} profile(s).", C.GRN))
    return emit({"assigned": len(assigned), "pool": len(proxies), "profiles": assigned})


def cmd_health(args, store: ProfileStore) -> int:
    """One row per profile: coherence, proxy, collisions, schedule and live state.

    The dashboard's Health page also shows a *cached* trust grade, written by the
    API when a trust/align run finishes. Nothing caches one on the engine side,
    so this reports ``trust: null`` — run `numera trust <ref>` for a live grade.
    """
    from .collision import find_collisions
    from .scheduler import ScheduleStore, next_run_at

    profiles = store.list()
    colliding: set[str] = set()
    for g in find_collisions(profiles):
        colliding.update(m["id"] for m in g["members"])

    by_profile: dict[str, dict] = {}
    for s in ScheduleStore(store.root).list():
        cur = by_profile.get(s["profileId"])
        if cur is None or s.get("everyHours", 1e9) < cur.get("everyHours", 1e9):
            by_profile[s["profileId"]] = s
    running = {j.get("profile") for j in jobs.listing(store.root) if j["running"]}

    rows, summary = [], {"total": len(profiles), "running": 0, "coherent": 0,
                         "withProxy": 0, "scheduled": 0, "colliding": 0,
                         "needsAttention": 0}
    for p in profiles:
        issues = validate(p.fingerprint)
        sc = by_profile.get(p.id)
        row = {
            **_row(p),
            "coherent": not issues,
            "issues": issues,
            "collides": p.id in colliding,
            "hasProxy": p.proxy is not None,
            "status": "running" if p.id in running else "stopped",
            "trust": None,
            "schedule": ({"id": sc["id"], "everyHours": sc["everyHours"],
                          "preset": sc["preset"], "enabled": sc.get("enabled", True),
                          "nextRun": next_run_at(sc)} if sc else None),
        }
        row["needsAttention"] = bool(issues) or row["collides"]
        summary["running"] += row["status"] == "running"
        summary["coherent"] += row["coherent"]
        summary["withProxy"] += row["hasProxy"]
        summary["scheduled"] += sc is not None
        summary["colliding"] += row["collides"]
        summary["needsAttention"] += row["needsAttention"]
        rows.append(row)

    if not rows:
        say(_c("No profiles yet.", C.DIM))
        return emit({"summary": summary, "profiles": []})
    say(_c(f"{'NAME':<22}{'STATUS':<10}{'COHERENT':<10}{'PROXY':<8}{'SCHED':<8}NOTES", C.B))
    for r in rows:
        flag = _c("yes", C.GRN) if r["coherent"] else _c("no", C.RED)
        notes = "; ".join(r["issues"]) or ("shares a fingerprint" if r["collides"] else "")
        say(f"{r['name']:<22}{r['status']:<10}{flag:<19}"
            f"{('yes' if r['hasProxy'] else '-'):<8}"
            f"{('yes' if r['schedule'] else '-'):<8}{_c(notes, C.DIM)}")
    say(_c(f"\n{summary['needsAttention']} of {summary['total']} need attention.", C.DIM))
    return emit({"summary": summary, "profiles": rows})


def cmd_attach(args, store: ProfileStore) -> int:
    prof = _resolve_or_fail(args, store)
    if not prof:
        return emit({"ok": False, "error": f"no profile matching '{args.ref}'"}, 1)
    from .automation import attach, snippets
    say(_c(f"Launching '{prof.name}' with DevTools on port {args.port}...", C.CYN))
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


def _cmd_ext_default(args, store: ProfileStore) -> int:
    """The global extension set: what every *new* profile starts with.

    Chromium has no browser-wide extension store to hook — each profile is its
    own browser instance — so "global" means this list is copied onto profiles
    as they are created. Use `numera apply --extension` to push it onto the
    profiles that already exist.
    """
    current = store.get_default_extensions()
    if args.ext_command == "default-list":
        if not current:
            say(_c("No default extensions. New profiles start with none.", C.DIM))
            return emit({"extensions": []})
        say(_c("Default extensions for new profiles:", C.B))
        for path in current:
            missing = "" if Path(path).expanduser().is_dir() else _c("  (folder not found)", C.RED)
            say(f"  {path}{missing}")
        return emit({"extensions": current})

    path = str(Path(args.path).expanduser())
    if args.ext_command == "default-add":
        if not Path(path).is_dir():
            say(_c(f"'{path}' is not a folder. Point at an *unpacked* extension "
                   f"(the folder containing manifest.json).", C.RED))
            return emit({"ok": False, "error": f"'{path}' is not a folder"}, 1)
        if path in current:
            say(_c("Already in the default set.", C.DIM))
            return emit({"ok": True, "extensions": current, "changed": False})
        current.append(path)
    else:  # default-remove
        if path not in current:
            say(_c(f"'{path}' isn't in the default set.", C.RED))
            return emit({"ok": False, "error": f"'{path}' isn't in the default set"}, 1)
        current.remove(path)
    store.set_default_extensions(current)
    say(_c(f"Default set now has {len(current)} extension(s). "
           f"New profiles will load them.", C.GRN))
    return emit({"ok": True, "extensions": current, "changed": True})


def cmd_ext(args, store: ProfileStore) -> int:
    if args.ext_command.startswith("default"):
        return _cmd_ext_default(args, store)
    prof = _resolve_or_fail(args, store)
    if not prof:
        return emit({"ok": False, "error": f"no profile matching '{args.ref}'"}, 1)
    if args.ext_command == "list":
        if not prof.extensions:
            say(_c(f"'{prof.name}' loads no extensions.", C.DIM))
            return emit({"id": prof.id, "extensions": []})
        say(_c(f"Extensions for '{prof.name}':", C.B))
        for path in prof.extensions:
            exists = Path(path).expanduser().is_dir()
            note = "" if exists else _c("  (folder not found)", C.RED)
            say(f"  {path}{note}")
        return emit({"id": prof.id, "extensions": list(prof.extensions)})

    path = str(Path(args.path).expanduser())
    if args.ext_command == "add":
        if not Path(path).is_dir():
            say(_c(f"'{path}' is not a folder. Point at an *unpacked* extension "
                   f"(the folder containing manifest.json).", C.RED))
            return emit({"ok": False, "error": f"'{path}' is not a folder"}, 1)
        if path in prof.extensions:
            say(_c("Already loaded.", C.DIM))
            return emit({"ok": True, "id": prof.id, "extensions": list(prof.extensions),
                         "changed": False})
        prof.extensions.append(path)
    else:  # remove
        if path not in prof.extensions:
            say(_c(f"'{path}' isn't on this profile.", C.RED))
            return emit({"ok": False, "error": f"'{path}' isn't on this profile"}, 1)
        prof.extensions.remove(path)
    store.save(prof)
    say(_c(f"{'Added' if args.ext_command == 'add' else 'Removed'} — "
           f"'{prof.name}' now loads {len(prof.extensions)} extension(s).", C.GRN))
    return emit({"ok": True, "id": prof.id, "extensions": list(prof.extensions),
                 "changed": True})


def cmd_apply(args, store: ProfileStore) -> int:
    from .bulk import apply_patch, select
    targets = select(store.list(), refs=args.ref, tag=args.tag, everything=args.all)
    if not targets:
        say(_c("Nothing matched. Pass profile names/ids, --tag TAG, or --all.", C.RED))
        return emit({"ok": False, "error": "no profiles matched"}, 1)

    patch: dict = {}
    for key in ("engine", "notes", "locale", "timezone"):
        if getattr(args, key) is not None:
            patch[key] = getattr(args, key)
    if args.startup_url:
        patch["startup_urls"] = args.startup_url
    if args.extension is not None:
        # An empty --extension "" clears the list; otherwise it replaces it.
        patch["extensions"] = [str(Path(e).expanduser()) for e in args.extension if e]
    if args.add_tag:
        patch["add_tags"] = args.add_tag
    if args.remove_tag:
        patch["remove_tags"] = args.remove_tag
    if args.regen:
        patch["regen"] = True
    if args.proxy is not None:
        patch["proxy"] = _parse_proxy(args.proxy, args.country) if args.proxy else None
    if not patch:
        say(_c("No changes given. Try --engine, --proxy, --locale, --add-tag, --regen…", C.RED))
        return emit({"ok": False, "error": "no changes given"}, 1)

    if not args.yes:
        say(_c(f"About to change {len(targets)} profile(s):", C.B))
        for p in targets[:10]:
            say(f"  {p.name}")
        if len(targets) > 10:
            say(_c(f"  … and {len(targets) - 10} more", C.DIM))
        if not _confirm("Continue? (y/N) "):
            say("Aborted.")
            return emit({"ok": False, "aborted": True, "matched": len(targets)})

    touched = {}
    for prof in targets:
        changed = apply_patch(prof, patch)
        if changed:
            store.save(prof)
            touched[prof.id] = changed
            say(f"  {prof.name:<24}{_c(', '.join(changed), C.DIM)}")
    if patch.get("engine"):
        store.set_default_engine(patch["engine"])   # "last engine wins"
    say(_c(f"Updated {len(touched)} of {len(targets)} profile(s).", C.GRN))
    return emit({"ok": True, "matched": len(targets), "updated": len(touched),
                 "changes": touched})


def _prompt_password(confirm: bool = False) -> str:
    import getpass
    if _JSON:
        raise RuntimeError("this needs a master password — pass --password "
                           "or set NUMERA_PASSWORD (a --json run never prompts)")
    pw = getpass.getpass("Master password: ")
    if confirm and getpass.getpass("Confirm master password: ") != pw:
        raise RuntimeError("Passwords didn't match.")
    return pw


def cmd_vault(args, store: ProfileStore) -> int:
    from . import crypto
    if args.vault_command == "status":
        if not store.vault_enabled():
            state = _c("off", C.DIM) + " — proxy passwords are stored in plaintext"
        elif store.unlocked():
            state = _c("unlocked", C.GRN)
        else:
            state = _c("locked", C.YEL) + " — set NUMERA_PASSWORD or pass --password"
        say(_c("Vault: ", C.B) + state)
        if not crypto.available():
            say(_c('  (install the backend with: pip install -e ".[secure]")', C.DIM))
        return emit({"enabled": store.vault_enabled(),
                     "unlocked": store.unlocked() if store.vault_enabled() else None,
                     "backend": crypto.available()})

    if not crypto.available():
        say(_c('Encrypted storage needs the cryptography package:  pip install -e ".[secure]"', C.RED))
        return emit({"ok": False, "error": "the cryptography package is not installed"}, 1)

    if args.vault_command == "enable":
        pw = store._password or _prompt_password(confirm=True)
        store._password = pw
        n = store.enable_vault(pw)
        say(_c(f"Vault enabled — encrypted secrets in {n} profile(s).", C.GRN))
        say(_c("Keep this password safe: without it, encrypted proxy passwords can't be recovered.", C.YEL))
        say(_c("Set NUMERA_PASSWORD (or pass --password) when launching so profiles can use their proxy.", C.DIM))
        return emit({"ok": True, "enabled": True, "encrypted": n})

    return emit({"ok": False, "error": f"unknown vault command '{args.vault_command}'"}, 1)


def cmd_import_from(args, store: ProfileStore) -> int:
    from . import importers
    try:
        results = importers.import_file(args.file, store)
    except (FileNotFoundError, ValueError) as e:
        say(_c(f"Couldn't read {args.file}: {e}", C.RED))
        return emit({"ok": False, "error": f"couldn't read {args.file}: {e}"}, 1)
    if not results:
        say(_c("No profiles found in that file.", C.RED))
        return emit({"ok": False, "error": "no profiles found in that file"}, 1)
    clean = sum(1 for _, issues in results if not issues)
    say(_c(f"Imported {len(results)} profile(s)  ", C.GRN)
        + _c(f"({clean} coherent out of the box)", C.DIM))
    for prof, issues in results:
        if issues:
            say(f"  {prof.name:<24}{_c('review: ' + '; '.join(issues), C.YEL)}")
    say(_c("Run `numera check <name>` on any flagged profile to see details.", C.DIM))
    return emit({"ok": True, "imported": len(results), "coherent": clean,
                 "results": [{"id": p.id, "name": p.name, "issues": iss}
                             for p, iss in results]})


def cmd_serve(args, store: ProfileStore) -> int:
    from .server import serve
    serve(host=args.host, port=args.port, data_dir=args.data_dir,
          verbose=getattr(args, "verbose", False))
    return 0


def cmd_mcp(args, store: ProfileStore) -> int:
    """Speak MCP on stdin/stdout. Every command below is a tool; see mcp.py."""
    from . import mcp
    return mcp.serve(data_dir=str(store.root))


def cmd_check(args, store: ProfileStore) -> int:
    prof = store.resolve(args.ref)
    if not prof:
        return _no_profile(args.ref)
    problems = validate(prof.fingerprint)
    if not problems:
        say(_c(f"'{prof.name}' fingerprint is coherent.", C.GRN))
        return emit({"ok": True, "id": prof.id, "name": prof.name, "issues": []})
    say(_c(f"'{prof.name}' has {len(problems)} issue(s):", C.RED))
    for p in problems:
        say(f"  - {p}")
    return emit({"ok": False, "id": prof.id, "name": prof.name, "issues": problems}, 1)


# ---- rendering -----------------------------------------------------------
def _print_summary(prof: Profile, full: bool = False) -> None:
    fp = prof.fingerprint
    rows = [
        ("id", prof.id),
        ("engine", prof.engine),
        ("os", fp.os + (" (mobile)" if fp.is_mobile else "")),
        ("chrome", fp.chrome_version),
        ("screen", f"{fp.screen_width}x{fp.screen_height}"),
        ("cpu / ram", f"{fp.hardware_concurrency} cores / {fp.device_memory} GB"),
        ("gpu", fp.webgl_renderer),
        ("locale / tz", f"{fp.locale}  {fp.timezone}"),
    ]
    if full:
        rows.insert(2, ("user-agent", fp.user_agent))
        rows.append(("languages", ", ".join(fp.languages)))
        rows.append(("proxy", prof.proxy.server if prof.proxy else "-"))
        rows.append(("tags", ", ".join(prof.tags) or "-"))
        rows.append(("notes", prof.notes or "-"))
        rows.append(("seed", str(fp.seed)))
    for k, v in rows:
        say(f"  {_c(k + ':', C.DIM):<24}{v}")


# ---- parser --------------------------------------------------------------
# The two flags every command carries, defined once and inherited. They live on
# the leaf parsers so their position never matters: `numera trust x --json` is
# what server.py already runs, and `numera launch x --detach` reads the same way.
_AGENT = argparse.ArgumentParser(add_help=False)
_AGENT.add_argument("--json", action="store_true",
                    help="Print the result as JSON on stdout (and nothing else)")
_AGENT.add_argument("--detach", action="store_true",
                    help="Run in the background and print a job handle instead of blocking")


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        prog="numera",
        description="Browser fingerprint & profile manager.",
    )
    p.add_argument("--version", action="version", version=f"numera {__version__}")
    p.add_argument("--data-dir", default=None,
                   help="Override the data directory (default: ~/.numera)")
    p.add_argument("--password", default=None,
                   help="Master password for the secrets vault "
                        "(or set the NUMERA_PASSWORD env var)")
    sub = p.add_subparsers(dest="command", required=True)

    def add(name, **kw):
        return sub.add_parser(name, parents=[_AGENT], **kw)

    c = add("create", help="Create a new profile")
    c.add_argument("name")
    c.add_argument("--os", choices=["windows", "macos", "linux", "android"])
    c.add_argument("--locale")
    c.add_argument("--proxy", help="scheme://[user:pass@]host:port")
    c.add_argument("--country", help="ISO code to align locale/timezone (e.g. US, PK)")
    c.add_argument("--seed", type=int, help="Fixed seed for a reproducible fingerprint")
    c.add_argument("--mobile", action="store_true", help="Generate a mobile device")
    c.add_argument("--engine", help="Launch backend: cloak | camoufox | patchright | playwright "
                                    "(default: saved default engine)")
    c.add_argument("--notes", default="")
    c.add_argument("--tag", action="append", help="Add a tag (repeatable)")
    c.set_defaults(func=cmd_create)

    c = add("list", help="List profiles")
    c.add_argument("--tag", help="Filter by tag")
    c.set_defaults(func=cmd_list)

    c = add("show", help="Show full profile details")
    c.add_argument("ref", help="Profile name or id")
    c.set_defaults(func=cmd_show)

    c = add("launch", help="Launch a profile in a browser")
    c.add_argument("ref")
    c.add_argument("--headless", action="store_true")
    c.add_argument("--engine", help="Override the profile's launch engine for this run")
    c.set_defaults(func=cmd_launch)

    c = add("stop", help="End the sessions a profile has running")
    c.add_argument("ref")
    c.set_defaults(func=cmd_stop)

    c = add("jobs", help="Background work started with --detach")
    jb = c.add_subparsers(dest="jobs_command", required=True)
    s = jb.add_parser("list", parents=[_AGENT], help="Every job and its state")
    s.set_defaults(func=cmd_jobs)
    s = jb.add_parser("show", parents=[_AGENT], help="One job, with the tail of its output")
    s.add_argument("id")
    s.add_argument("--lines", type=int, default=20, help="How much output to show")
    s.set_defaults(func=cmd_jobs)
    s = jb.add_parser("stop", parents=[_AGENT], help="Terminate a job and anything it started")
    s.add_argument("id")
    s.set_defaults(func=cmd_jobs)
    s = jb.add_parser("clear", parents=[_AGENT], help="Forget the jobs that have finished")
    s.set_defaults(func=cmd_jobs)

    c = add("engines", help="List available launch engines")
    c.set_defaults(func=cmd_engines)

    c = add("default-engine",
            help="Show or set the engine used for new profiles")
    c.add_argument("name", nargs="?",
                   help="Engine to make default (omit to show the current one)")
    c.set_defaults(func=cmd_default_engine)

    c = add("config", help="Show or change settings (the dashboard's /api/config)")
    c.add_argument("--default-engine", help="Engine new profiles are created with")
    c.add_argument("--headless-launch", choices=["on", "off"],
                   help="Whether the API launches profiles without a window")
    c.add_argument("--default-extension", action="append",
                   help="Unpacked extension folder for new profiles "
                        "(repeatable, replaces the set; pass \"\" to clear)")
    c.set_defaults(func=cmd_config)

    c = add("edit", help="Edit notes / tags / proxy")
    c.add_argument("ref")
    c.add_argument("--notes")
    c.add_argument("--add-tag", action="append")
    c.add_argument("--proxy")
    c.add_argument("--country")
    c.set_defaults(func=cmd_edit)

    c = add("regen", help="Regenerate the fingerprint")
    c.add_argument("ref")
    c.add_argument("--os", choices=["windows", "macos", "linux", "android"])
    c.add_argument("--locale")
    c.set_defaults(func=cmd_regen)

    c = add("delete", help="Delete a profile")
    c.add_argument("ref")
    c.add_argument("-y", "--yes", action="store_true", help="Skip confirmation")
    c.set_defaults(func=cmd_delete)

    c = add("export", help="Export a profile to JSON")
    c.add_argument("ref"); c.add_argument("file")
    c.set_defaults(func=cmd_export)

    c = add("import", help="Import a profile from JSON")
    c.add_argument("file")
    c.set_defaults(func=cmd_import)

    c = add("backup", help="Zip a profile + its whole session to a portable file")
    c.add_argument("ref"); c.add_argument("file", help="Destination .zip")
    c.set_defaults(func=cmd_backup)

    c = add("restore", help="Recreate a profile + session from a backup zip")
    c.add_argument("file", help="A backup made with `numera backup`")
    c.add_argument("--new-id", action="store_true",
                   help="Import as a fresh copy instead of overwriting the original id")
    c.set_defaults(func=cmd_restore)

    c = add("collisions",
            help="Find profiles that share a fingerprint (a linkage risk)")
    c.set_defaults(func=cmd_collisions)

    c = add("health", help="One row per profile: coherence, proxy, collisions, schedule")
    c.set_defaults(func=cmd_health)

    c = add("import-from",
            help="Import profiles exported from GoLogin / AdsPower / Multilogin")
    c.add_argument("file", help="Their exported JSON (a single profile or a list)")
    c.set_defaults(func=cmd_import_from)

    c = add("cookies", help="List, export or import a profile's cookies")
    ck = c.add_subparsers(dest="cookies_command", required=True)

    s = ck.add_parser("list", parents=[_AGENT], help="Show what's in the profile's cookie jar")
    s.add_argument("ref")
    s.add_argument("--engine", help="Open with a different engine than the profile's")
    s.set_defaults(func=cmd_cookies_list)

    s = ck.add_parser("export", parents=[_AGENT], help="Write the profile's cookies to a file")
    s.add_argument("ref"); s.add_argument("file")
    s.add_argument("--format", choices=["json", "netscape"],
                   help="Output format (default: netscape for .txt, else json)")
    s.add_argument("--engine", help="Open with a different engine than the profile's")
    s.set_defaults(func=cmd_cookies_export)

    s = ck.add_parser("import", parents=[_AGENT], help="Load cookies from a file into the profile")
    s.add_argument("ref"); s.add_argument("file")
    s.add_argument("--clear", action="store_true",
                   help="Empty the existing jar first (replace instead of merge)")
    s.add_argument("--engine", help="Open with a different engine than the profile's")
    s.set_defaults(func=cmd_cookies_import)

    c = add("check", help="Validate fingerprint coherence")
    c.add_argument("ref")
    c.set_defaults(func=cmd_check)

    c = add("proxy", help="Proxy tools")
    px = c.add_subparsers(dest="proxy_command", required=True)
    s = px.add_parser("test", parents=[_AGENT],
                      help="Check the exit IP, country and WebRTC leaks")
    s.add_argument("ref")
    s.add_argument("--engine", help="Test with a different engine than the profile's")
    s.set_defaults(func=cmd_proxy_test)

    c = add("pool", help="The shared proxy pool: import, test and assign proxies")
    pl = c.add_subparsers(dest="pool_command", required=True)
    s = pl.add_parser("list", parents=[_AGENT], help="Every proxy in the pool")
    s.set_defaults(func=cmd_pool)
    s = pl.add_parser("add", parents=[_AGENT], help="Import a proxy list")
    s.add_argument("source", help="A file of proxy lines, or - to read stdin")
    s.set_defaults(func=cmd_pool)
    s = pl.add_parser("remove", parents=[_AGENT], help="Drop one proxy from the pool")
    s.add_argument("id")
    s.set_defaults(func=cmd_pool)
    s = pl.add_parser("test", parents=[_AGENT], help="Route a real request through a proxy")
    s.add_argument("id", nargs="?", help="Proxy id (omit with --all)")
    s.add_argument("--all", action="store_true", help="Test the whole pool")
    s.set_defaults(func=cmd_pool)
    s = pl.add_parser("assign", parents=[_AGENT],
                      help="Spread the pool across profiles, aligning locale to country")
    s.add_argument("ref", nargs="*", help="Profile names or ids")
    s.add_argument("--tag", help="Every profile carrying this tag")
    s.add_argument("--all", action="store_true", help="Every profile")
    s.add_argument("--proxy-id", action="append",
                   help="Use only these pool entries (repeatable; default: the whole pool)")
    s.set_defaults(func=cmd_pool)

    c = add("tls", help="Show the profile's TLS/JA3 handshake fingerprint")
    c.add_argument("ref")
    c.add_argument("--engine", help="Read with a different engine than the profile's")
    c.set_defaults(func=cmd_tls)

    c = add("dns", help="Check whether DNS lookups leak past the proxy")
    c.add_argument("ref")
    c.add_argument("--engine", help="Test with a different engine than the profile's")
    c.set_defaults(func=cmd_dns)

    c = add("trust", help="Score the profile the way a fingerprinter would")
    c.add_argument("ref")
    c.add_argument("--engine", help="Inspect with a different engine than the profile's")
    c.set_defaults(func=cmd_trust)

    c = add("align",
            help="Auto-adjust: match the profile to what its browser really shows")
    c.add_argument("ref")
    c.add_argument("--engine", help="Align against a different engine than the profile's")
    c.set_defaults(func=cmd_align)

    c = add("bulk-create", help="Generate many coherent profiles at once")
    c.add_argument("--count", type=int, required=True, help="How many to create")
    c.add_argument("--os", action="append", choices=["windows", "macos", "linux", "android"],
                   help="OS to spread the batch across (repeatable; default windows)")
    c.add_argument("--locale", action="append",
                   help="Locale to spread across (repeatable; e.g. --locale en-US --locale de-DE)")
    c.add_argument("--chrome", action="append",
                   help="Chrome version to draw from (repeatable; e.g. 128.0.0.0)")
    c.add_argument("--engine", help="Launch backend for the batch")
    c.add_argument("--prefix", default="Profile", help="Name prefix (default: Profile)")
    c.add_argument("--start", type=int, default=1, help="First number in the sequence")
    c.add_argument("--pad", type=int, default=4, help="Zero-pad the number to this width")
    c.add_argument("--tag", action="append", help="Tag every profile (repeatable)")
    c.add_argument("--mobile", action="store_true", help="Generate mobile devices")
    c.add_argument("--seed", type=int, help="Fixed seed for a reproducible batch")
    c.set_defaults(func=cmd_bulk_create)

    c = add("warmup", help="Browse normally for a while to age the profile")
    c.add_argument("ref")
    c.add_argument("--minutes", type=float, default=5, help="How long to browse (default: 5)")
    c.add_argument("--site", action="append",
                   help="Site to visit (repeatable; replaces the preset/default list)")
    c.add_argument("--preset", choices=["general", "ads", "ecommerce", "crypto", "social", "news"],
                   help="Per-vertical warm-up set to browse (default: general)")
    c.add_argument("--headless", action="store_true")
    c.add_argument("--engine")
    c.set_defaults(func=cmd_warmup)

    c = add("schedule", help="Recurring unattended warm-up (add/list/update/remove/run-due)")
    sc = c.add_subparsers(dest="schedule_command", required=True)
    s = sc.add_parser("list", parents=[_AGENT],
                      help="Show all warm-up schedules and when they next run")
    s.set_defaults(func=cmd_schedule)
    s = sc.add_parser("add", parents=[_AGENT], help="Schedule a recurring warm-up for a profile")
    s.add_argument("ref")
    s.add_argument("--every", type=float, required=True, help="Interval in hours (e.g. 12)")
    s.add_argument("--minutes", type=float, default=5, help="How long each warm-up runs")
    s.add_argument("--preset", choices=["general", "ads", "ecommerce", "crypto", "social", "news"],
                   help="Per-vertical warm-up set (default: general)")
    s.set_defaults(func=cmd_schedule)
    s = sc.add_parser("update", parents=[_AGENT], help="Change an existing schedule")
    s.add_argument("id")
    s.add_argument("--every", type=float, help="New interval in hours")
    s.add_argument("--minutes", type=float, help="New warm-up length")
    s.add_argument("--preset", choices=["general", "ads", "ecommerce", "crypto", "social", "news"])
    s.add_argument("--enabled", choices=["on", "off"], help="Pause or resume it")
    s.set_defaults(func=cmd_schedule)
    s = sc.add_parser("remove", parents=[_AGENT], help="Delete a schedule by id")
    s.add_argument("id")
    s.set_defaults(func=cmd_schedule)
    s = sc.add_parser("run-due", parents=[_AGENT],
                      help="Run every schedule that's due now (for cron/Task Scheduler)")
    s.set_defaults(func=cmd_schedule)

    c = add("attach", help="Open the profile with DevTools for your own automation")
    c.add_argument("ref")
    c.add_argument("--port", type=int, default=9222, help="DevTools port (default: 9222)")
    c.add_argument("--engine")
    c.add_argument("--headless", action="store_true",
                   help="Attach without a visible window (for servers/CI)")
    c.set_defaults(func=cmd_attach)

    c = add("ext", help="Manage a profile's browser extensions")
    ex = c.add_subparsers(dest="ext_command", required=True)
    s = ex.add_parser("list", parents=[_AGENT], help="Show the extensions this profile loads")
    s.add_argument("ref")
    s.set_defaults(func=cmd_ext)
    for verb, helptext in (("add", "Load an unpacked extension folder"),
                           ("remove", "Stop loading an extension")):
        s = ex.add_parser(verb, parents=[_AGENT], help=helptext)
        s.add_argument("ref"); s.add_argument("path")
        s.set_defaults(func=cmd_ext)
    s = ex.add_parser("default-list", parents=[_AGENT],
                      help="Show the extensions every NEW profile starts with")
    s.set_defaults(func=cmd_ext)
    for verb, helptext in (("default-add", "Add to the set every NEW profile starts with"),
                           ("default-remove", "Drop one from that set")):
        s = ex.add_parser(verb, parents=[_AGENT], help=helptext)
        s.add_argument("path")
        s.set_defaults(func=cmd_ext)

    c = add("apply", help="Apply one change to many profiles at once")
    c.add_argument("ref", nargs="*", help="Profile names or ids")
    c.add_argument("--tag", help="Every profile carrying this tag")
    c.add_argument("--all", action="store_true", help="Every profile")
    c.add_argument("--engine", help="Switch them to this launch engine")
    c.add_argument("--proxy", help="scheme://[user:pass@]host:port  (empty string = go direct)")
    c.add_argument("--country", help="ISO code to align locale/timezone with the proxy")
    c.add_argument("--locale", help="Locale (also moves timezone + languages)")
    c.add_argument("--timezone")
    c.add_argument("--notes")
    c.add_argument("--startup-url", action="append", help="Startup URL (repeatable, replaces)")
    c.add_argument("--extension", action="append",
                   help="Unpacked extension folder (repeatable, replaces; pass \"\" to clear)")
    c.add_argument("--add-tag", action="append")
    c.add_argument("--remove-tag", action="append")
    c.add_argument("--regen", action="store_true",
                   help="Give each one a fresh fingerprint (same OS, locale and session)")
    c.add_argument("-y", "--yes", action="store_true", help="Skip confirmation")
    c.set_defaults(func=cmd_apply)

    c = add("vault", help="Encrypt proxy passwords at rest with a master password")
    vt = c.add_subparsers(dest="vault_command", required=True)
    vt.add_parser("status", parents=[_AGENT],
                  help="Show whether the vault is on, locked or unlocked").set_defaults(func=cmd_vault)
    vt.add_parser("enable", parents=[_AGENT],
                  help="Turn on encryption and encrypt existing secrets").set_defaults(func=cmd_vault)

    c = add("mcp", help="Speak MCP on stdin/stdout, exposing every command as a tool")
    c.set_defaults(func=cmd_mcp)

    c = add("serve", help="Run Numera (serves the dashboard and the API)")
    c.add_argument("--host", default="127.0.0.1")
    c.add_argument("--port", type=int, default=8787)
    c.add_argument("--verbose", action="store_true",
                   help="Debug-level detail in the run log (credentials stay redacted)")
    c.set_defaults(func=cmd_serve)

    return p


def main(argv: Optional[list[str]] = None) -> int:
    global _JSON
    argv = list(sys.argv[1:] if argv is None else argv)
    parser = build_parser()
    args = parser.parse_args(argv)
    _JSON = bool(getattr(args, "json", False))
    store = ProfileStore(args.data_dir, password=args.password)

    if getattr(args, "detach", False):
        rec = jobs.spawn(store.root, jobs.child_argv(argv, store.root),
                         command=args.command, profile=_detach_target(args, store))
        say(_c(f"Started job {rec['id']}", C.GRN)
            + _c(f"  (pid {rec['pid']}, log {rec['log']})", C.DIM))
        say(_c(f"  poll it with:  numera jobs show {rec['id']}", C.DIM))
        return emit(rec)

    code = 0
    try:
        code = args.func(args, store)
    except RuntimeError as e:
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
