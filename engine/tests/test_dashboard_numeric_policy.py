"""Numbers must satisfy the same policy before and after shape conversion."""
import copy

import pytest
from numera import convert, field_policy, write_contract as contract
from numera.server import convert as http_convert

COUNTS = ["cores", "mediaVideo", "mediaAudio", "mediaSpeakers", "maxTouchPoints", "seed"]


@pytest.mark.parametrize("field", COUNTS + ["memory"])
@pytest.mark.parametrize("surface", ["patch", "http-conversion"])
def test_fractional_counts_are_refused_before_lossy_conversion(field, surface):
    value = 3.5 if field == "memory" else 1.5
    record = {"name": "numeric-case", field: value}
    before = copy.deepcopy(record)
    with pytest.raises(contract.WriteRefused, match=field):
        if surface == "patch":
            contract.require(record, kind=contract.DASHBOARD_PATCH)
        else:
            http_convert.to_engine_profile(record)
    assert record == before


@pytest.mark.parametrize("field", list(contract.NUMBER_RANGES))
@pytest.mark.parametrize("bad", [True, "8", float("nan"), float("inf"), -1])
def test_http_conversion_refuses_invalid_numbers_by_field(field, bad):
    with pytest.raises(contract.WriteRefused, match=field):
        http_convert.to_engine_profile({"name": "bad-number", field: bad})


@pytest.mark.parametrize("memory", [.25, .5, 0, 1, 8, 16, 1024])
def test_memory_and_fractional_display_ratio_round_trip(memory):
    record = {"name": "exact-number", "id": "exact", "memory": memory,
              "devicePixelRatio": 1.5, "cores": 8.0}
    contract.require(record)
    converted = http_convert.to_engine_profile(record)
    contract.require(converted)
    restored = http_convert.to_dashboard_profile(converted)
    assert restored["memory"] == memory and restored["devicePixelRatio"] == 1.5
    assert restored["cores"] == 8


@pytest.mark.parametrize("field", list(contract.NUMBER_RANGES))
def test_omitted_and_null_numbers_keep_existing_defaulting(field):
    a = http_convert.to_engine_profile({"name": "defaulted"})
    b = http_convert.to_engine_profile({"name": "defaulted", field: None})
    assert a.fingerprint.to_dict() == b.fingerprint.to_dict()


@pytest.mark.parametrize("field", list(contract.NUMBER_RANGES))
def test_extreme_python_integer_has_a_bounded_domain_error(field):
    with pytest.raises(contract.WriteRefused) as caught:
        contract.require({"name": "huge", field: 10 ** 5000})
    assert field in str(caught.value) and len(str(caught.value)) < 300


def test_every_numeric_alias_has_exactly_one_model_rule():
    aliases = [r.dash for r in field_policy.FINGERPRINT.values()
               if r.kind in ("int", "number") and r.dash]
    assert len(aliases) == len(set(aliases))
    assert set(aliases) == set(contract.NUMBER_RANGES)


def test_tolerant_core_conversion_is_not_changed_for_legacy_readers():
    profile = convert.to_engine_profile({"name": "legacy", "cores": "8",
                                       "memory": None, "mediaVideo": float("nan")})
    assert profile.fingerprint.hardware_concurrency == 8
    assert profile.fingerprint.device_memory == 16
    assert profile.fingerprint.cameras == 1
