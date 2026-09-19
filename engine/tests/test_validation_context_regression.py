"""Explicit input context is a value, and malformed records are domain errors."""
import copy
import json
from collections import UserDict

import pytest

from numera.convert import to_engine_profile, to_dashboard_profile
from numera.write_contract import (
    DASHBOARD_PATCH, ENGINE_OBJECT, ENGINE_RECORD, WriteRefused,
    profile_problems, require,
)


def complete_profile():
    return to_engine_profile({"id": "context-test", "name": "context", "memory": 0.5})


@pytest.mark.parametrize("context", [ENGINE_RECORD, DASHBOARD_PATCH])
def test_context_deserialized_from_json_has_the_same_semantics(context):
    decoded = json.loads(json.dumps(context))
    assert decoded == context and decoded is not context
    value = complete_profile().to_dict() if context == ENGINE_RECORD else {"notes": "ok"}
    assert profile_problems(value, kind=context) == []
    assert profile_problems(value, kind=decoded) == []
    require(value, kind=decoded)


@pytest.mark.parametrize("context", [ENGINE_RECORD, DASHBOARD_PATCH, ENGINE_OBJECT])
@pytest.mark.parametrize("value", [None, 17, "not a record", [], [1]])
@pytest.mark.parametrize("decoded", [False, True])
def test_wrong_declared_container_is_a_write_refusal(context, value, decoded):
    with pytest.raises(WriteRefused, match="profile"):
        require(value, kind=json.loads(json.dumps(context)) if decoded else context)


@pytest.mark.parametrize("bad", [None, 17, [], "fingerprint"])
def test_complete_mappings_cannot_become_partial_patches(bad):
    record = complete_profile().to_dict()
    record["fingerprint"] = bad
    before = copy.deepcopy(record)
    with pytest.raises(WriteRefused, match="fingerprint"):
        require(record, kind=ENGINE_RECORD)
    assert record == before


def test_missing_fingerprint_is_refused_in_complete_context():
    record = complete_profile().to_dict()
    del record["fingerprint"]
    with pytest.raises(WriteRefused, match="fingerprint"):
        require(record, kind=ENGINE_RECORD)


def test_mapping_subclasses_are_supported():
    require(UserDict(complete_profile().to_dict()), kind=ENGINE_RECORD)
    require(UserDict({"notes": "ok"}), kind=DASHBOARD_PATCH)


@pytest.mark.parametrize("memory", [0.25, 0.5, 1, 16])
def test_fractional_compatibility_is_preserved(memory):
    profile = to_engine_profile({"name": "memory", "memory": memory})
    require(profile, kind=ENGINE_OBJECT)
    require(profile.to_dict(), kind=ENGINE_RECORD)
    restored = to_engine_profile(to_dashboard_profile(profile))
    assert restored.fingerprint.device_memory == memory


def test_patch_omission_is_not_a_complete_record_omission():
    require({}, kind=DASHBOARD_PATCH)
    with pytest.raises(WriteRefused, match="fingerprint"):
        require({}, kind=ENGINE_RECORD)


def test_invalid_context_still_rejects_programmer_mistakes():
    with pytest.raises(ValueError, match="unknown profile shape"):
        require({}, kind="arbitrary")
