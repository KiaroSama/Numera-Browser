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


# ---- the API door ---------------------------------------------------------
# `routes_profiles` kept its own ranges-only copy of the same check, so the
# HTTP write path truncated `cores=1.5` even with the contract fixed.

@pytest.fixture
def api(tmp_path, monkeypatch):
    pytest.importorskip("fastapi")
    pytest.importorskip("httpx")
    from fastapi.testclient import TestClient
    from numera.server import create_app
    monkeypatch.delenv("NUMERA_API_TOKEN", raising=False)
    with TestClient(create_app(str(tmp_path))) as client:
        yield client


def test_api_create_and_batch_refuse_a_fractional_count(api):
    created = api.post("/api/profiles", json={"name": "a", "cores": 1.5})
    assert created.status_code == 422 and "whole number" in created.text
    batch = api.post("/api/profiles/batch", json={"profiles": [
        {"name": "ok", "cores": 4}, {"name": "bad", "mediaVideo": 1.5}]})
    assert batch.status_code == 400 and "whole number" in batch.text
    assert api.get("/api/profiles").json() == [], "a refused write left a profile"


def test_api_update_refuses_and_keeps_the_stored_value(api):
    pid = api.post("/api/profiles", json={"name": "a", "cores": 4}).json()["id"]
    assert api.put(f"/api/profiles/{pid}", json={"cores": 2.5}).status_code == 422
    stored, = [p for p in api.get("/api/profiles").json() if p["id"] == pid]
    assert stored["cores"] == 4
    assert api.put(f"/api/profiles/{pid}", json={"memory": 0.5}).status_code == 200
