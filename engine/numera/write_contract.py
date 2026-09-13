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

**Which fields** are checked is not decided here either - it is `field_policy`,
one entry per model field, diffed against the dataclasses by a coverage test.
This module is the rule; that one is the list. They were the same thing once,
and the result was a validator that checked twelve fields of a fifty-six-field
record and could not say which forty-four it had skipped.

Three shapes arrive, and telling them apart is the point of `shape()`:

* a **complete engine object** - a `Profile`, what the CLI and the store hold;
* a **complete engine record** - `to_dict()` output, what a batch or a backup
  carries;
* a **partial dashboard patch** - flat dashboard vocabulary, what an API
  caller `PUT`s, where an absent field means "not changing this one".

The first two are complete, so a missing required field is a missing field. The
third is not, so it is checked for what it *states* and nothing else; the API
merges it over the stored record, and the merged result is what reaches the
complete check through the converter. Conflating the two is how `seed = None`
came to be stored: absent and null were the same answer to a validator that
only ever saw one of the shapes.
"""
from __future__ import annotations

import math

from . import field_policy
from .field_policy import NUMBER_RANGES        # noqa: F401 - re-export; the API
from .models import Fingerprint, Profile, Proxy  # reads it off this module

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

#: Proxy fields that must be text, in both vocabularies at once: an engine
#: `Proxy` calls them `server`/`username`/`password`, a stored dashboard record
#: `host`/`user`/`pass`. Absent is not wrong, so one table covers either shape
#: and there is no second copy to drift. `port` is deliberately absent: vendors
#: export it as text as often as as a number and the product reads both.
PROXY_TEXT_FIELDS = ("server", "username", "password", "country",
                     "type", "host", "user", "pass")

#: Never quoted back in a refusal, under either vocabulary. The message says
#: what the field must be; a validator is not a place to print a credential,
#: and these messages reach stderr, the API response body and the logs.
SECRET_FIELDS = ("password", "pass")


def _name(value) -> str:
    return type(value).__name__


def text_problem(field: str, value, where: str, *, optional=None, quote=True):
    """Why `value` cannot be stored as `field`'s text, or None.

    `optional` overrides the module-level `REQUIRED_TEXT_FIELDS` answer, which
    only knows the dashboard's flat names; a policy entry states requiredness
    per field and passes it in. `quote` is off for a credential - see
    `SECRET_FIELDS`.
    """
    if optional is None:
        optional = field.rsplit(".", 1)[-1] not in REQUIRED_TEXT_FIELDS
    if value is None and optional:
        return None                      # absent is not wrong; empty is text
    if not isinstance(value, str):
        shown = f" ({value!r})" if quote else ""
        return (f"{where}: {field!r} must be text, not {_name(value)}"
                f"{shown}. Nothing was written.")
    return None


def list_problem(field: str, value, where: str, *, optional=True):
    """Why `value` cannot be stored as `field`'s list of text, or None."""
    if value is None:
        if optional:
            return None
        return (f"{where}: {field!r} must be a list, not None. "
                f"Nothing was written.")
    if not isinstance(value, (list, tuple)):
        return (f"{where}: {field!r} must be a list, not {_name(value)}. "
                f"Nothing was written.")
    for item in value:
        if not isinstance(item, str):
            return (f"{where}: every entry in {field!r} must be text, "
                    f"got {_name(item)}. Nothing was written.")
    return None


