"""What a profile must look like before anything persists it.

One contract, one module, no optional dependencies. The API had a full field
schema in `server/routes_profiles.py` and the core had a name check in
`repo.py`, so `tags: [{}]` was refused over HTTP and accepted by
`numera import-from` - two doors into the same store disagreeing about what a
profile is.

This module is the shared half: the invariants that hold wherever a record is
written, expressed over plain Python so the engine can import it with no
server extra installed. It deliberately does **not** try to be the API schema.
That schema also polices which *keys* a client may send, what the dashboard
shape looks like, and how a vendor field maps - none of which the CLI has an
opinion about. The rule here is narrower and absolute: a stored profile must be
one the rest of the product can read back.

Each surface adapts the result to its own error type: the API turns it into a
422 naming the field, the CLI into a refusal on stderr. Neither of them decides
*whether* it is a problem.
"""
from __future__ import annotations

import math

#: Fields whose elements the product dereferences as text. `p.tags.map(...)`
#: and `", ".join(profile.tags)` both assume strings; one dict in the list
#: takes down the page or the command that touches it.
TEXT_LIST_FIELDS = (
    "tags", "startup_urls", "extensions", "launch_args",
    "ip_based", "clear_on_start", "switched_off",
)

#: Fields the product treats as text outright.
TEXT_FIELDS = ("name", "notes", "engine", "dns")

#: Of those, the ones that may not be absent. `notes` and `dns` are genuinely
#: optional and default to empty; `name` is the profile's identity on every
#: surface - `resolve(name)`, the uniqueness check, the GUI's
#: `p.name.toLowerCase()` - so `None` there is a missing identity, not an
#: unset field, and is refused like any other non-text value.
REQUIRED_TEXT_FIELDS = ("name",)

#: Numeric bounds shared by both write paths. Generous by design: they exist to
#: refuse a value no device has, not to curate hardware.
NUMBER_RANGES = {
    "cores": (1, 512), "memory": (0, 1024),
    "mediaVideo": (0, 64), "mediaAudio": (0, 64), "mediaSpeakers": (0, 64),
    "maxTouchPoints": (0, 32), "devicePixelRatio": (0.05, 20),
    "seed": (0, 2 ** 53),
}

#: Where those same numbers live on an engine `Profile`: one level down, under
#: the names the browser APIs use. The bounds above are keyed by the
#: dashboard's vocabulary and were read off the *top level* of whatever
#: arrived - which is `None` for every one of them on the shape the core write
#: path actually carries. So `fingerprint.seed = float("inf")` was written by
#: `ProfileStore.save` while the flat `{"seed": inf}` was refused over HTTP:
#: the advertised numeric contract held only on the shape that already had a
#: second schema in front of it. This map is the one place the two vocabularies
#: meet, so there is still exactly one set of bounds to disagree about.
FINGERPRINT_NUMBERS = {
    "cores": "hardware_concurrency",
    "memory": "device_memory",
    "mediaVideo": "cameras",
    "mediaAudio": "microphones",
    "mediaSpeakers": "speakers",
    "maxTouchPoints": "max_touch_points",
    "devicePixelRatio": "device_pixel_ratio",
    "seed": "seed",
}

#: Fingerprint collections the product joins or iterates - `", ".join(langs)`
#: reaches the launch layer and one integer in the list stops a browser
#: starting.
FINGERPRINT_LIST_FIELDS = ("languages", "fonts")

#: Proxy fields that must be text, in both vocabularies at once: an engine
#: `Proxy` calls them `server`/`username`/`password`, a stored dashboard record
#: `host`/`user`/`pass`. Absent is not wrong, so one table covers either shape
#: and there is no second copy to drift. `port` is deliberately absent: vendors
#: export it as text as often as as a number and the product reads both.
PROXY_TEXT_FIELDS = ("server", "username", "password", "country",
                     "type", "host", "user", "pass")


def _name(value) -> str:
    return type(value).__name__


def text_problem(field: str, value, where: str):
    """Why `value` cannot be stored as `field`'s text, or None."""
    if value is None and field not in REQUIRED_TEXT_FIELDS:
        return None                      # absent is not wrong; empty is text
    if not isinstance(value, str):
        return (f"{where}: {field!r} must be text, not {_name(value)} "
                f"({value!r}). Nothing was written.")
    return None


