"""Flat numbers obey the model's rule, and a refusal can always be written.

Two defects at the flat (dashboard-vocabulary) write door:

* **Range-only numbers.** The flat patch checked `cores`, `seed`, the media
  counts and `maxTouchPoints` for a range and nothing else, so `cores = 1.5`
  was accepted - and then `convert.to_engine_profile` cast it with `int()` and
  stored `1`, a value the caller never sent. `memory = 3.5` passed the same
  way although the complete model refuses it. The complete model already knew
  the rule; the flat door had its own, thinner copy (two, counting the API's).
* **An error message that raised.** CPython will not print an integer longer
  than 4300 digits, so quoting `10**5000` back in a refusal raised `ValueError`
  from inside the validator, and the caller got a conversion crash instead of
  `WriteRefused`.
"""

import math

import pytest

from numera import convert, field_policy, write_contract
from numera.server import convert as http_convert
from numera.write_contract import WriteRefused

COUNTS = ["cores", "mediaVideo", "mediaAudio", "mediaSpeakers",
          "maxTouchPoints", "seed"]


def problems(**fields):
    return write_contract.profile_problems({"name": "n", **fields})


def test_every_flat_number_has_exactly_one_model_rule():
    by_dash = {}
    for name, rule in field_policy.FINGERPRINT.items():
        if rule.dash:
            by_dash.setdefault(rule.dash, []).append(name)
    assert set(by_dash) == set(field_policy.NUMBER_RANGES)
    assert all(len(names) == 1 for names in by_dash.values()), by_dash
    for dash, (name,) in by_dash.items():
        assert write_contract.FLAT_NUMBER_RULES[dash] is field_policy.FINGERPRINT[name]


@pytest.mark.parametrize("field", COUNTS)
@pytest.mark.parametrize("value", [1.5, 2.25])
def test_a_fractional_count_is_refused_before_conversion(field, value):
    found = problems(**{field: value})
    assert found and "whole number" in found[0] and repr(field) in found[0], \
        f"{field}={value} was accepted and would be truncated to {int(value)}"


def test_memory_refuses_a_fraction_no_device_reports():
    assert problems(memory=3.5), "memory=3.5 passed the flat contract"


@pytest.mark.parametrize("fields", [
    # Not `seed: 0` - the converter reads 0 as "no seed" and derives one.
    {"cores": 8}, {"cores": 2.0}, {"seed": 7}, {"seed": 2 ** 53},
    {"memory": 0.25}, {"memory": 0.5}, {"memory": 16},
    {"devicePixelRatio": 1.5}, {"devicePixelRatio": 2.75},
    {"mediaVideo": 0}, {"maxTouchPoints": 10},
])
def test_legal_values_still_pass_and_survive_conversion(fields):
    assert problems(**fields) == []
    (key, sent), = fields.items()
    fp = convert.to_engine_profile({"name": "n", "os": "windows", **fields}).fingerprint
    stored = {"cores": fp.hardware_concurrency, "seed": fp.seed,
              "memory": fp.device_memory, "devicePixelRatio": fp.device_pixel_ratio,
              "mediaVideo": fp.cameras, "maxTouchPoints": fp.max_touch_points}[key]
    assert stored == sent, f"{key}={sent!r} was stored as {stored!r}"


@pytest.mark.parametrize("value", [True, "8", math.inf, math.nan, -1, 10 ** 6])
def test_non_numbers_and_out_of_range_stay_refused(value):
    assert problems(cores=value), f"cores={value!r} was accepted"


def test_omitted_and_null_still_mean_not_changing():
    assert problems() == []
    assert problems(**{f: None for f in COUNTS + ["memory", "devicePixelRatio"]}) == []


@pytest.mark.parametrize("fields", [
    {"cores": 10 ** 5000}, {"seed": -(10 ** 5000)}, {"name": 10 ** 5000},
])
def test_an_unprintable_integer_is_refused_not_crashed(fields):
    with pytest.raises(WriteRefused) as caught:
        write_contract.require({"name": "n", **fields})
    assert "bits" in str(caught.value)


def test_a_secret_is_still_not_quoted_back():
    with pytest.raises(WriteRefused) as caught:
        write_contract.require({"name": "n", "proxy": {"host": "h", "pass": 10 ** 5000}})
    assert "bits" not in str(caught.value) and "1000" not in str(caught.value)


# ---- the conversion door ------------------------------------------------
# The server conversion is a writer too: `DashboardStore.save` converts before
# anything else checks, and the core cast turned `1.5` into `1` in the engine
# copy only. Every HTTP route and the store are exercised end to end, both
# copies compared, in `test_dashboard_numeric_boundaries.py` (from PR #22).

# A fractional pixel ratio is legal, so 1.25 is not a bad value for that one.
CONVERSION_CASES = [(field, bad) for field in write_contract.NUMBER_RANGES
                    for bad in (1.25, True, "8", math.nan, math.inf, -1)
                    if not (field == "devicePixelRatio" and bad == 1.25)]


@pytest.mark.parametrize("field,bad", CONVERSION_CASES)
def test_the_server_conversion_refuses_a_number_it_would_change(field, bad):
    record = {"name": "n", field: bad}
    with pytest.raises(WriteRefused, match=field):
        http_convert.to_engine_profile(record)
    assert record == {"name": "n", field: bad}, "the caller's input was mutated"


def test_the_core_conversion_stays_tolerant_for_legacy_readers():
    """Stored records written before this check must still load."""
    fp = convert.to_engine_profile({"name": "legacy", "cores": "8", "memory": None,
                                   "mediaVideo": math.nan}).fingerprint
    assert (fp.hardware_concurrency, fp.device_memory, fp.cameras) == (8, 16, 1)