def number_problem(field: str, value, where: str, *, label=None, bounds=None,
                   whole=False, optional=True):
    """Why `value` cannot be stored as `field`'s number, or None.

    `field` selects the bounds, `label` names the field in the message. They
    differ for a nested value: a refusal that says `cores` to a caller who set
    `hardware_concurrency` is a refusal about a field that caller cannot find.

    Integers are compared without converting to float: `math.isfinite` builds
    one first, and `10**400` raises `OverflowError` out of the check rather
    than being refused by it. An `int` is exact and unbounded in Python, so the
    bounds compare directly and only a genuine `float` is asked about infinity.

    `whole` is the difference between a count and a ratio. `navigator`
    reports `hardwareConcurrency` and `deviceMemory` as whole numbers and a
    screen has a whole number of pixels, while `devicePixelRatio` is 1.5 on a
    real display - so "is this a number in range" was the wrong question for
    every field but that one, and `hardware_concurrency = 8.5` passed it.
    """
    shown = label or field
    if value is None:
        if optional:
            return None
        return f"{where}: {shown!r} is required. Nothing was written."
    if bounds is None:
        if field not in NUMBER_RANGES:
            return None
        bounds = NUMBER_RANGES[field]
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        return (f"{where}: {shown!r} must be a number, not {_name(value)}. "
                f"Nothing was written.")
    if isinstance(value, float) and not math.isfinite(value):
        return f"{where}: {shown!r} must be a finite number. Nothing was written."
    if whole and not isinstance(value, int) and value != int(value):
        return (f"{where}: {shown!r} must be a whole number, got "
                f"{str(value)[:40]}. Nothing was written.")
    low, high = bounds
    if low is not None and high is not None and not low <= value <= high:
        # Truncated: the value being quoted back is the caller's, and CPython
        # decodes an integer of up to 4300 digits before refusing one. The API
        # copy of this check already bounded its message; this one did not, so
        # the CLI could print four thousand digits at a refusal.
        return (f"{where}: {shown!r} must be between {low} and {high}, "
                f"got {str(value)[:40]}. Nothing was written.")
    return None


def bool_problem(field: str, value, where: str, *, label=None):
    """Why `value` cannot be stored as `field`'s boolean, or None.

    `is_mobile = 2` is truthy, so every branch behaved and nothing complained -
    then `to_dict()` wrote `2`, JSON round-tripped it as `2`, and the field
    stayed an integer in a record that claims to hold a flag.
    """
    shown = label or field
    if isinstance(value, bool):
        return None
    return (f"{where}: {shown!r} must be true or false, not {_name(value)}. "
            f"Nothing was written.")


def screen_problem(value, where: str, *, field: str = "screen"):
    """Why `value` cannot be stored as the dashboard's packed resolution, or None.

    `screen` is the dashboard's single name for two engine fields: `convert`
    writes `f"{screen_width}x{screen_height}"` and parses it back with `int()`,
    falling back to 1920x1080 when that raises. So every dimension rule this
    module applies to the engine record could be walked straight past by
    sending the pair as text - `"0x0"` produced an engine twin with a zero
    screen, and `"potato"` silently replaced the caller's resolution with a
    default while the request reported success.

    The bounds are the engine field's own, read off the policy rather than
    repeated, so the packed form and the nested form cannot come to disagree.
    """
    if value is None:
        return None
    problem = text_problem(field, value, where)
    if problem:
        return problem
    parts = value.lower().split("x")
    low, high = field_policy.FINGERPRINT["screen_width"].bounds()
    try:
        sides = [int(part) for part in parts]
    except ValueError:
        sides = []
    if len(sides) != 2 or not all(low <= side <= high for side in sides):
        return (f"{where}: {field!r} must be two whole numbers between {low} "
                f"and {high} as '<width>x<height>', got {value[:40]!r}. "
                f"Nothing was written.")
    return None


#: Dashboard keys that carry an engine field in a packed form of their own, so
#: the nested rule cannot see them. Each one is a way round a check the engine
#: record already has, which is why they are listed rather than left to the
#: generic type table.
PACKED_FIELDS = ("screen",)


def packed_problems(field: str, value, where: str) -> list:
    """What a packed dashboard field is still wrong about, if anything.

    The API's schema types `screen` as a string and stops there, so this is the
    hook that lets the one contract finish the sentence for the keys where
    "a string" is not the whole rule. Empty for every other key.
    """
    if field == "screen":
        problem = screen_problem(value, where)
        return [problem] if problem else []
    return []


