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
    proxy = _parse_proxy(args.proxy, args.country)
    fp = generate(seed=args.seed, os=args.os, locale=args.locale,
                  proxy=proxy, mobile=args.mobile)
    # No --engine given? Fall back to the saved default (CloakBrowser first).
    # If one IS given, remember it as the new default ("last engine wins").
    engine = args.engine or store.get_default_engine()
    from .. import drivers
    installed = engine not in drivers.names() or drivers.is_installed(engine)
    if not installed:
        say(_c(f"Note: engine '{engine}' isn't installed yet; install it before launching.", C.YEL))
    prof = Profile(name=args.name, fingerprint=fp, proxy=proxy,
                   notes=args.notes or "", tags=args.tag or [], engine=engine,
                   extensions=store.get_default_extensions())
    # Through the coordinator: it writes the dashboard twin in the same
    # transaction, and decides the name is free *inside* it - the old
    # `find_by_name()` beforehand was a check two processes could both pass.
    from ..repo import NameTaken, repo
    try:
        # The default engine rides along in the same transaction. It was
        # written first, so a create that then failed to save left the global
        # default changed by a command that reported failure.
        repo(store).create(prof, config_change=(
            {"default_engine": args.engine} if args.engine else None))
    except NameTaken as e:
        say(_c(f"A profile named '{args.name}' already exists.", C.RED))
        return emit({"ok": False, "error": str(e)}, 1)
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
    from ..repo import repo

    # `apply` runs inside the transaction and receives the profile as it is
    # *now*. Reading it out here, mutating the copy and saving it back is how a
    # concurrent edit got acknowledged and then erased.
    prof = repo(store).edit(args.ref, lambda prof: _apply_edit(args, prof))
    if not prof:
        return _no_profile(args.ref)
    say(_c(f"Updated '{prof.name}'.", C.GRN))
    return emit({"ok": True, "profile": prof.to_dict()})


def _apply_edit(args, prof):
    if args.notes is not None:
        prof.notes = args.notes
    if args.add_tag:
        for t in args.add_tag:
            if t not in prof.tags:
                prof.tags.append(t)
    if args.proxy is not None:
        prof.proxy = _parse_proxy(args.proxy, args.country) if args.proxy else None
    return prof


def cmd_regen(args, store: ProfileStore) -> int:
    from ..repo import repo

    def regenerate(prof):
        prof.fingerprint = generate(os=args.os, locale=args.locale, proxy=prof.proxy)
        return prof

    prof = repo(store).edit(args.ref, regenerate)
    if not prof:
        return _no_profile(args.ref)
    say(_c(f"Regenerated fingerprint for '{prof.name}'.", C.GRN))
    _print_summary(prof)
    return emit({"ok": True, "profile": prof.to_dict()})


