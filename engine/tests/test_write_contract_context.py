"""Which validator runs, and what a number survives being stored.

Two defects with one shape: a value decided something it was also the subject
of.

* **The context came from the data.** `shape()` asked whether `fingerprint`
  was non-null, so a complete engine record whose fingerprint was `null` - or
  missing - answered "dashboard patch" and reached the *partial* validator,
  which exists to allow exactly that omission. A required field being absent
  cannot also be what decides whether absence is allowed.
* **The conversion decided the value.** `device_memory` was cast with `int`,
  and `int(0.25)` is `0`. A dashboard reporting a quarter-gigabyte device
  stored zero, passed every later guard because zero is a legal amount, and
  round-tripped as zero - a value the caller never sent, written without a
  word.

`navigator.deviceMemory` is typed `double` in the Device Memory draft and real
browsers report 0.25 and 0.5 on small devices, so those are values to preserve
rather than reject. The bounds stay the product's own; what changed is that a
number is no longer quietly replaced by a different number.
"""

import json

import pytest

from numera import convert, field_policy, write_contract
from numera.models import Fingerprint, Profile
from numera.store import ProfileStore


# ---------------------------------------------------------------------------
# A. The validation context is not chosen by the data under test
# ---------------------------------------------------------------------------

@pytest.mark.parametrize("record,why", [
    ({"id": "p1", "name": "one", "fingerprint": None}, "a null fingerprint"),
    ({"id": "p1", "name": "one", "fingerprint": 123}, "a numeric fingerprint"),
    ({"id": "p1", "name": "one", "fingerprint": "text"}, "a text fingerprint"),
    ({"id": "p1", "name": "one", "fingerprint": []}, "a list fingerprint"),
])
def test_a_malformed_engine_record_stays_a_complete_record(record, why):
    """It is still an engine record when its fingerprint is wrong.

    `fingerprint is not None` routed every one of these to the partial
    validator, which has nothing to say about a field a patch is allowed to
    omit. The key is present, so the shape is not in doubt.
    """
    assert write_contract.shape(record) == write_contract.ENGINE_RECORD, why
    assert write_contract.profile_problems(record), \
        f"{why} was accepted by the partial validator"


def test_an_entry_point_may_declare_the_shape_outright():
    """The case the contents genuinely cannot settle.

    A record with no `fingerprint` key at all looks exactly like an ordinary
    patch. A caller holding a batch entry or a backup knows which it is, and
    says so rather than hoping the guess lands.
    """
    headless = {"id": "p1", "name": "one"}

    assert write_contract.shape(headless) == write_contract.DASHBOARD_PATCH
    assert write_contract.profile_problems(headless) == [], \
        "as a patch this states nothing wrong"

    assert write_contract.profile_problems(
        headless, kind=write_contract.ENGINE_RECORD), \
        "declared a complete record, the missing fingerprint must be refused"


def test_an_unknown_declared_shape_is_refused_not_guessed():
    with pytest.raises(ValueError, match="unknown profile shape"):
        write_contract.shape({"name": "n"}, kind="something-else")


def test_an_ordinary_patch_is_still_a_patch():
    """The control. A partial dashboard payload must keep its own rules, or
    every `PUT` that changes one field starts demanding all of them."""
    assert write_contract.shape({"name": "renamed"}) == \
        write_contract.DASHBOARD_PATCH
    assert write_contract.profile_problems({"name": "renamed"}) == []


def test_a_profile_object_is_unaffected():
    """The third shape, which never depended on the guess."""
    p = convert.to_engine_profile({"name": "one", "os": "windows"})
    assert write_contract.shape(p) == write_contract.ENGINE_OBJECT
    assert write_contract.profile_problems(p) == []


# ---------------------------------------------------------------------------
# B. A number is stored as the number that was sent
# ---------------------------------------------------------------------------

@pytest.mark.parametrize("sent", [0.25, 0.5])
def test_a_fractional_device_memory_survives_conversion(sent):
    """`int(0.25)` is 0, and 0 is a legal amount - so nothing downstream
    noticed that the value had been replaced."""
    fp = convert.to_engine_profile({"name": "n", "os": "windows",
                                   "memory": sent}).fingerprint

    assert fp.device_memory == sent, \
        f"{sent} was silently converted to {fp.device_memory}"


@pytest.mark.parametrize("sent,expect", [(16, 16), ("8", 8), (4.0, 4)])
def test_a_whole_device_memory_stays_a_whole_number(sent, expect):
    """The control, and the reason `_exact` exists rather than `float`:
    nothing that wrote `16` may start writing `16.0`."""
    fp = convert.to_engine_profile({"name": "n", "os": "windows",
                                   "memory": sent}).fingerprint

    assert fp.device_memory == expect
    assert isinstance(fp.device_memory, int), \
        f"a whole value came back as {type(fp.device_memory).__name__}"


@pytest.mark.parametrize("sent", [0.25, 0.5])
def test_a_fractional_device_memory_round_trips_through_a_real_store(
        tmp_path, sent):
    """The production boundary: saved by the real store, read back off disk.

    Conversion preserving the value is worth nothing if the complete-record
    guard then refuses it or the serializer drops it.
    """
    store = ProfileStore(tmp_path)
    profile = convert.to_engine_profile(
        {"name": "small-device", "os": "windows", "memory": sent})
    store.save(profile)

    stored = json.loads(
        next((tmp_path / "profiles").glob("*.json")).read_text(encoding="utf-8"))
    assert stored["fingerprint"]["device_memory"] == sent, \
        "the stored bytes do not carry the value that was sent"
    assert store.list()[0].fingerprint.device_memory == sent, \
        "the value did not survive being read back"


@pytest.mark.parametrize("sent", [0.25, 0.5, 16])
def test_the_dashboard_guard_and_the_complete_guard_agree(sent):
    """They disagreed, which is what let the truncation through unnoticed:
    the flat guard accepted `.25`, the core policy demanded a whole number,
    and the converter resolved the argument by discarding the value."""
    assert write_contract.profile_problems({"name": "n", "memory": sent}) == [], \
        "the dashboard guard refused a value the API accepts"

    fp = convert.to_engine_profile({"name": "n", "os": "windows",
                                   "memory": sent}).fingerprint
    assert write_contract.profile_problems(
        Profile(name="n", fingerprint=fp)) == [], \
        "the complete guard refused what the dashboard guard allowed"


def test_device_memory_allows_exactly_the_reported_fractions():
    """Whole, plus the two sub-integer values the API actually reports.

    Not an open number: `3.5` is not a reading any browser produces, and
    `test_write_contract_complete_record.py` still refuses it. The list is the
    compatibility policy, written down where the checker reads it.
    """
    rule = field_policy.FINGERPRINT["device_memory"]
    assert rule.kind == "int"
    assert rule.fractions == (0.25, 0.5)


@pytest.mark.parametrize("field", ["hardware_concurrency", "screen_width",
                                   "cameras", "max_touch_points"])
def test_counts_are_still_whole(field):
    """The control that keeps the fix narrow. A ratio may be fractional; a
    number of cores, pixels or cameras may not."""
    assert field_policy.FINGERPRINT[field].kind == "int", \
        f"{field} stopped being a whole number along with device_memory"


@pytest.mark.parametrize("bad", [float("inf"), float("nan"), "lots", None])
def test_an_unusable_memory_still_falls_back(bad):
    """Preserving a real fraction is not accepting anything at all."""
    fp = convert.to_engine_profile({"name": "n", "os": "windows",
                                   "memory": bad}).fingerprint

    assert fp.device_memory == 16, f"{bad!r} produced {fp.device_memory!r}"
