"""Profile CRUD, bulk operations, import, and the health and collision
overviews. Nothing here opens a browser."""

from __future__ import annotations

import uuid
from functools import lru_cache

from ..paths import safe_id
from ..handshake import ChildTerminationFailed

import json
import math
import os
from .. import devices, write_contract
from ..fingerprint import validate
from .stores import StaleProfile, UnconvertibleProfile
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

# One schema for every field a client may write, because half a schema is how
# `tags: [{}]` and `fontList: 17` both reached disk. A string where a list
# belongs is not a cosmetic problem - `p.tags.map(...)` takes the whole page
# down - and neither is a list whose *elements* are the wrong shape.
#
# Every key `client_fields()` exposes appears here, so adding a field to the
# converter and forgetting to type it is a visible omission rather than a
# silent hole. `test_the_schema_covers_every_writable_field` holds that line.
FIELD_TYPES = {
    "name": str, "notes": str, "folder": str, "engine": str, "os": str,
    "userAgent": str, "screen": str, "locale": str, "timezone": str,
    "browserVersion": str, "startupUrls": str, "extensions": str,
    "bookmarks": str, "launchArgs": str, "dns": str,
    "browser": str, "webglVendor": str, "webglRenderer": str, "canvas": str,
    "webrtc": str, "audio": str, "fonts": str, "clientRects": str,
    "webglImage": str, "mediaDevices": str, "geo": str, "fontList": str,
    "osVersion": str, "deviceModel": str,
    "tags": list, "ipBased": list, "clearOnStart": list, "switchedOff": list,
    "cores": (int, float), "memory": (int, float),
    "mediaVideo": (int, float), "mediaAudio": (int, float),
    "mediaSpeakers": (int, float), "maxTouchPoints": (int, float),
    "devicePixelRatio": (int, float), "seed": (int, float),
    "dnt": bool, "proxy": dict,
}

# What each number is allowed to *be*, beyond being a number. `NaN` and
# `Infinity` pass `isinstance(x, (int, float))`, are not JSON, and made
# `int(...)` raise inside the converter - which the write path then swallowed.
# The bounds are generous: they exist to refuse a value no device has, not to
# curate hardware.
NUMBER_RANGES = {
    "cores": (1, 512), "memory": (0, 1024),
    "mediaVideo": (0, 64), "mediaAudio": (0, 64), "mediaSpeakers": (0, 64),
    "maxTouchPoints": (0, 32), "devicePixelRatio": (0.05, 20),
    "seed": (0, 2 ** 53),
}

# The proxy is the one nested object a client sends, and it carries the
# credential - so its own fields are typed too. Unknown keys inside it are
# left alone: the pool adds its own bookkeeping and refusing that would be a
# guess about a shape this module does not own.
PROXY_TYPES = {"type": str, "host": str, "port": (str, int), "user": str,
               "pass": str, "country": str}


def _value_problems(key: str, value, where: str) -> list:
    """Everything a correctly *typed* value can still be wrong about."""
    if isinstance(value, list):
        bad = [type(v).__name__ for v in value if not isinstance(v, str)]
        return ([f"{where}: every entry in {key!r} must be a string, "
                 f"got {bad[0]}"] if bad else [])
    if key in NUMBER_RANGES:
        low, high = NUMBER_RANGES[key]
        # `math.isfinite` converts its argument to a float first, so it is not
        # a question an arbitrary JSON integer can answer: `math.isfinite(10 **
        # 400)` raises `OverflowError: int too large to convert to float` and
        # took the whole validator with it - a 500 where `cores: -4` already
        # gets a named 4xx. A Python `int` is exact and unbounded, so it is
        # compared against the bounds directly and only a `float` is asked
        # whether it is finite. Truncated in the message because the value
        # being quoted back is the client's.
        if isinstance(value, float) and not math.isfinite(value):
            return [f"{where}: {key!r} must be a finite number"]
        if not low <= value <= high:
            return [f"{where}: {key!r} must be between {low} and {high}, "
                    f"got {str(value)[:40]}"]
        return []
    if key == "proxy":
        # Keys only - a message is not a place to quote a password back.
        return [f"{where}: proxy {k!r} must be "
                f"{getattr(t, '__name__', None) or ' or '.join(x.__name__ for x in t)}"
                f", got {type(value[k]).__name__}"
                for k, t in PROXY_TYPES.items()
                if value.get(k) is not None
                and (not isinstance(value[k], t) or isinstance(value[k], bool))]
    return []


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
        value = patch[key]
        if want is bool and isinstance(value, bool):
            continue
        if want is not bool and isinstance(value, want) and not isinstance(value, bool):
            problems += _value_problems(key, value, where)
            continue
        names = getattr(want, "__name__", None) or " or ".join(t.__name__ for t in want)
        problems.append(f"{where}: {key!r} must be {names}, got "
                        f"{type(value).__name__}")
    return problems