def cmd_delete(args, store: ProfileStore) -> int:
    """Delete one or more profiles, with their sessions.

    Resolve everything first: confirming a list and then discovering halfway
    through that one name was a typo leaves a partial deletion nobody asked for.
    """
    refs = args.ref if isinstance(args.ref, list) else [args.ref]
    # Resolve once to build the confirmation set - and keep the *reference* the
    # user typed beside it. What is deleted later is the reference, not this
    # id: `delete alpha` that resolves A and then deletes `A` by id defeats the
    # coordinator's own protection, because by the time it runs, `alpha` may
    # name a different profile and A may be called something else. Asking again
    # by the original word is what lets `TargetMoved` fire.
    #
    # Deduplicated by id, so `delete alpha <alpha's id>` is one deletion rather
    # than one deletion and one confusing miss.
    targets, seen, missing = [], set(), []
    for ref in refs:
        try:
            prof = store.resolve(ref)
        except Exception as exc:                                # noqa: BLE001
            # An ambiguous legacy name, most likely. It names no single profile,
            # so there is nothing to confirm and nothing to delete.
            say(_c(str(exc), C.RED))
            return emit({"ok": False, "error": str(exc)}, 1)
        if prof is None:
            missing.append(ref)
        elif prof.id not in seen:
            seen.add(prof.id)
            targets.append((ref, prof))
    if missing:
        return _no_profile(", ".join(missing))
    if not targets:
        return _no_profile(", ".join(refs))

    if not args.yes:
        first = targets[0][1]
        what = (f"'{first.name}' [{first.id}]" if len(targets) == 1
                else f"{len(targets)} profiles")
        if not _confirm(f"Delete {what}? This removes "
                        f"{'its session' if len(targets) == 1 else 'their sessions'} too. (y/N) "):
            say("Aborted.")
            return emit({"ok": False, "aborted": True})

    from ..repo import repo

    coordinator = repo(store)
    removed, failed = [], []
    for ref, prof in targets:
        try:
            # Through the coordinator so the dashboard twin goes too. Deleting
            # only the engine record left a dashboard copy that the next
            # listing adopted straight back, id and all.
            #
            # `ref`, not `prof.id`: see above. And the answer is checked -
            # False means the profile was already gone, which is not a deletion
            # this command performed and must not be reported as one.
            if coordinator.delete(ref):
                removed.append({"id": prof.id, "name": prof.name})
                say(_c(f"Deleted '{prof.name}'.", C.GRN))
            else:
                missing.append(ref)
                say(_c(f"'{prof.name}' was already gone.", C.DIM))
        except Exception as exc:                                # noqa: BLE001
            # A target that moved, a name that became ambiguous, or a session
            # that could not be removed - reported, not swallowed.
            failed.append({"id": prof.id, "name": prof.name,
                           "error": str(exc).splitlines()[0][:200]})
            say(_c(f"Could not delete '{prof.name}': {exc}", C.RED))
    if len(removed) == 1 and not failed and not missing:
        return emit({"ok": True, **removed[0]})
    return emit({"ok": not failed and not missing, "removed": removed,
                 "missing": missing, "failed": failed},
                0 if not failed and not missing else 1)


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

    from ..repo import repo

    # The selection above is a *plan* - which profiles to touch - and nothing
    # more. The change itself is applied inside the transaction, to each
    # profile as it is at that moment.
    #
    # Two defects, one after the other. The loop used to save as it went, so a
    # failure on the Nth profile left the first N-1 changed under a command
    # that reported failure; `save_many` fixed that by writing them together.
    # But it still wrote the objects this loop had mutated, read before any
    # lock was held - so an API edit committing in between was acknowledged and
    # then silently overwritten. Making the write atomic could not repair a
    # decision taken outside it; `patch_many` moves the decision in.
    touched: dict = {}

    def change(prof):
        fields = apply_patch(prof, patch)
        if fields:
            touched[prof.id] = fields
        return fields

    written = repo(store).patch_many(
        [prof.id for prof in targets], change,
        config_change=({"default_engine": patch["engine"]}
                       if patch.get("engine") else None))
    for prof in written:
        say(f"  {prof.name:<24}{_c(', '.join(touched[prof.id]), C.DIM)}")
    say(_c(f"Updated {len(written)} of {len(targets)} profile(s).", C.GRN))
    return emit({"ok": True, "matched": len(targets), "updated": len(written),
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
    from ..repo import repo

    # One transaction, including the default-engine change, and including the
    # decision about which names are free. A partial bulk create is the worst
    # outcome here: nothing tells the user which half landed, and re-running
    # duplicates it. Reading the taken names *here* and choosing around them
    # was the same check-then-act - two of these running at once both saw the
    # same free names and created two profiles called each of them.
    created = repo(store).save_many(profiles, on_name_clash="skip", config_change=(
        {"default_engine": args.engine} if args.engine else None))
    skipped = len(profiles) - len(created)
    ids = [prof.id for prof in created]
    note = _c(f", skipped {skipped} name clash(es)", C.YEL) if skipped else ""
    say(_c(f"Created {len(ids)} profile(s)", C.GRN) + note
        + _c(f"  (engine {engine})", C.DIM))
    return emit({"created": len(ids), "skipped": skipped, "engine": engine, "ids": ids})


def cmd_import_from(args, store: ProfileStore) -> int:
    from .. import importers
    try:
        results, skipped = importers.import_file(args.file, store)
    except (FileNotFoundError, ValueError) as e:
        say(_c(f"Couldn't read {args.file}: {e}", C.RED))
        return emit({"ok": False, "error": f"couldn't read {args.file}: {e}"}, 1)
    if not results and not skipped:
        say(_c("No profiles found in that file.", C.RED))
        return emit({"ok": False, "error": "no profiles found in that file"}, 1)
    clean = sum(1 for _, issues in results if not issues)
    say(_c(f"Imported {len(results)} profile(s)  ", C.GRN)
        + _c(f"({clean} coherent out of the box)", C.DIM))
    for prof, issues in results:
        if issues:
            say(f"  {prof.name:<24}{_c('review: ' + '; '.join(issues), C.YEL)}")
    if skipped:
        say(_c(f"Skipped {len(skipped)} - those names are already in use: "
               + ", ".join(sorted(set(skipped))[:10]), C.YEL))
    say(_c("Run `numera check <name>` on any flagged profile to see details.", C.DIM))
    return emit({"ok": True, "imported": len(results), "coherent": clean,
                 "skipped": skipped,
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
