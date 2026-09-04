"""Profile CRUD, bulk operations, import, and the health and collision
overviews. Nothing here opens a browser."""

from __future__ import annotations

import uuid
from functools import lru_cache

from ..paths import safe_id

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
CLIENT_MAY_NOT_SET = {"id", "_new", "_created", "_last_active", "_trust",
                      "_trustAt", "status", "lastActive", "profiles"}


def strip_server_owned(patch: dict) -> list:
    """Remove every key the server owns from `patch`; return what was removed.

    An explicit list misses the next `_field` somebody adds, so the underscore
    prefix is reserved wholesale: those are the store's own bookkeeping.

    Dropped, not refused. `GET /api/profiles` returns `status` and `lastActive`
    along with the store's `_`-prefixed fields, so a client that reads a
    profile, changes one value and sends it back is sending exactly what it was
    given. Refusing that made the API reject its own output: inline rename in
    the dashboard posts `{...p, name}` straight from the list and got a 422.
    The client's value still never reaches disk, which was the actual goal.
    """
    dropped = sorted(k for k in patch
                     if k in CLIENT_MAY_NOT_SET or (k.startswith("_") and k != "_"))
    for key in dropped:
        patch.pop(key, None)
    return dropped


@lru_cache(maxsize=1)
def client_fields() -> frozenset:
    """Every key a client may send on a profile.

    Derived from the converter's own output instead of hand-listed, so adding a
    field to the dashboard shape carries the contract with it and cannot leave a
    real field being refused as unknown.
    """
    return frozenset(to_dashboard_profile(to_engine_profile({})))

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


