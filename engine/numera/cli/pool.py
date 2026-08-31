"""The shared proxy pool: import, edit, health-check, export, assign.

The same `<data-dir>/proxies.json` the dashboard's Proxies page reads and
writes, so a change from either side is visible to both."""

from __future__ import annotations

import sys
import time
from pathlib import Path
from ..models import Proxy
from ..store import ProfileStore
from .out import C, _c, emit, say


def cmd_pool(args, store: ProfileStore) -> int:
    """The shared proxy pool — the same ``<data-dir>/proxies.json`` the dashboard's
    Proxies page reads and writes, so an import from either side is visible to both."""
    from .. import proxypool as pp
    from ..server import ProxyPool          # a plain JSON store; needs no FastAPI

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
        # Several ids at once: the dashboard's bulk delete needs a headless
        # equivalent, and removing 20 dead proxies one command at a time is not one.
        ids = args.id if isinstance(args.id, list) else [args.id]
        removed = [i for i in ids if pool.delete(i)]
        missing = [i for i in ids if i not in removed]
        if removed:
            say(_c(f"Removed {len(removed)} proxy/proxies.", C.GRN))
        for i in missing:
            say(_c(f"No proxy with id {i}.", C.RED))
        return emit({"removed": len(removed), "ids": removed, "missing": missing,
                     "total": len(pool.list())}, 0 if removed else 1)

    if sub == "rotate":
        ids = args.id if isinstance(args.id, list) else [args.id]
        targets = [p for p in (pool.get(i) for i in ids) if p]
        if not targets:
            say(_c("No proxy with that id.", C.RED))
            return emit({"ok": False, "error": "no proxy with that id"}, 1)
        results, moved = [], 0
        for pr in targets:
            # Serial: these hit one provider's control plane and firing several
            # at once is how a pool gets rate-limited.
            res = pp.rotate(pr)
            patch = {"rotatedAt": time.time()} if res.get("rotated") else {}
            if res.get("ok") is not None:
                patch.update({"status": "ok" if res.get("ok") else "failed",
                              "lastCheck": res, "checkedAt": time.time()})
            if patch:
                pool.update(pr["id"], patch)
            if res.get("rotated"):
                moved += 1
                say(_c(f"{pr['host']}:{pr['port']}  {res.get('previousIp') or '?'} "
                       f"-> {res.get('ip')}", C.GRN))
            else:
                say(_c(f"{pr['host']}:{pr['port']}  {res.get('error') or 'did not move'}", C.RED))
            results.append({"id": pr["id"], **res})
        return emit({"rotated": moved, "tried": len(results), "results": results},
                    0 if moved else 1)

    if sub == "archive":
        ids = args.id if isinstance(args.id, list) else [args.id]
        archived = not args.restore
        changed = [pool.update(i, {"archived": archived}) for i in ids if pool.get(i)]
        changed = [c for c in changed if c]
        word = "Archived" if archived else "Restored"
        say(_c(f"{word} {len(changed)} proxy/proxies.", C.GRN) if changed
            else _c("No proxy with that id.", C.RED))
        return emit({"archived" if archived else "restored": len(changed),
                     "ids": [c["id"] for c in changed]}, 0 if changed else 1)

    if sub == "edit":
        pr = pool.get(args.id)
        if not pr:
            say(_c("No proxy with that id.", C.RED))
            return emit({"ok": False, "error": "no proxy with that id"}, 1)
        patch = {k: v for k, v in (
            ("label", args.label), ("type", args.type), ("host", args.host),
            ("port", args.port), ("user", args.user), ("pass", args.password),
            ("country", args.country), ("rotateUrl", args.rotate_url)) if v is not None}
        if not patch:
            say(_c("Nothing to change. Pass --host, --port, --user, --pass, "
                   "--type, --country, --label or --rotate-url.", C.RED))
            return emit({"ok": False, "error": "nothing to change"}, 1)
        # A changed address means the stored verdict describes somewhere else.
        if {"host", "port", "user", "pass", "type"} & set(patch):
            patch.update({"status": "untested", "lastCheck": None, "checkedAt": None})
        out = pool.update(args.id, patch)
        say(_c(f"Updated {out['host']}:{out['port']}.", C.GRN))
        return emit({"ok": True, "proxy": out})

    if sub == "export":
        items = pool.list()
        if args.id:
            keep = set(args.id)
            items = [p for p in items if p["id"] in keep]
        lines = [pp.to_line(p) for p in items]
        text = "\n".join(lines) + ("\n" if lines else "")
        if args.file:
            Path(args.file).write_text(text, encoding="utf-8")
            say(_c(f"Wrote {len(lines)} proxy/proxies to {args.file}.", C.GRN))
        else:
            # No file: the list goes to stdout so it can be piped. Under --json
            # `emit` owns stdout, so the plain print is suppressed there.
            say(text.rstrip("\n"))
        return emit({"exported": len(lines), "file": args.file, "text": text})

    if sub == "test":
        ids = args.id if isinstance(args.id, list) else ([args.id] if args.id else [])
        targets = pool.list() if args.all else [p for p in (pool.get(i) for i in ids) if p]
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
    from .. import devices
    from ..bulk import select

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