def list_problem(field: str, value, where: str):
    """Why `value` cannot be stored as `field`'s list of text, or None."""
    if value is None:
        return None
    if not isinstance(value, (list, tuple)):
        return (f"{where}: {field!r} must be a list, not {_name(value)}. "
                f"Nothing was written.")
    for item in value:
        if not isinstance(item, str):
            return (f"{where}: every entry in {field!r} must be text, "
                    f"got {_name(item)}. Nothing was written.")
    return None


def number_problem(field: str, value, where: str, *, label=None):
    """Why `value` cannot be stored as `field`'s number, or None.

    `field` selects the bounds, `label` names the field in the message. They
    differ for a nested value: a refusal that says `cores` to a caller who set
    `hardware_concurrency` is a refusal about a field that caller cannot find.

    Integers are compared without converting to float: `math.isfinite` builds
    one first, and `10**400` raises `OverflowError` out of the check rather
    than being refused by it. An `int` is exact and unbounded in Python, so the
    bounds compare directly and only a genuine `float` is asked about infinity.
    """
    if value is None or field not in NUMBER_RANGES:
        return None
    shown = label or field
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        return (f"{where}: {shown!r} must be a number, not {_name(value)}. "
                f"Nothing was written.")
    if isinstance(value, float) and not math.isfinite(value):
        return f"{where}: {shown!r} must be a finite number. Nothing was written."
    low, high = NUMBER_RANGES[field]
    if not low <= value <= high:
        # Truncated: the value being quoted back is the caller's, and CPython
        # decodes an integer of up to 4300 digits before refusing one. The API
        # copy of this check already bounded its message; this one did not, so
        # the CLI could print four thousand digits at a refusal.
        return (f"{where}: {shown!r} must be between {low} and {high}, "
                f"got {str(value)[:40]}. Nothing was written.")
    return None


def _reader(container):
    """Read fields off a mapping or an object with one call, or None.

    All three shapes a write arrives in reach this: the CLI holds an engine
    `Profile`, a batch or a backup holds `to_dict()` output, the API holds a
    dashboard payload. Adapting the *access* is the whole difference between
    them, so the rule below it stays single.
    """
    if container is None:
        return None
    if hasattr(container, "get"):
        return container.get
    return lambda name: getattr(container, name, None)


def _fingerprint_problems(fingerprint, where: str) -> list:
    """The numbers and collections an engine profile keeps one level down."""
    read = _reader(fingerprint)
    if read is None:
        return []
    found = []
    for field, engine_name in FINGERPRINT_NUMBERS.items():
        problem = number_problem(field, read(engine_name), where,
                                 label=f"fingerprint.{engine_name}")
        if problem:
            found.append(problem)
    for name in FINGERPRINT_LIST_FIELDS:
        problem = list_problem(f"fingerprint.{name}", read(name), where)
        if problem:
            found.append(problem)
    return found


def _proxy_problems(proxy, where: str) -> list:
    """The credential block, under whichever set of names it arrived with.

    This had no core check at all: `Proxy.server = 123` reached the driver's
    `to_playwright()` and a list password reached the vault. A proxy that is
    not a proxy did not even get that far - `repo._secrets_for` asks it for
    `password_intent` and a string raised `AttributeError` out of the writer,
    where a refusal naming the field belongs.

    Read, never assigned. `Proxy.__setattr__` records that `password` was
    *chosen*, and that flag is what tells a write to preserve, clear or replace
    the stored secret; a validator that wrote the value back - even the value
    already there - would turn every untouched proxy into a replacement.
    """
    if proxy is None:
        return []
    if not hasattr(proxy, "get") and not hasattr(proxy, "server"):
        return [f"{where}: 'proxy' must be a proxy or absent, not "
                f"{_name(proxy)}. Nothing was written."]
    read = _reader(proxy)
    found = []
    for name in PROXY_TEXT_FIELDS:
        problem = text_problem(f"proxy.{name}", read(name), where)
        if problem:
            found.append(problem)
    return found


