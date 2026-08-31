"""Commands that configure Numera rather than a profile: engines, settings,
extensions, folders, the secrets vault, schedules, and the two servers.

`serve` and `mcp` sit here because they are the same kind of thing - a
process you start rather than a task that finishes."""

from __future__ import annotations

import json
from pathlib import Path
from .. import __version__
from ..store import ProfileStore
from .out import C, _c, _resolve_or_fail, emit, json_mode, say


def cmd_engines(args, store: ProfileStore) -> int:
    from .. import drivers
    default = store.get_default_engine()
    say(_c("Launch engines:", C.B))
    for name, ok in drivers.available().items():
        mark = _c("installed", C.GRN) if ok else _c("not installed", C.DIM)
        star = _c("  <- default (new profiles)", C.CYN) if name == default else ""
        say(f"  {name:<14}{mark}{star}")
    return emit({"engines": drivers.available(), "default": default})


def cmd_default_engine(args, store: ProfileStore) -> int:
    from .. import drivers
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
        from .. import drivers
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


def cmd_vault(args, store: ProfileStore) -> int:
    from .. import crypto
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


def cmd_folder(args, store: ProfileStore) -> int:
    """Folders were GUI-only until now (B18). The rules live in numera.folders so
    this and the HTTP API cannot disagree about what a rename does to the
    profiles inside a folder."""
    from .. import folders

    root = store.root
    try:
        if args.folder_command == "list":
            names = folders.listing(root)
            for n in names:
                say(f"  {n}")
            if not names:
                say("No folders yet.")
            return emit({"folders": names})

        if args.folder_command == "create":
            names = folders.create(root, args.name)
            say(_c(f"Created folder '{args.name}'.", C.GRN))
            return emit({"folders": names, "created": args.name})

        if args.folder_command == "rename":
            names = folders.rename(root, args.old, args.new)
            say(_c(f"Renamed '{args.old}' to '{args.new}'; its profiles moved with it.", C.GRN))
            return emit({"folders": names, "from": args.old, "to": args.new})

        if args.folder_command == "delete":
            target = args.reassign_to or folders.UNFILED
            names = folders.delete(root, args.name, target)
            say(_c(f"Deleted '{args.name}'. Its profiles moved to '{target}'.", C.GRN))
            return emit({"folders": names, "deleted": args.name, "reassignedTo": target})
    except ValueError as e:
        say(_c(str(e), C.RED))
        return emit({"error": str(e)}, 1)

    return 1


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


def _cmd_ext_store(args, store: ProfileStore) -> int:
    """Web Store extensions: download, unpack, list, remove.

    Chromium refuses to install from the store under automation, so the crx is
    fetched from Google's update endpoint and unpacked into the data directory -
    where it travels with a portable install rather than pointing at one machine."""
    from .. import webstore

    if args.ext_command == "installed":
        items = webstore.installed(store.root)
        for e in items:
            say(f"  {e['id']}  {e['name']}")
        if not items:
            say(_c("No extensions installed from the Web Store yet.", C.DIM))
        return emit({"extensions": items})

    try:
        if args.ext_command == "uninstall":
            ok = webstore.uninstall(args.url, store.root)
            say(_c("Removed." if ok else "Not installed.", C.GRN if ok else C.DIM))
            return emit({"removed": ok})

        result = webstore.install(args.url, store.root, force=getattr(args, "force", False))
    except webstore.WebStoreError as e:
        say(_c(str(e), C.RED))
        return emit({"error": str(e)}, 1)

    verb = "Already installed" if result["reused"] else "Installed"
    say(_c(f"{verb}: {result['name']}  ({result['id']})", C.GRN))
    say(_c(f"  {result['path']}", C.DIM))

    if not getattr(args, "no_default", False):
        current = store.get_default_extensions()
        if result["path"] not in current:
            store.set_default_extensions([*current, result["path"]])
            say(_c("  Added to the set every new profile starts with.", C.DIM))
    return emit(result)


def cmd_ext(args, store: ProfileStore) -> int:
    if args.ext_command in ("install", "installed", "uninstall"):
        return _cmd_ext_store(args, store)
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


def _prompt_password(confirm: bool = False) -> str:
    import getpass
    if json_mode():
        raise RuntimeError("this needs a master password — pass --password "
                           "or set NUMERA_PASSWORD (a --json run never prompts)")
    pw = getpass.getpass("Master password: ")
    if confirm and getpass.getpass("Confirm master password: ") != pw:
        raise RuntimeError("Passwords didn't match.")
    return pw


def cmd_schedule(args, store: ProfileStore) -> int:
    import time as _time

    from ..scheduler import ScheduleStore, due_schedules, next_run_at
    from ..warmup import warm_up

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


def cmd_serve(args, store: ProfileStore) -> int:
    from ..server import serve
    serve(host=args.host, port=args.port, data_dir=args.data_dir,
          verbose=getattr(args, "verbose", False))
    return 0


def cmd_mcp(args, store: ProfileStore) -> int:
    """Speak MCP on stdin/stdout. Every command below is a tool; see mcp.py."""
    from .. import mcp
    return mcp.serve(data_dir=str(store.root))