def check_fields(patch: dict, *, where: str, extra=()) -> "list[str]":
    """Every reason this payload may not be written. Empty means it may."""
    problems = []
    if not isinstance(patch, dict):
        return [f"{where} must be an object"]
    # Unknown keys are refused rather than stored. A typo used to persist as a
    # field nothing reads and nothing shows - a dead control that looks saved,
    # and the same shape of defect as a switch that reports instead of acting.
    # Call `strip_server_owned` first; the keys it owns are not unknown.
    for key in sorted(set(patch) - client_fields() - set(extra)):
        problems.append(f"{where}: unknown field {key!r}")
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

    def _validate_profile(profile: dict, *, pid: "str | None" = None,
                          strip: bool = True) -> dict:
        """Reject shapes the dashboard cannot render, before they reach disk.

        An unrenderable profile used to be stored happily and then take the
        whole UI down on the next load, so the write path is where it has to
        be caught."""
        from .. import drivers  # local, matching the other call sites in this module
        if not isinstance(profile, dict):
            raise HTTPException(422, "profile must be an object")
        if strip:
            strip_server_owned(profile)  # takes `id` and `_new` with it
        # `strip=False` is for a record already merged onto stored state: the
        # patch was stripped before the merge, and stripping again here would
        # throw away the server-owned bookkeeping (`_created`, `_trust`, ...)
        # this merge exists to preserve.
        problems = check_fields({k: v for k, v in profile.items()
                                 if strip or k not in CLIENT_MAY_NOT_SET and not k.startswith("_")},
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
        # Create mints; update keeps the id from the path. Minting on both was
        # a data-loss bug, not just a cosmetic one: `PUT /api/profiles/{pid}`
        # set the id and then this line replaced it, so every edit wrote a
        # *second* profile and left the original untouched. Accepting a client
        # id is still not on offer - that is how `"../../escaped"` once reached
        # the store's path builder.
        if pid is None:
            profile["id"] = uuid.uuid4().hex[:12]
        else:
            try:
                profile["id"] = safe_id(pid)
            except ValueError as exc:
                raise HTTPException(422, str(exc)) from None
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
        # One listing, not three. This read the dashboard store, read the
        # engine store, and then read the dashboard store *again* for the
        # response - on an endpoint the UI polls every three seconds, with the
        # vault decrypting every record on each pass.
        profiles = dash.list()
        known = {p["id"] for p in profiles}
        for prof in engine_store.list():
            if prof.id not in known:
                d = to_dashboard_profile(prof)
                d["id"] = prof.id
                profiles.append(dash.save(d))
        return [decorate(p) for p in profiles]

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
        # The stored record as written, server-owned bookkeeping and all. The
        # decrypted view is not used as the base: it reports a locked secret as
        # None, and writing that back would destroy the proxy password.
        stored = dash._raw(pid)
        if not stored:
            raise HTTPException(404, "profile not found")

        # Read explicitness *before* normalisation: `_validate_profile` fills a
        # missing engine in from the current default, so asking afterwards can
        # never tell what the caller actually sent.
        #
        # This used to remember on every PUT, so renaming a `playwright`
        # profile silently switched the global default away from whatever the
        # user had chosen - and nothing in the UI said so.
        asked = profile.get("engine")
        if not isinstance(profile, dict):
            raise HTTPException(422, "profile must be an object")
        strip_server_owned(profile)
        problems = check_fields(profile, where="profile")
        if problems:
            raise HTTPException(422, "; ".join(problems[:8]))

        # A PUT is a patch. Normalising the body on its own treated every
        # omitted field as "reset me": a name-only rename moved a Camoufox
        # profile to Playwright (the converter's default), emptied its tags,
        # notes and folder, and minted a fresh `_created`. Merge onto what is
        # stored, then normalise the complete result.
        merged = {**stored, **profile}
        _validate_profile(merged, pid=pid, strip=False)
        if asked and asked != stored.get("engine"):
            _remember_engine(merged)
        return decorate(dash.save(merged))

    @app.delete("/api/profiles/{pid}")
    def delete_profile(pid: str):
        # Check before destroying anything. This used to delete the engine
        # record first and *then* discover the dashboard had no such profile,
        # so a 404 could arrive after an engine-only profile had already been
        # erased - the one response that promises nothing happened.
        if not dash.get(pid) and not engine_store.get(pid):
            raise HTTPException(404, "profile not found")
        if is_running(pid):
            stop_session(pid)
        # One owner. The route deleted the engine copy and the dashboard store
        # deleted it again, each half-aware of the other.
        dash.delete(pid)
        return {"ok": True}

    @app.post("/api/profiles/delete")
    def delete_profiles(body: dict):
        """Delete many profiles in one call.

        The dashboard looped `DELETE /api/profiles/{id}` per selection, so
        removing 200 profiles was 200 round trips - the odd one out beside
        `/batch` and `/bulk`, which are both batched.

        Each delete keeps its own semantics: existence checked before anything
        is destroyed, one owner for both copies, session directory last. A
        profile that fails is reported rather than aborting the rest, because a
        half-finished delete of an explicit selection is worse than a partial
        one the caller can see and retry.
        """
        ids = body.get("ids")
        if not isinstance(ids, list) or not all(isinstance(i, str) for i in ids):
            raise HTTPException(400, "'ids' must be a list of profile ids")
        removed, missing, failed = [], [], []
        for pid in ids:
            if not dash.get(pid) and not engine_store.get(pid):
                missing.append(pid)
                continue
            if is_running(pid):
                stop_session(pid)
            try:
                dash.delete(pid)
                removed.append(pid)
            except Exception as exc:                            # noqa: BLE001
                failed.append({"id": pid, "error": str(exc).splitlines()[0][:200]})
        return {"removed": removed, "missing": missing, "failed": failed}

    @app.post("/api/profiles/batch")
    def batch_create(body: dict):
        """Save many dashboard profiles in one request (bulk create at scale).

        The dashboard generates coherent profiles client-side and posts them in
        chunks; this just persists them, assigning each a fresh unique id."""
        incoming = body.get("profiles") or []
        if not isinstance(incoming, list):
            raise HTTPException(400, f"'profiles' must be a list, got {type(incoming).__name__}")
        if not all(isinstance(i, dict) for i in incoming):
            raise HTTPException(400, "every entry in 'profiles' must be an object")
        # Validate the whole batch before writing any of it. Half a batch on
        # disk is worse than none, because nothing tells the user which half.
        problems = []
        for i, item in enumerate(incoming):
            strip_server_owned(item)     # `id` too: every entry is server-minted
            problems += check_fields(item, where=f"profiles[{i}]")
        if problems:
            raise HTTPException(400, "; ".join(problems[:8]))
        # Through the same normalisation a single create uses. Batch only
        # type-checked the fields that were present and then saved, so
        # `{"name": "one"}` landed with no `tags` at all - and the profile list
        # calls `p.tags.some(...)`, which takes the page down on the next load.
        # `_validate_profile` also mints the id first, which is what the noise
        # seed is derived from, so two minimal entries get different identities
        # instead of sharing one.
        # Which entries named an engine, before any auto-fill overwrites the
        # answer. `last_engine` used to be read after the fill, so in a mixed
        # batch an entry that named nothing could override an explicit choice
        # made earlier in the same request.
        chosen = [p.get("engine") for p in incoming if p.get("engine")]
        for p in incoming:
            _validate_profile(p)
            _apply_default_extensions(p)
            if not p.get("engine"):
                p["engine"] = engine_store.get_default_engine()
        last_engine = chosen[-1] if chosen else None
        # One transaction. A sequential loop left the profiles written before a
        # failing one on disk while the request reported failure.
        ids = [p["id"] for p in dash.save_many(incoming)]
        if last_engine:
            engine_store.set_default_engine(last_engine)
        return {"created": len(ids), "ids": ids}

    @app.post("/api/profiles/bulk")
    def bulk_update(body: dict):
        """Apply one patch to many profiles — the dashboard's multi-select edit."""
        ids = body.get("ids") or []
        patch = body.get("patch") or {}
        if not isinstance(ids, list) or not all(isinstance(i, str) for i in ids):
            raise HTTPException(400, "'ids' must be a list of profile ids")
        if not isinstance(patch, dict):
            raise HTTPException(400, f"'patch' must be an object, got {type(patch).__name__}")
        # One patch, many profiles: a bad field would otherwise be written to
        # every one of them before anything noticed.
        strip_server_owned(patch)
        problems = check_fields(patch, where="patch",
                                extra=("addTags", "removeTags"))
        for key in ("addTags", "removeTags"):
            if key in patch and not isinstance(patch[key], list):
                problems.append(f"patch: {key!r} must be a list")
        if problems:
            raise HTTPException(400, "; ".join(problems))
        pending = []
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
            # The same validator a single-profile PUT runs. Bulk checked only
            # `check_fields`, to which {"engine": "bogus"} is a perfectly good
            # string - so an unknown engine reached every selected profile and
            # was promoted to the global default from there.
            _validate_profile(d, pid=pid, strip=False)
            pending.append(d)
        # One transaction. Saving in the loop meant a write failure on item N
        # left 0..N-1 edited while the request raised - and the obvious retry
        # applied the patch to part of the selection twice.
        saved = dash.save_many(pending)
        # Config *after* data. `_remember_engine` writes `config.json`, which is
        # outside the profile batch; running it in the loop meant a failing
        # `save_many` rolled the profiles back and left the default engine set
        # by a request that failed. Only when the patch names one - a tag-only
        # bulk edit must not touch the default at all.
        if patch.get("engine") and pending:
            _remember_engine(patch)
        return [decorate(p) for p in saved]

    @app.post("/api/fingerprint/validate")
    def validate_fingerprint(profile: dict):
        return {"issues": dashboard_coherence(profile)}

    def _named_engine(exp: dict) -> "str | None":
        """The Numera engine this export explicitly names, or None.

        A vendor export names a browser, not one of our engines, so this is
        almost always None - which is the point: the importer's fallback is a
        default, not a choice, and must not be persisted as one.

        It returns the value rather than a bool because the previous version
        only *detected* the choice. `profile_from_export` does not carry an
        engine through, so an export naming `camoufox` produced a Playwright
        profile whose global default was then set to Camoufox - the one engine
        the profile was not going to use.
        """
        from .. import drivers

        if not isinstance(exp, dict):
            return None
        known = set(drivers.names())
        for key in ("engine", "launcher", "browserEngine"):
            value = exp.get(key)
            if isinstance(value, str) and value.strip().lower() in known:
                return value.strip().lower()
        return None

    @app.post("/api/profiles/import")
    def import_profiles(body: dict):
        """Import profiles exported from GoLogin / AdsPower / Multilogin.

        Runs the engine's tolerant importer, converts each result to the
        dashboard shape and saves it, flagging any coherence issues for review."""
        from .. import importers
        text = body.get("text") or ""
        if not isinstance(text, str):
            raise HTTPException(400, f"'text' must be a string, got {type(text).__name__}")
        try:
            raw = json.loads(text)
        except json.JSONDecodeError:
            raise HTTPException(400, "that file isn't valid JSON")
        exports = importers._unwrap(raw)
        if not exports:
            raise HTTPException(400, "no profiles found in that file")
        results, pending, explicit = [], [], None
        for exp in exports:
            prof, issues = importers.profile_from_export(exp)
            d = to_dashboard_profile(prof)
            # `_new` is a dashboard form flag. Persisted, the imported profile
            # reloads as a create form and saves as a duplicate instead of an
            # update, with its cookie actions disabled.
            d.pop("_new", None)
            d.pop("id", None)                  # server-minted, like any create
            # The importer fills the engine in from the current default when the
            # export named none. Remembering that as "last engine wins" lets an
            # import silently change the user's default to something nobody
            # picked, so only an engine the export actually specified counts.
            chosen = _named_engine(exp)
            if chosen:
                d["engine"] = chosen           # the export's choice, on the profile
                explicit = chosen              # last explicit choice wins
            pending.append(d)
            results.append({"name": prof.name, "issues": issues})
        # Same contract as batch create: an import either lands whole or not at
        # all. Saving as it went left a partial import that a retry duplicated.
        dash.save_many(pending)
        # Config after data, for the same reason as bulk: a failed import must
        # not leave the default engine changed.
        if explicit:
            _remember_engine({"engine": explicit})
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
        # Convert once and reuse. `dashboard_coherence(d)` converts internally
        # too, so every profile was mapped to engine shape twice per page load.
        engine_shaped = {d["id"]: to_engine_profile(d) for d in profiles}
        for g in find_collisions(list(engine_shaped.values())):
            colliding.update(m["id"] for m in g["members"])

        rows = []
        summary = {"total": 0, "running": 0, "coherent": 0, "withProxy": 0,
                   "scheduled": 0, "graded": 0, "colliding": 0, "needsAttention": 0}
        for d in profiles:
            pid = d["id"]
            issues = validate(engine_shaped[pid].fingerprint)
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