def bookmarks_problem(field: str, value, where: str):
    """Why `value` cannot be stored as the bookmark list, or None.

    The dashboard converter reads `b.get("name")` and `b["url"]` off every
    entry, so a bare string or number in the list raised `AttributeError` out
    of the write path, and `{"name": 1}` was formatted into the stored text as
    though the number had been a title.
    """
    if value is None:
        return None
    if not isinstance(value, (list, tuple)):
        return (f"{where}: {field!r} must be a list, not {_name(value)}. "
                f"Nothing was written.")
    for entry in value:
        if not hasattr(entry, "get"):
            return (f"{where}: every entry in {field!r} must be a "
                    f"{{'name', 'url'}} object, got {_name(entry)}. "
                    f"Nothing was written.")
        for key, optional in (("url", False), ("name", True)):
            problem = text_problem(f"{field}[].{key}", entry.get(key), where,
                                   optional=optional)
            if problem:
                return problem
    return None


# --------------------------------------------------------------------------
# Shapes
# --------------------------------------------------------------------------
ENGINE_OBJECT, ENGINE_RECORD, DASHBOARD_PATCH = (
    "engine object", "engine record", "dashboard patch")


def shape(profile) -> str:
    """Which of the three shapes this is. See the module docstring.

    Nesting is the tell, and it is a reliable one: an engine record keeps its
    fingerprint one level down under `fingerprint`, and the dashboard's payload
    is flat by construction - `cores` and `seed` sit at the top or not at all.
    """
    if isinstance(profile, Profile):
        return ENGINE_OBJECT
    if hasattr(profile, "get") and profile.get("fingerprint") is not None:
        return ENGINE_RECORD
    return DASHBOARD_PATCH


def _reader(container, table):
    """Read fields off a mapping or a matching object, or None if it is neither.

    Returning `None` for anything else is the whole fix for one of the two
    halves of this defect. The generic version was
    `lambda name: getattr(container, name, None)`, which answers `None` for
    every lookup on an object that has no such attribute - so `fingerprint =
    123` produced a reader that reported all thirty-five fields absent, the
    checks found nothing to say about any of them, and the record was passed as
    valid. It then crashed in `Fingerprint.to_dict()` a few frames later with
    `AttributeError: 'int' object has no attribute 'to_dict'`, which is a
    traceback where a refusal naming the field belongs - and, over HTTP, a 500
    where a 422 belongs.

    An object qualifies by carrying the fields the table names, not by its
    class: `Fingerprint` and `Proxy` do, and so does a stand-in built for a
    test, while an int, a string, a list and `None` do not.
    """
    if hasattr(container, "get"):
        return container.get
    if all(hasattr(container, name) for name in table):
        return lambda name: getattr(container, name, None)
    return None


def _rule_problem(rule, field: str, value, where: str, label: str):
    """One field's verdict, applied. `None` means the value may be stored."""
    if rule.kind in field_policy.EXEMPT:
        return None                      # server-owned or reshaped; see `why`
    if rule.kind == "text":
        return text_problem(label, value, where, optional=rule.optional,
                            quote=field not in SECRET_FIELDS)
    if rule.kind == "texts":
        return list_problem(label, value, where, optional=rule.optional)
    if rule.kind in ("int", "number"):
        return number_problem(field, value, where, label=label,
                              bounds=rule.bounds(), whole=rule.kind == "int",
                              optional=rule.optional)
    if rule.kind == "bool":
        return bool_problem(field, value, where, label=label)
    if rule.kind == "bookmarks":
        return bookmarks_problem(label, value, where)
    if rule.kind == "fingerprint":
        return _nested(value, field_policy.FINGERPRINT, Fingerprint,
                       "fingerprint", where, rule)
    if rule.kind == "proxy":
        return _nested(value, field_policy.PROXY, Proxy, "proxy", where, rule)
    raise AssertionError(f"field_policy has no rule kind {rule.kind!r}")


def _nested(container, table, model, label: str, where: str, rule):
    """A nested block's own verdict, or the first problem inside it.

    Returns a list so the caller can flatten one level; a container that is not
    one at all yields a single refusal naming the field, which is the answer
    `fingerprint = 123` should have had all along.
    """
    if container is None:
        if rule.optional:
            return []
        return [f"{where}: {label!r} is required and must be a "
                f"{model.__name__.lower()}. Nothing was written."]
    read = _reader(container, table)
    if read is None:
        return [f"{where}: {label!r} must be a {model.__name__.lower()} or "
                f"absent, not {_name(container)}. Nothing was written."]
    return _table_problems(read, table, where, prefix=f"{label}.")


