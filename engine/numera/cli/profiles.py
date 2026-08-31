"""Commands that create, read, change and move profiles.

Nothing here opens a browser - that is browser.py. A command belongs in
this file when it only touches the store."""

from __future__ import annotations

import os
from pathlib import Path
from .. import jobs
from ..fingerprint import generate, validate
from ..models import Profile
from ..store import ProfileStore
from .out import C, _c, _confirm, _no_profile, _parse_proxy, _print_summary, _row, emit, say


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
    from .. import drivers
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
    from ..collision import find_collisions
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


def cmd_apply(args, store: ProfileStore) -> int:
    from ..bulk import apply_patch, select
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


def cmd_bulk_create(args, store: ProfileStore) -> int:
    from ..bulk import generate_profiles
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


def cmd_import_from(args, store: ProfileStore) -> int:
    from .. import importers
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


def cmd_health(args, store: ProfileStore) -> int:
    """One row per profile: coherence, proxy, collisions, schedule and live state.

    The dashboard's Health page also shows a *cached* trust grade, written by the
    API when a trust/align run finishes. Nothing caches one on the engine side,
    so this reports ``trust: null`` — run `numera trust <ref>` for a live grade.
    """
    from ..collision import find_collisions
    from ..scheduler import ScheduleStore, next_run_at

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
