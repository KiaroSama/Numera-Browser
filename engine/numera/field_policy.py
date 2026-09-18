"""What every field of a stored profile is allowed to be, field by field.

`write_contract` holds the *rule* - what "must be text" means, what a refusal
reads like. This module holds the *list*: one entry per field of `Profile`,
`Fingerprint` and `Proxy`, so that "is this field checked?" has an answer that
can be read off a table instead of inferred from which branch of a validator
happens to mention it.

The reason it is a table and not more branches is what the audit found. The
validator checked eight numbers and four lists picked by hand, and every other
field of a 35-field `Fingerprint` was simply not mentioned anywhere - so
`screen_width = float("inf")` was written to disk as the non-JSON token
`Infinity`, `user_agent = 123` was written as an integer, and `seed = None`
was written over a field the noise generator dereferences. None of those were
decisions; they were fields nobody had listed. A hand-picked list cannot say
which fields it left out, and that is the defect.

So the table is checked against the dataclasses themselves. `unpoliced()` diffs
`dataclasses.fields()` against the entries below and the coverage test fails on
anything it finds, which means a field added to a model tomorrow breaks the
build until somebody says what it may contain. There is deliberately no
catch-all entry and no default rule: "not listed" has to stay an error, because
an entry that accepts anything is indistinguishable from the gap it would hide.

Three verdicts are available per field and only three:

``Rule(...)``   validated - the value must satisfy the named check;
``OWNED``       server-owned - the write path overwrites it, so the caller's
                value is not persisted and checking it would refuse a value
                that never reaches disk;
``NORMALIZED``  intentionally reshaped before storage, with the rule saying
                what it becomes.

`OWNED` and `NORMALIZED` both carry a `why`, and the coverage test requires it
to be non-empty: a field is exempt because somebody wrote down a reason, never
because the table was quiet about it.
"""
from __future__ import annotations

from dataclasses import dataclass, fields

#: Numeric bounds, keyed by the dashboard's vocabulary because that is the
#: vocabulary the API's own schema reports to a client. Generous by design:
#: they exist to refuse a value no device has, not to curate hardware.
#:
#: They live here rather than in `write_contract` so the entries below can name
#: a bound instead of repeating one. `cores` and `hardware_concurrency` are the
#: same limit under two names, and a second copy is precisely how the two write
#: doors came to disagree about `seed` in the first place.
NUMBER_RANGES = {
    "cores": (1, 512), "memory": (0, 1024),
    "mediaVideo": (0, 64), "mediaAudio": (0, 64), "mediaSpeakers": (0, 64),
    "maxTouchPoints": (0, 32), "devicePixelRatio": (0.05, 20),
    "seed": (0, 2 ** 53),
}

#: Pixels. A display no bigger than this is not a real constraint, which is the
#: point: the bound exists so `inf`, `-1` and `10**400` cannot reach a field
#: that `convert` formats into `"{w}x{h}"` and parses back with `int()`.
_MAX_PIXELS = 65536


@dataclass(frozen=True)
class Rule:
    """One field's verdict.

    `kind` selects the check; `dash` names the entry in `NUMBER_RANGES` when
    the bounds are shared with the dashboard's name for the same value, and
    `low`/`high` state them outright when they are not.
    """

    kind: str
    dash: str = ""
    low: float | None = None
    high: float | None = None
    optional: bool = False          # None is a legitimate "not set"
    why: str = ""                   # required for OWNED / NORMALIZED
    #: Sub-integer values this field accepts despite being a whole number
    #: otherwise. A narrow, documented compatibility list - not a licence to
    #: be fractional. See `device_memory`.
    fractions: tuple = ()

    def bounds(self):
        """The numeric range this rule polices."""
        if self.dash:
            return NUMBER_RANGES[self.dash]
        return self.low, self.high


def OWNED(why: str) -> Rule:
    """The write path sets this field itself; the caller's value never lands."""
    return Rule("owned", why=why)


def NORMALIZED(why: str) -> Rule:
    """Stored in a reshaped form, described by `why`."""
    return Rule("normalized", why=why)