def profile_problems(profile, *, where: str = "profile") -> list:
    """Every reason this profile may not be persisted. Empty means it may.

    Accepts an engine `Profile` (attributes), a serialized engine mapping
    (`to_dict()` keys) or a dashboard payload, because the CLI holds the first,
    the batch paths hold the second and the API holds the third - and
    duplicating the rule per shape is how the two doors drifted apart in the
    first place.

    The top-level pass is the dashboard's flat vocabulary; `fingerprint` and
    `proxy` are where an engine profile keeps the same values. Both passes run
    over whatever arrives: a shape that has no such block reads `None` and is
    charged nothing for the lookup.
    """
    get = _reader(profile)
    found = []
    for field in TEXT_FIELDS:
        problem = text_problem(field, get(field), where)
        if problem:
            found.append(problem)
    for field in TEXT_LIST_FIELDS:
        problem = list_problem(field, get(field), where)
        if problem:
            found.append(problem)
    for field in NUMBER_RANGES:
        problem = number_problem(field, get(field), where)
        if problem:
            found.append(problem)
    found += _fingerprint_problems(get("fingerprint"), where)
    found += _proxy_problems(get("proxy"), where)
    return found


class WriteRefused(ValueError):
    """A record was refused before anything was written.

    `ValueError` so that a caller with no opinion about this module still
    handles it as bad input; the API adapter turns it into a 422.
    """


def require(profile, *, where: str = "profile") -> None:
    """Raise `WriteRefused` unless this profile may be persisted."""
    found = profile_problems(profile, where=where)
    if found:
        raise WriteRefused("; ".join(found))


# --------------------------------------------------------------------------
# Reading back what an older build stored
# --------------------------------------------------------------------------
def readable_value(field: str, value, *,
                   text_fields=TEXT_FIELDS, list_fields=TEXT_LIST_FIELDS):
    """The value a reader can render, for a record written before this contract.

    Refusing new writes does not repair an installation that already holds
    `name: 123` or `name: null`; the dashboard still calls
    `p.name.toLowerCase()` on it and blanks the page. The stored bytes are not
    touched - this is the view, and it is deliberately lossy in one direction
    only: a number becomes its own text, a null becomes empty, a list keeps the
    entries that are strings. Nothing is invented, and no field disappears.
    """
    if field in text_fields:
        if value is None:
            return ""
        if isinstance(value, bool):
            return str(value).lower()
        if isinstance(value, (int, float)):
            return format(value, "g") if isinstance(value, float) else str(value)
        if not isinstance(value, str):
            return ""
        return value
    if field in list_fields:
        if not isinstance(value, (list, tuple)):
            return []
        if isinstance(value, list) and all(isinstance(v, str) for v in value):
            return value                 # already renderable; hand back the same
        return [v for v in value if isinstance(v, str)]
    if isinstance(value, float) and not math.isfinite(value):
        return None                      # not JSON; `null` is the honest answer
    return value


def readable(record: dict, *,
             text_fields=TEXT_FIELDS, list_fields=TEXT_LIST_FIELDS) -> dict:
    """A stored record a reader can render. The stored file is not touched.

    The field names are a parameter because the two shapes genuinely differ -
    the engine calls it `startup_urls` and the dashboard `startupUrls`, and the
    dashboard's is a string where the engine's is a list. One implementation,
    two vocabularies, rather than two copies of the repair drifting apart.
    """
    if not isinstance(record, dict):
        return record
    fixed = {}
    for key, value in record.items():
        safe = readable_value(key, value, text_fields=text_fields,
                              list_fields=list_fields)
        if safe is not value:
            fixed[key] = safe
    # A key that is not there has no item, so the loop above never saw it. That
    # left the repair helping a name that was present and wrong while an older
    # record with no `name` at all came back nameless - and `p.name.toLowerCase()`
    # throws on `undefined` exactly as it does on a number, blanking the page for
    # every profile rather than the one. Same answer a stored `null` already
    # gets, and only for a field this vocabulary owns.
    for field in REQUIRED_TEXT_FIELDS:
        if field in text_fields and field not in record:
            fixed[field] = ""
    return {**record, **fixed} if fixed else record