def readable(profile: dict) -> dict:
    """A stored record the API can actually answer with.

    Two corruptions a record written before this schema existed can carry, and
    the only two that break a *reader* rather than a writer:

    * a non-finite number cannot be serialised at all - `NaN` and `Infinity`
      are not JSON - so one profile holding one made `GET /api/profiles` fail
      to serialise and took every other profile down with it;
    * a non-string entry in a list field reaches `p.tags.map(...)` and blanks
      the page, which is the poisoning this schema exists to stop.

    The stored file is not touched and no field disappears: the number comes
    back as `null` - the same "not set" every other absent field uses - and the
    list keeps the entries that are strings. A write that would carry the
    original value onward is refused by `check_fields`, which names the field.
    """
    # One implementation, in `write_contract`, with this shape's vocabulary.
    # A third repair lived here and did not cover the text fields, so a record
    # already holding `name: 123` or `name: null` still reached the dashboard,
    # which calls `p.name.toLowerCase()` on it. Refusing new writes never
    # repaired the installations that already had one.
    return write_contract.readable(
        profile,
        text_fields=("name", "notes", "folder", "engine", "os", "userAgent",
                     "screen", "locale", "timezone", "browserVersion",
                     "startupUrls", "extensions", "bookmarks", "launchArgs",
                     "dns"),
        list_fields=LIST_FIELDS)


def engine_view(profile: dict):
    """The engine shape of a stored profile, or None when it has none.

    For the read-only overviews only. `profiles_health` and
    `profiles_collisions` convert every stored profile, and one record the
    converter could not model raised straight out of the route - so a single
    bad profile made both pages unreachable for all of them. The write paths do
    the opposite and refuse; see `_validate_profile`.
    """
    try:
        return to_engine_profile(profile)
    except Exception:                                        # noqa: BLE001
        return None