# --------------------------------------------------------------------------
# Fingerprint
# --------------------------------------------------------------------------
#: Every field is `required` - that is, may not be `None`. The defaults on the
#: dataclass exist so a profile written by an older build still *loads*; they
#: do not make the field optional on the way in. Each one is dereferenced by
#: something: `webrtc.capitalize()` in the dashboard converter, `seed` by the
#: noise generator, `", ".join(languages)` by the launch layer. `None` in any
#: of them is a crash somewhere downstream of the write that allowed it, which
#: is why the refusal belongs here.
FINGERPRINT: dict[str, Rule] = {
    # Identity strings. The UA goes through a regex on import and into the
    # request headers at launch; the platform tokens are read by the patch
    # layer verbatim.
    "os": Rule("text"),
    "user_agent": Rule("text"),
    "platform": Rule("text"),
    "ch_platform": Rule("text"),
    "chrome_version": Rule("text"),
    "webgl_vendor": Rule("text"),
    "webgl_renderer": Rule("text"),
    "locale": Rule("text"),
    "timezone": Rule("text"),
    "os_version": Rule("text"),
    "device_model": Rule("text"),
    # Signal modes. Text, not a fixed choice: the converter calls
    # `.capitalize()` on each of them - which is what a list argument crashed
    # on - while an unrecognised *word* is a question about which modes the
    # patch layer honours, not about whether the record can be stored.
    "webrtc": Rule("text"),
    "canvas": Rule("text"),
    "audio": Rule("text"),
    "font_mode": Rule("text"),
    "geolocation": Rule("text"),
    "client_rects": Rule("text"),
    "webgl_image": Rule("text"),
    "media_mode": Rule("text"),
    # Dimensions. `convert` writes these into the dashboard's packed
    # `"{w}x{h}"` string and parses them back with `int()`, so a non-finite or
    # negative value becomes `"infx1080"` on disk - which that parser rejects
    # and silently replaces with 1920x1080, losing the profile's real screen.
    "screen_width": Rule("int", low=1, high=_MAX_PIXELS),
    "screen_height": Rule("int", low=1, high=_MAX_PIXELS),
    "viewport_width": Rule("int", low=1, high=_MAX_PIXELS),
    "viewport_height": Rule("int", low=1, high=_MAX_PIXELS),
    # Counts a browser API reports as whole numbers. `navigator`'s side of
    # each of these is an integer, so a fractional one is a value no device
    # reports - and `hardware_concurrency = 8.5` passed the range check that
    # was the only thing asked of it.
    "hardware_concurrency": Rule("int", dash="cores"),
    # Whole, **except** for the two values the API it imitates actually
    # reports below 1. The Device Memory draft defines the value as a power of
    # two, so 0.25 and 0.5 are real readings from small devices and 3.5 is not
    # a reading at all. Requiring a whole number refused the first two; calling
    # the field an open number would have accepted the third.
    #
    # The converter used to settle the argument by casting with `int`, which
    # turned 0.25 into 0 - a legal amount, so nothing downstream noticed the
    # value had been replaced.
    "device_memory": Rule("int", dash="memory", fractions=(0.25, 0.5)),
    "cameras": Rule("int", dash="mediaVideo"),
    "microphones": Rule("int", dash="mediaAudio"),
    "speakers": Rule("int", dash="mediaSpeakers"),
    "max_touch_points": Rule("int", dash="maxTouchPoints"),
    "seed": Rule("int", dash="seed"),
    # The one genuinely fractional number: 1.5 and 2.0 are ordinary values for
    # a real display, so this is the field where a float is not a mistake.
    "device_pixel_ratio": Rule("number", dash="devicePixelRatio"),
    # Collections the product joins or iterates. One integer in `languages`
    # stops a browser starting.
    "languages": Rule("texts"),
    "fonts": Rule("texts"),
    # Booleans the launch layer branches on. `is_mobile = 2` is truthy and
    # behaved, then round-tripped through JSON as an integer and stayed one.
    "is_mobile": Rule("bool"),
    "do_not_track": Rule("bool"),
}


# --------------------------------------------------------------------------
# Proxy
# --------------------------------------------------------------------------
#: `password` is **read** here and never assigned. `Proxy.__setattr__` records
#: that the password was *chosen*, and that flag is what tells a write to
#: preserve, clear or replace the stored secret - so a validator that wrote the
#: value back, even the value already there, would turn every untouched proxy
#: into a replacement and overwrite the vault copy on an unrelated note edit.
PROXY: dict[str, Rule] = {
    "server": Rule("text"),
    "username": Rule("text", optional=True),
    "password": Rule("text", optional=True),
    "country": Rule("text", optional=True),
}


# --------------------------------------------------------------------------
# Profile
# --------------------------------------------------------------------------
PROFILE: dict[str, Rule] = {
    # The profile's identity on every surface: `resolve(name)`, the uniqueness
    # check, and the dashboard's `p.name.toLowerCase()`.
    "name": Rule("text"),
    "engine": Rule("text"),
    "notes": Rule("text"),
    "dns": Rule("text"),
    # Used as a dictionary key by the uniqueness check and as a filename by the
    # store, so a list here raised `TypeError: unhashable type` out of the
    # writer where a refusal naming the field belongs.
    "id": Rule("text"),
    "fingerprint": Rule("fingerprint"),
    "proxy": Rule("proxy", optional=True),
    "tags": Rule("texts"),
    "startup_urls": Rule("texts"),
    "extensions": Rule("texts"),
    "launch_args": Rule("texts"),
    "ip_based": Rule("texts"),
    "clear_on_start": Rule("texts"),
    "switched_off": Rule("texts"),
    # `{"name", "url"}` pairs. The dashboard converter reads `b.get("name")`
    # and `b["url"]` off each entry, so a bare string or number in the list
    # raised `AttributeError` out of the write, and a numeric `name` was
    # formatted into the stored text as though it had been one.
    "bookmarks": Rule("bookmarks"),
    "created_at": Rule("number", low=0, high=2 ** 53),
    "updated_at": OWNED("every write path stamps it with time.time() before "
                        "serialising, so the caller's value never reaches disk"),
}


TABLES = {"Profile": PROFILE, "Fingerprint": FINGERPRINT, "Proxy": PROXY}

#: The verdicts that are not a check. Listed once so the coverage test and the
#: validator agree on which entries mean "deliberately not validated".
EXEMPT = ("owned", "normalized")


def unpoliced(model) -> set:
    """Fields of `model` that no entry above mentions.

    The coverage test fails on anything this returns. A new model field is a
    new decision about what may be stored, and the only way to make it is to
    write it down here.
    """
    return {f.name for f in fields(model)} - set(TABLES[model.__name__])


def phantom(model) -> set:
    """Entries above that name no field of `model`.

    The mirror image, and the one that catches a rename: a policy left behind
    pointing at a field that no longer exists is a field now travelling
    unchecked under its new name.
    """
    return set(TABLES[model.__name__]) - {f.name for f in fields(model)}


def unexplained(model) -> set:
    """Exempt entries of `model` with no reason written down.

    A blanket exemption is the gap this table exists to prevent, so an `OWNED`
    or `NORMALIZED` field has to say why it is one.
    """
    return {name for name, rule in TABLES[model.__name__].items()
            if rule.kind in EXEMPT and not rule.why.strip()}