def _table_problems(read, table, where: str, prefix: str = "") -> list:
    """Every field of one table, in table order."""
    found = []
    for field, rule in table.items():
        problem = _rule_problem(rule, field, read(field), where,
                                f"{prefix}{field}")
        if isinstance(problem, list):
            found += problem
        elif problem:
            found.append(problem)
    return found


def _dashboard_problems(patch, where: str) -> list:
    """A partial dashboard payload, checked for what it states.

    Requiredness deliberately does not apply: an absent key here means "not
    changing this one", and the API merges the patch over the stored record
    before anything complete is built from it. The flat names are this shape's
    own - `cores`, `seed`, `startupUrls` - and the numbers resolve to the same
    `NUMBER_RANGES` the nested fields use, so there is still one set of bounds.
    """
    get = patch.get
    found = []
    for field in TEXT_FIELDS:
        if field not in patch:
            continue        # absent means "not changing this one" - `name`
        problem = text_problem(field, get(field), where)   # included, which is
        if problem:                                        # the whole point of
            found.append(problem)                          # a patch shape
    for field in TEXT_LIST_FIELDS:
        problem = list_problem(field, get(field), where)
        if problem:
            found.append(problem)
    for field in NUMBER_RANGES:
        problem = number_problem(field, get(field), where)
        if problem:
            found.append(problem)
    for field in PACKED_FIELDS:
        found += packed_problems(field, get(field), where)
    proxy = get("proxy")
    if proxy is None:
        return found
    if not hasattr(proxy, "get") and not hasattr(proxy, "server"):
        found.append(f"{where}: 'proxy' must be a proxy or absent, not "
                     f"{_name(proxy)}. Nothing was written.")
        return found
    read = _reader(proxy, PROXY_TEXT_FIELDS) or (lambda n: getattr(proxy, n, None))
    for name in PROXY_TEXT_FIELDS:
        problem = text_problem(f"proxy.{name}", read(name), where,
                               quote=name not in SECRET_FIELDS)
        if problem:
            found.append(problem)
    return found


def profile_problems(profile, *, where: str = "profile") -> list:
    """Every reason this profile may not be persisted. Empty means it may.

    Dispatches on `shape()`, because the three shapes disagree about what an
    absent field means and a single pass over all of them had to pick one
    answer. It picked "absent", which is right for a patch and wrong for a
    record - so a complete engine profile carrying `seed = None` was stored.
    """
    kind = shape(profile)
    if kind is DASHBOARD_PATCH:
        if not hasattr(profile, "get"):
            # Not a record, not a patch, not a `Profile`. The generic reader
            # used to answer `None` to every lookup here too, so an integer
            # handed to this function came back reported as a profile whose
            # name was unset rather than as something that is not a profile.
            return [f"{where}: a profile must be a record or a patch, not "
                    f"{_name(profile)}. Nothing was written."]
        return _dashboard_problems(profile, where)
    read = (profile.get if kind is ENGINE_RECORD
            else lambda name: getattr(profile, name, None))
    return _table_problems(read, field_policy.PROFILE, where)


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


def readable_names(pairs) -> list:
    """`(id, name)` pairs with every name repaired to the text a reader sees.

    The uniqueness check builds a `{id: {names}}` map out of what is on disk,
    which puts every stored name into a `set` - so one legacy record holding
    `name: ["a"]` raised `TypeError: unhashable type: 'list'` out of
    `name_clash`, and the store stopped accepting *any* new profile. A record
    an older build wrote cannot be allowed to make the store unwritable for
    everything else; refusing new damage and repairing the view of old damage
    are the two halves of the same rule, and this is the second one applied at
    the one read the write path performs.

    Repairing rather than dropping is deliberate: a stored `123` is shown as
    `"123"` by every surface, so a new profile asking for that name is asking
    for one that is already visibly taken, and `resolve("123")` would be
    ambiguous forever if this let it through.
    """
    return [(pid, readable_value("name", name)) for pid, name in pairs]


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