def register(app, ctx):
    """Attach these routes to `app`.

    The names are unpacked from `ctx` so each body below sees exactly the
    environment it had inside `create_app` - the bodies are unchanged.
    """
    from fastapi import HTTPException
    from fastapi.responses import JSONResponse

    from ..repo import NameTaken, secret_intent

    engine_store, dash, sched, running, is_running, decorate, stop_session = ctx.engine_store, ctx.dash, ctx.sched, ctx.running, ctx.is_running, ctx.decorate, ctx.stop_session

    @app.exception_handler(NameTaken)
    def name_taken(request, exc: NameTaken):
        """One name, one profile - answered the same way on every route.

        Five separate routes write profiles (create, `PUT`, batch, bulk and
        import) and each of them can now be refused by the store's name check.
        A handler rather than five `try`/`except` blocks because the sixth
        write route somebody adds next would otherwise return the store's
        `ValueError` as a 500, which is the wrong answer to a request that is
        merely asking for a name somebody else has.

        The body matches `HTTPException`'s so a client cannot tell which
        mechanism produced it.
        """
        return JSONResponse(status_code=409, content={"detail": str(exc)})

    @app.exception_handler(UnconvertibleProfile)
    def unconvertible(request, exc: UnconvertibleProfile):
        """A record with no engine shape is a bad request, not a server fault.

        The store refuses it rather than writing the dashboard copy alone, and
        that refusal has to reach every write route - including the import,
        which builds its records from the vendor importer and never passes
        through `_validate_profile`. A handler rather than a `try` per route,
        for the same reason `NameTaken` has one.
        """
        return JSONResponse(status_code=422, content={"detail": str(exc)})

    def _validate_profile(profile: dict, *, pid: "str | None" = None,
                          strip: bool = True,
                          default_engine: "str | None" = None) -> dict:
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
        # The per-field type checks that used to sit here - four numbers and
        # the proxy - are `check_fields`' job and were only ever a subset of
        # it. Two half-schemas in one function is how the other half came to be
        # missing seventeen fields.

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
        # The saved creation default, applied *before* the conversion below.
        # `to_engine_profile` answers `playwright` for a profile that names no
        # engine, so filling from `complete` got there first and the create
        # route's own fallback - `if not profile.get("engine")` - was never
        # reached: a request omitting the engine produced a Playwright profile
        # while the config said Camoufox. Only the creation paths pass this;
        # an update must not reset the engine a profile already has.
        if default_engine and not profile.get("engine"):
            profile["engine"] = default_engine
        try:
            complete = to_dashboard_profile(to_engine_profile(profile))
        except Exception as exc:  # noqa: BLE001 - the message is the client's
            # Swallowed, this was the missing-mirror bug: the dashboard copy
            # was written from the fallback lists below and the engine copy -
            # which `_twin` could not build either - was silently skipped. The
            # two stores must move together, so a profile the engine cannot
            # model is refused rather than half-written.
            raise HTTPException(
                422, f"this profile cannot be converted for the engine: {exc}"
            ) from None
        for key, value in complete.items():
            if profile.get(key) is None:
                profile[key] = value
        # Belt and braces for the handful the view calls a method on. The
        # conversion above can no longer fail quietly, so these should never
        # fire; they stay because a blank profile list is the failure this
        # whole function exists to stop, and the cost of holding the line twice
        # is two loops over a dozen keys.
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
        # Under the transaction lock, because this endpoint *writes*. A delete
        # could remove the dashboard copy, this listing could observe the engine
        # copy that had not been removed yet, the delete could finish - and then
        # this would `save()` the stale engine object and resurrect a profile
        # the caller had been told was deleted.
        from ..paths import transaction_lock

        with transaction_lock(engine_store.root):
            profiles = dash.list()
            known = {p["id"] for p in profiles}
            adopting = []
            for prof in engine_store.list():
                if prof.id in known:
                    continue
                # Re-check inside the lock: `engine_store.list()` was taken from
                # the same snapshot, but the record must still exist to adopt.
                if not engine_store._path(prof.id).exists():
                    continue
                d = to_dashboard_profile(prof)
                d["id"] = prof.id
                adopting.append(d)
            # One batch, not one `save()` each. Every write now checks the name
            # it claims against every stored profile, so adopting a store the
            # CLI filled with N profiles would otherwise scan N records N times
            # - on the endpoint the UI polls. The batch makes that one scan, and
            # a store adopts either wholly or not at all.
            if adopting:
                profiles += dash.save_many(adopting)
        return [decorate(readable(p)) for p in profiles]

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
        chose_engine = bool(isinstance(profile, dict) and profile.get("engine"))
        # The stored default goes in before normalisation, not after it: by the
        # time this route ran, `_validate_profile` had already filled the
        # converter's own `playwright` in and `not profile.get("engine")` was
        # false for every request.
        _validate_profile(profile,
                          default_engine=engine_store.get_default_engine())
        _apply_default_extensions(profile)
        # One commit. `_remember_engine` wrote `config.json` *before* the
        # profile, so a failed profile write left the global default changed by
        # a request that returned an error.
        saved = dash.save(profile, config_change=(
            {"default_engine": profile["engine"]} if chose_engine else None),
            secret_intent=secret_intent(profile))
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
        # missing engine in from the conversion, so asking afterwards can never
        # tell what the caller actually sent. (It does not fill in the saved
        # *creation* default here - only the create and batch routes pass that,
        # because applying it on a patch would re-engine every profile edited
        # after the setting changed.)
        #
        # This used to remember on every PUT, so renaming a `playwright`
        # profile silently switched the global default away from whatever the
        # user had chosen - and nothing in the UI said so.
        asked = profile.get("engine")
        # Read the revision here too, for the same reason `asked` is read here:
        # `strip_server_owned` drops every `_`-prefixed key, so asking for it
        # after that point always finds nothing and the check silently does not
        # happen. That is exactly how the engine-explicitness bug worked.
        sent_rev = profile.get(dash.REVISION)
        if not isinstance(profile, dict):
            raise HTTPException(422, "profile must be an object")
        strip_server_owned(profile)
        problems = check_fields(profile, where="profile")
        if problems:
            raise HTTPException(422, "; ".join(problems[:8]))

        # A PUT is a patch, and the merge happens *inside* the store's lock.
        # Normalising the body on its own treated every omitted field as "reset
        # me": a name-only rename moved a Camoufox profile to Playwright (the
        # converter's default), emptied its tags, notes and folder, and minted a
        # fresh `_created`. Merging onto stored state fixed that - but merging
        # it out here left a second race, because two concurrent patches could
        # merge onto the same old record and the later write would carry stale
        # values for every field the earlier one had just changed. `expect_rev`
        # cannot help there: a patch carries no revision.
        def merge(current: dict):
            merged = {**current, **profile}
            _validate_profile(merged, pid=pid, strip=False)
            change = ({"default_engine": merged["engine"]}
                      if asked and asked != current.get("engine") else None)
            return merged, change

        try:
            # The intent comes from the *body*, not from the merged record:
            # a patch built on the stored profile echoes the stored password
            # for every field it is not changing, and reading that as a choice
            # is how an unrelated edit used to settle a credential conflict.
            saved = dash.patch(pid, merge, expect_rev=sent_rev,
                               secret_intent=secret_intent(profile))
        except StaleProfile as e:
            return JSONResponse({"detail": str(e), "kind": "StaleProfile"}, status_code=409)
        if saved is None:
            raise HTTPException(404, "profile not found")
        return decorate(saved)

    @app.delete("/api/profiles/{pid}")
    def delete_profile(pid: str):
        # Check before destroying anything. This used to delete the engine
        # record first and *then* discover the dashboard had no such profile,
        # so a 404 could arrive after an engine-only profile had already been
        # erased - the one response that promises nothing happened.
        from ..paths import SessionBusy

        if not dash.get(pid) and not engine_store.get(pid):
            raise HTTPException(404, "profile not found")
        # Only a session THIS server started can be stopped: `stop_session`
        # terminates a local `Popen`, and `is_running` now also reports a lease
        # held by another process. Calling it for a foreign session did nothing
        # and the route deleted anyway, out from under a live browser. The
        # lease inside `delete` is the real check; this just closes ours first
        # so the common case does not have to fail.
        if pid in running:
            try:
                stop_session(pid)
            except ChildTerminationFailed as e:
                # Deleting a profile whose browser is still running is how a
                # session outlives the identity it belongs to. The lease inside
                # `delete` would refuse it anyway; saying why here is clearer
                # than the busy message that would follow.
                raise HTTPException(409, str(e)) from None
        try:
            # One owner. The route deleted the engine copy and the dashboard
            # store deleted it again, each half-aware of the other.
            dash.delete(pid)
        except SessionBusy as e:
            raise HTTPException(409, str(e)) from None
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
            try:
                if is_running(pid):
                    stop_session(pid)
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
        # Read once, not per entry: every implicit entry in one request should
        # get the same default even if something else changes it mid-batch.
        # And applied *before* normalisation, for the reason `create_profile`
        # gives - afterwards the converter's `playwright` was already there.
        fallback = engine_store.get_default_engine()
        for p in incoming:
            _validate_profile(p, default_engine=fallback)
            _apply_default_extensions(p)
        last_engine = chosen[-1] if chosen else None
        # One transaction. A sequential loop left the profiles written before a
        # failing one on disk while the request reported failure.
        # ...including the default-engine change. Committing it afterwards
        # meant a failing config write reported failure over profiles that were
        # already on disk, and the retry duplicated every one of them.
        ids = [p["id"] for p in dash.save_many(
            incoming,
            config_change={"default_engine": last_engine} if last_engine else None)]
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
        # `extra=` tells the preflight to *allow* these two keys, which is not
        # the same as checking them - so they arrived untyped and only their
        # list-ness was looked at here. Their elements reach a set and a sort
        # in `merge` below: `addTags: [{}]` raised `unhashable type: 'dict'`
        # and `["a", 1]` raised inside `sorted`, both out of the store's lock
        # and both a 500 over a field the schema already knows how to refuse.
        # `_value_problems` is that answer - the same "every entry must be a
        # string" every other list field gets.
        for key in ("addTags", "removeTags"):
            if key not in patch:
                continue
            if not isinstance(patch[key], list):
                problems.append(f"patch: {key!r} must be a list")
            else:
                problems += _value_problems(key, patch[key], "patch")
        if problems:
            raise HTTPException(400, "; ".join(problems))
        # Built inside the store's lock, for the same reason the single patch
        # is: reading every target first and writing them later lets a bulk edit
        # carry values that changed in between.
        def merge(pid: str, d: dict):
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
            return d
        # One transaction. Saving in the loop meant a write failure on item N
        # left 0..N-1 edited while the request raised - and the obvious retry
        # applied the patch to part of the selection twice.
        # Config in the *same* batch as the data. After it, a failed config
        # write reported failure over profiles that had already committed;
        # before it, a failed profile write left the default changed. Only when
        # the patch names an engine - a tag-only bulk edit must not touch it.
        saved = dash.patch_many(
            ids, merge, config_change=(
                {"default_engine": patch["engine"]} if patch.get("engine") else None),
            secret_intent=secret_intent(patch))
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
        """Import profiles, from a Numera export or from a vendor's.

        Two formats, read two different ways, because they are two different
        things. A vendor file goes through the tolerant importer: it is
        somebody else's schema, so fields are looked for under every name they
        are known by and whatever is missing is generated coherently.

        Numera's own export is *configuration this install wrote*, and running
        it through those heuristics was destruction dressed as tolerance -
        `profile_from_export` reads no `seed`, no per-signal mode, no startup
        URLs, extensions, launch arguments, bookmarks, DNS or clear-on-start
        list, and reads `folder` only to append it to the tags. A user's own
        file came back as a different device in the wrong folder, reported as a
        successful import. So it is versioned, and read field for field.

        What is deliberately not preserved: the id and the store's own
        bookkeeping. The id is what the session directory, proxy assignments
        and revisions hang off, and a file cannot claim one.

        What cannot be preserved is said out loud rather than implied: a file
        from before the format was versioned is refused by name, and a proxy
        password the vault was holding when the file was written comes back as
        an issue on that profile, not as a silent blank.

        This is configuration, not a session backup: the browser profile
        directory - cookies, logins, local storage - is not in this file.
        """
        from .. import importers

        def read_native(entries: list) -> dict:
            """Save a versioned Numera export, field for field."""
            # The whole file is checked before any of it is written, exactly as
            # batch create does: half a rejected import on disk is worse than
            # none, because nothing tells the user which half.
            problems = []
            for i, entry in enumerate(entries):
                strip_server_owned(entry)     # `id` too: every entry is re-minted
                problems += check_fields(entry, where=f"profiles[{i}]")
            if problems:
                raise HTTPException(400, "; ".join(problems[:8]))
            results = []
            for entry in entries:
                px = entry.get("proxy")
                # `null` is how the listing reports a password the vault was holding
                # - an absent one reads as `""` - so this is the file saying "that
                # credential was not mine to write", not a profile without one.
                redacted = (isinstance(px, dict) and px.get("host")
                            and px.get("pass") is None)
                # Whether this entry *named* an engine, before normalisation fills
                # one in: an engine we supplied is not the file's choice.
                chose_engine = bool(entry.get("engine"))
                _validate_profile(entry)
                if not chose_engine:
                    entry["engine"] = engine_store.get_default_engine()
                # Not `_apply_default_extensions`: an export carries its own list,
                # and an empty one means this profile has no extensions.
                issues = dashboard_coherence(entry)
                if redacted:
                    issues.append("the proxy password was not in this file - the "
                                  "vault was holding it when the export was written; "
                                  "set it again on this profile")
                results.append({"name": entry.get("name"), "issues": issues})
            # No `config_change`. A native export names an engine on every profile,
            # so "last one wins" would reset the default on every import - the same
            # defect `_named_engine` exists to keep out of the vendor path. Opening
            # a file is not choosing an engine for the next new profile.
            dash.save_many(entries)
            return {"imported": len(results), "format": "numera", "results": results}

        text = body.get("text") or ""
        if not isinstance(text, str):
            raise HTTPException(400, f"'text' must be a string, got {type(text).__name__}")
        try:
            raw = json.loads(text)
        except json.JSONDecodeError:
            raise HTTPException(400, "that file isn't valid JSON")
        try:
            native = importers.native_export(raw)
        except ValueError as e:
            raise HTTPException(400, str(e)) from None
        if native is not None:
            return read_native(native)
        exports = importers._unwrap(raw)
        if not exports:
            raise HTTPException(400, "no profiles found in that file")
        # An unversioned Numera export - the shape 0.30 and earlier wrote. The
        # vendor importer would accept it and quietly drop most of it, so it is
        # named and refused instead.
        if any(importers.looks_native(e) for e in exports):
            raise HTTPException(
                400, "this looks like a Numera profile export written before "
                     "the format carried a version, and reading it with the "
                     "vendor importer would drop its fingerprint seed, signal "
                     "modes, startup URLs, extensions and folder. Re-export it "
                     "from the install that wrote it.")
        results, pending, explicit, problems = [], [], None, []
        for i, exp in enumerate(exports):
            try:
                prof, issues = importers.profile_from_export(exp)
                d = to_dashboard_profile(prof)
            except Exception as exc:                            # noqa: BLE001
                # A vendor file is arbitrary JSON from outside this install.
                # Being tolerant about the *name* a value hides under is this
                # importer's whole job; a traceback because the value under
                # that name had a type the conversion could not survive is not
                # tolerance, it is a 500 over a file the user merely opened.
                problems.append(
                    f"profiles[{i}]: this export cannot be read: {exc}")
                continue
            # `_new` is a dashboard form flag. Persisted, the imported profile
            # reloads as a create form and saves as a duplicate instead of an
            # update, with its cookie actions disabled.
            d.pop("_new", None)
            d.pop("id", None)                  # server-minted, like any create
            # The same contract every other write route holds, over the record
            # this one builds for itself. This branch had none at all: it went
            # straight from the importer to `save_many`, so `{"name": 123}`
            # reached disk with a numeric name - `readable()` repairs non-finite
            # numbers and list fields, not `name` - and the profile list calls
            # `p.name.toLowerCase()` on it, which throws and blanks the page for
            # every profile, not just that one.
            #
            # Refused by field rather than quietly rewritten: a vendor may call
            # a value whatever it likes, but there is no honest guess at what a
            # name that is not text was meant to say, and a record the write
            # contract refuses is a profile that can be opened and never saved
            # again. The same filter `_validate_profile(strip=False)` uses -
            # `status` and `lastActive` are the converter's own output, not the
            # file's claim.
            problems += check_fields(
                {k: v for k, v in d.items()
                 if k not in CLIENT_MAY_NOT_SET and not k.startswith("_")},
                where=f"profiles[{i}]")
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
        # After the whole file, not at the entry that failed: the refusal has to
        # name which entry, and an import that stops at the first bad one tells
        # the user nothing about the rest of their file. Nothing has been
        # written at this point either way - `save_many` is below.
        if problems:
            raise HTTPException(400, "; ".join(problems[:8]))
        # Same contract as batch create: an import either lands whole or not at
        # all. Saving as it went left a partial import that a retry duplicated.
        dash.save_many(pending, config_change=(
            {"default_engine": explicit} if explicit else None))
        return {"imported": len(results), "format": "vendor", "results": results}

    # ---- Fingerprint collisions -----------------------------------------
    @app.get("/api/profiles/collisions")
    def profiles_collisions():
        """Profiles that share a fingerprint — a cross-account linkage risk.

        Converts each stored dashboard profile to its engine fingerprint and
        groups the ones that look like the same device."""
        from ..collision import find_collisions
        # A record the converter cannot model has no fingerprint, so it cannot
        # collide with anything - but raising here made one such profile hide
        # the collisions of every other one.
        profs = [p for p in (engine_view(d) for d in dash.list()) if p]
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
        # `None` for a record this converter cannot model. One of those used to
        # raise out of the route and make the whole Health page unreachable;
        # the row below reports it as needing attention instead.
        engine_shaped = {d["id"]: engine_view(d) for d in profiles}
        for g in find_collisions([p for p in engine_shaped.values() if p]):
            colliding.update(m["id"] for m in g["members"])

        rows = []
        summary = {"total": 0, "running": 0, "coherent": 0, "withProxy": 0,
                   "scheduled": 0, "graded": 0, "colliding": 0, "needsAttention": 0}
        for d in profiles:
            pid = d["id"]
            eng = engine_shaped[pid]
            issues = (validate(eng.fingerprint) if eng else
                      ["this profile cannot be read; open it, correct the "
                       "highlighted field and save it again"])
            px = d.get("proxy") if isinstance(d.get("proxy"), dict) else {}
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
