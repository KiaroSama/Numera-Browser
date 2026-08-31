"""Profile CRUD, bulk operations, import, and the health and collision
overviews. Nothing here opens a browser."""

from __future__ import annotations

import uuid

import json
import os
from .. import devices
from ..fingerprint import validate
from ..models import Fingerprint
from .convert import _rel_time, dashboard_coherence, to_dashboard_profile, to_engine_profile


# What the dashboard dereferences rather than merely displays. Missing means a
# blank page, not a blank cell. The main fill comes from the engine's own
# defaults (round-tripping through `to_engine_profile`), so the row the dashboard
# draws is the identity the browser will actually wear; this list is the fallback
# for a shape too odd to convert.
LIST_FIELDS = ("tags", "ipBased", "clearOnStart", "switchedOff")
TEXT_FIELDS = {"startupUrls": "", "extensions": "", "bookmarks": "",
               "launchArgs": "", "dns": "", "notes": "",
               # `browserVersion` is split on "." in the profile list.
               "browserVersion": "128.0.0.0"}


# Fields a client may never set, whatever the route.
#
# `id` is what every profile file, session directory and proxy assignment hangs
# off; a patch that rewrites it orphans all three. `_new` is a dashboard-only
# form flag - persisted, the profile reloads as a create form and saves as a
# duplicate. `_created`/`_last_active` are the store's own bookkeeping.
CLIENT_MAY_NOT_SET = {"id", "_new", "_created", "_last_active"}

# Types the dashboard dereferences. A string where a list belongs is not a
# cosmetic problem: `p.tags.map(...)` takes the whole page down.
FIELD_TYPES = {
    "name": str, "notes": str, "folder": str, "engine": str, "os": str,
    "userAgent": str, "screen": str, "locale": str, "timezone": str,
    "browserVersion": str, "startupUrls": str, "extensions": str,
    "bookmarks": str, "launchArgs": str, "dns": str,
    "tags": list, "ipBased": list, "clearOnStart": list, "switchedOff": list,
    "cores": (int, float), "memory": (int, float),
    "mediaVideo": (int, float), "mediaAudio": (int, float), "seed": (int, float),
    "dnt": bool, "proxy": dict,
}


def check_fields(patch: dict, *, where: str) -> "list[str]":
    """Every reason this payload may not be written. Empty means it may."""
    problems = []
    if not isinstance(patch, dict):
        return [f"{where} must be an object"]
    for key in sorted(set(patch) & CLIENT_MAY_NOT_SET):
        problems.append(f"{where}: {key!r} is assigned by the server and cannot be set")
    for key, want in FIELD_TYPES.items():
        if key not in patch or patch[key] is None:
            continue
        if want is bool and isinstance(patch[key], bool):
            continue
        if want is not bool and isinstance(patch[key], want) and not isinstance(patch[key], bool):
            continue
        names = getattr(want, "__name__", None) or " or ".join(t.__name__ for t in want)
        problems.append(f"{where}: {key!r} must be {names}, got "
                        f"{type(patch[key]).__name__}")
    return problems


def register(app, ctx):
    """Attach these routes to `app`.

    The names are unpacked from `ctx` so each body below sees exactly the
    environment it had inside `create_app` - the bodies are unchanged.
    """
    from fastapi import HTTPException

    engine_store, dash, sched, running, is_running, decorate, stop_session = ctx.engine_store, ctx.dash, ctx.sched, ctx.running, ctx.is_running, ctx.decorate, ctx.stop_session

    def _validate_profile(profile: dict) -> dict:
        """Reject shapes the dashboard cannot render, before they reach disk.

        An unrenderable profile used to be stored happily and then take the
        whole UI down on the next load, so the write path is where it has to
        be caught."""
        from .. import drivers  # local, matching the other call sites in this module
        if not isinstance(profile, dict):
            raise HTTPException(422, "profile must be an object")
        problems = check_fields(
            {k: v for k, v in profile.items() if k not in ("id", "_new")},
            where="profile")
        if problems:
            raise HTTPException(422, "; ".join(problems[:8]))
        eng = profile.get("engine")
        if eng is not None:
            if not isinstance(eng, str) or eng not in drivers.names():
                raise HTTPException(
                    422, f"unknown engine {eng!r}; known: {', '.join(drivers.names())}")
        for key in ("cores", "memory", "mediaVideo", "mediaAudio"):
            val = profile.get(key)
            if val is not None and not isinstance(val, (int, float)):
                raise HTTPException(422, f"'{key}' must be a number")
        px = profile.get("proxy")
        if px is not None and not isinstance(px, dict):
            raise HTTPException(422, "'proxy' must be an object or null")

        # Type checks alone did not deliver what the docstring promises. A body
        # like {"name": "alpha"} passed every check above and was stored with no
        # `tags` at all; the list view calls `p.tags.map(...)` and the whole page
        # went blank on the next load - the exact failure this function exists to
        # stop. A minimal POST is a reasonable request, so the missing fields are
        # filled rather than refused.
        # Mint the id first. The seed that drives canvas, audio and ClientRects
        # noise is derived from it, and this ran before the store assigned one -
        # so every profile created without an explicit seed hashed the literal
        # string "unsaved" and they all shared a fingerprint.
        if not profile.get("id"):
            profile["id"] = uuid.uuid4().hex[:12]
        try:
            complete = to_dashboard_profile(to_engine_profile(profile))
        except Exception:  # noqa: BLE001 - a shape too odd to convert keeps the
            complete = {}  # explicit list below; never refuse the write for this
        for key, value in complete.items():
            if profile.get(key) is None:
                profile[key] = value
        # Belt and braces for the handful the view calls a method on, in case the
        # conversion above could not run at all.
        for key in LIST_FIELDS:
            if not isinstance(profile.get(key), list):
                profile[key] = []
        for key, default in TEXT_FIELDS.items():
            if not isinstance(profile.get(key), str):
                profile[key] = default
        return profile

    def _remember_engine(profile: dict):
        # "Last engine wins" - but only for an engine the caller actually chose.
        # An engine we filled in from the current default is not a choice, and
        # persisting it lets a client's guess overwrite the real default.
        eng = profile.get("engine")
        if eng:
            engine_store.set_default_engine(eng)

    @app.get("/api/profiles")
    def list_profiles():
        # Adopt anything the CLI created that the dashboard has never seen.
        # `to_dashboard_profile` is the existing reverse mapping; without this
        # the two stores only converge in one direction and a headless-created
        # profile stays invisible in the GUI forever (B17).
        known = {p["id"] for p in dash.list()}
        for prof in engine_store.list():
            if prof.id not in known:
                d = to_dashboard_profile(prof)
                d["id"] = prof.id
                dash.save(d)
        return [decorate(p) for p in dash.list()]

    def _apply_default_extensions(profile: dict) -> dict:
        # The dashboard keeps extensions as newline-separated text. Only fill it
        # in when the caller left it empty, so an explicit choice always wins.
        if not (profile.get("extensions") or "").strip():
            defaults = engine_store.get_default_extensions()
            if defaults:
                profile["extensions"] = "\n".join(defaults)
        return profile

    @app.post("/api/profiles")
    def create_profile(profile: dict):
        profile.pop("_new", None)
        chose_engine = bool(profile.get("engine"))
        _validate_profile(profile)
        _apply_default_extensions(profile)
        if chose_engine:
            _remember_engine(profile)          # an explicit choice, worth remembering
        elif not profile.get("engine"):
            profile["engine"] = engine_store.get_default_engine()
        saved = dash.save(profile)
        return decorate(saved)

    @app.put("/api/profiles/{pid}")
    def update_profile(pid: str, profile: dict):
        if not dash.get(pid):
            raise HTTPException(404, "profile not found")
        profile["id"] = pid
        profile.pop("_new", None)
        _validate_profile(profile)
        _remember_engine(profile)
        return decorate(dash.save(profile))

    @app.delete("/api/profiles/{pid}")
    def delete_profile(pid: str):
        if is_running(pid):
            stop_session(pid)
        engine_store.delete(pid)
        if not dash.delete(pid):
            raise HTTPException(404, "profile not found")
        return {"ok": True}

    @app.post("/api/profiles/batch")
    def batch_create(body: dict):
        """Save many dashboard profiles in one request (bulk create at scale).

        The dashboard generates coherent profiles client-side and posts them in
        chunks; this just persists them, assigning each a fresh unique id."""
        incoming = body.get("profiles") or []
        # Validate the whole batch before writing any of it. Half a batch on
        # disk is worse than none, because nothing tells the user which half.
        problems = []
        for i, item in enumerate(incoming):
            problems += check_fields(
                {k: v for k, v in (item or {}).items() if k not in ("id", "_new")},
                where=f"profiles[{i}]")
        if problems:
            raise HTTPException(400, "; ".join(problems[:8]))
        ids = []
        last_engine = None
        for p in incoming:
            p.pop("_new", None)
            p.pop("id", None)           # always mint a server-side id
            _apply_default_extensions(p)
            if not p.get("engine"):
                p["engine"] = engine_store.get_default_engine()
            last_engine = p.get("engine")
            ids.append(dash.save(p)["id"])
        if last_engine:
            engine_store.set_default_engine(last_engine)
        return {"created": len(ids), "ids": ids}

    @app.post("/api/profiles/bulk")
    def bulk_update(body: dict):
        """Apply one patch to many profiles — the dashboard's multi-select edit."""
        ids = body.get("ids") or []
        patch = body.get("patch") or {}
        # One patch, many profiles: a bad field would otherwise be written to
        # every one of them before anything noticed.
        problems = check_fields({k: v for k, v in patch.items()
                                 if k not in ("addTags", "removeTags")},
                                where="patch")
        for key in ("addTags", "removeTags"):
            if key in patch and not isinstance(patch[key], list):
                problems.append(f"patch: {key!r} must be a list")
        if problems:
            raise HTTPException(400, "; ".join(problems))
        updated = []
        for pid in ids:
            d = dash.get(pid)
            if not d:
                continue
            for key, value in patch.items():
                if key == "addTags":
                    d["tags"] = sorted({*d.get("tags", []), *value})
                elif key == "removeTags":
                    d["tags"] = [t for t in d.get("tags", []) if t not in value]
                elif key == "locale":
                    # Locale drives timezone, so move both or the profile stops
                    # being coherent.
                    d["locale"] = value
                    d["timezone"] = devices.LOCALES.get(value, (d.get("timezone"), None))[0]
                else:
                    d[key] = value
            _remember_engine(d)
            updated.append(decorate(dash.save(d)))
        return updated

    @app.post("/api/fingerprint/validate")
    def validate_fingerprint(profile: dict):
        return {"issues": dashboard_coherence(profile)}

    @app.post("/api/profiles/import")
    def import_profiles(body: dict):
        """Import profiles exported from GoLogin / AdsPower / Multilogin.

        Runs the engine's tolerant importer, converts each result to the
        dashboard shape and saves it, flagging any coherence issues for review."""
        from .. import importers
        text = body.get("text") or ""
        try:
            raw = json.loads(text)
        except json.JSONDecodeError:
            raise HTTPException(400, "that file isn't valid JSON")
        exports = importers._unwrap(raw)
        if not exports:
            raise HTTPException(400, "no profiles found in that file")
        results = []
        for exp in exports:
            prof, issues = importers.profile_from_export(exp)
            d = to_dashboard_profile(prof)
            d["_new"] = True
            _remember_engine(d)
            dash.save(d)
            results.append({"name": prof.name, "issues": issues})
        return {"imported": len(results), "results": results}

    # ---- Fingerprint collisions -----------------------------------------
    @app.get("/api/profiles/collisions")
    def profiles_collisions():
        """Profiles that share a fingerprint — a cross-account linkage risk.

        Converts each stored dashboard profile to its engine fingerprint and
        groups the ones that look like the same device."""
        from ..collision import find_collisions
        profs = [to_engine_profile(d) for d in dash.list()]
        return {"total": len(profs), "groups": find_collisions(profs)}

    # ---- Health overview -------------------------------------------------
    @app.get("/api/profiles/health")
    def profiles_health():
        """One row per profile with everything the Health page needs, computed
        cheaply. Coherence and proxy are read straight from the stored profile;
        the trust grade is the *cached* one from the last trust/align run (a live
        trust check opens a real browser, too heavy to do for the whole grid)."""
        by_profile: dict[str, dict] = {}
        for s in sched.list():
            # A profile keeps its most-frequent (smallest-interval) schedule row.
            cur = by_profile.get(s["profileId"])
            if cur is None or s.get("everyHours", 1e9) < cur.get("everyHours", 1e9):
                by_profile[s["profileId"]] = s

        profiles = dash.list()
        # Which profiles share a fingerprint with another? (a linkage risk)
        from ..collision import find_collisions
        colliding: set[str] = set()
        for g in find_collisions([to_engine_profile(d) for d in profiles]):
            colliding.update(m["id"] for m in g["members"])

        rows = []
        summary = {"total": 0, "running": 0, "coherent": 0, "withProxy": 0,
                   "scheduled": 0, "graded": 0, "colliding": 0, "needsAttention": 0}
        for d in profiles:
            pid = d["id"]
            issues = dashboard_coherence(d)
            px = d.get("proxy") or {}
            has_proxy = bool(px.get("host"))
            trust = d.get("_trust")
            sc = by_profile.get(pid)
            run_now = is_running(pid)
            grade = (trust or {}).get("grade")
            collides = pid in colliding
            # "Needs attention" = something a user would want to fix: incoherent,
            # a known-poor trust grade, or a fingerprint shared with another profile.
            attention = bool(issues) or (grade in ("C", "D")) or collides

            rows.append({
                "id": pid,
                "name": d.get("name", pid),
                "engine": d.get("engine", "playwright"),
                "os": d.get("os", "Windows"),
                "status": "running" if run_now else "stopped",
                "lastActive": "now" if run_now else _rel_time(d.get("_last_active")),
                "coherent": not issues,
                "issues": issues,
                "collides": collides,
                "proxy": {"set": has_proxy, "country": px.get("country") or None,
                          "type": px.get("type") if has_proxy else None},
                "trust": trust,
                "schedule": ({"enabled": sc.get("enabled", True),
                              "everyHours": sc.get("everyHours"),
                              "preset": sc.get("preset")} if sc else None),
                "needsAttention": attention,
            })
            summary["total"] += 1
            summary["running"] += run_now
            summary["coherent"] += not issues
            summary["withProxy"] += has_proxy
            summary["scheduled"] += sc is not None
            summary["graded"] += trust is not None
            summary["colliding"] += collides
            summary["needsAttention"] += attention

        return {"summary": summary, "profiles": rows}
