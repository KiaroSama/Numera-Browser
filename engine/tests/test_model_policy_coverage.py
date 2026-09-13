"""A model field with no policy must fail the build.

This is the test the rest of N04 rests on. The validator's real defect was not
that it got any particular field wrong - it was that the list of fields it
checked was written by hand, so it could not say which of the fifty-six fields
of `Profile` + `Fingerprint` + `Proxy` it had never heard of. Twelve were
checked. `screen_width = float("inf")` went to disk as the non-JSON token
`Infinity`, `user_agent = 123` as an integer and `seed = None` over a field the
noise generator dereferences, not because anyone decided they were acceptable
but because nothing enumerated them.

Fixing those twelve-plus-forty-four by hand fixes today and nothing else: the
next field added to `Fingerprint` arrives unchecked exactly the way these did.
So the guard is the diff itself - `dataclasses.fields()` against the policy
table - and it is deliberately unhelpful about what to do next. There is no
default rule and no catch-all entry to fall into, because an entry that accepts
anything is indistinguishable from the gap it would hide. Adding a field means
saying what may be in it.

No store, no filesystem, no app: this is a diff of two sets, and it should cost
nothing to run on every change.
"""

from dataclasses import dataclass, field, fields

import pytest

from numera import field_policy, write_contract
from numera.models import Fingerprint, Profile, Proxy

MODELS = (Profile, Fingerprint, Proxy)


@pytest.mark.parametrize("model", MODELS, ids=lambda m: m.__name__)
def test_every_model_field_has_a_policy(model):
    """The requirement itself."""
    assert field_policy.unpoliced(model) == set(), (
        f"{model.__name__} has fields no policy mentions. Each one is a value "
        f"that reaches disk unchecked; add an entry to field_policy.")


@pytest.mark.parametrize("model", MODELS, ids=lambda m: m.__name__)
def test_no_policy_names_a_field_that_is_gone(model):
    """The mirror image, and the one that catches a rename.

    A policy left pointing at a field that no longer exists reads as coverage
    while the value now travelling under the new name is checked by nothing.
    """
    assert field_policy.phantom(model) == set()


@pytest.mark.parametrize("model", MODELS, ids=lambda m: m.__name__)
def test_a_field_exempt_from_checking_says_why(model):
    """`OWNED` and `NORMALIZED` are the two ways out, and both cost a sentence.

    An exemption with no reason is the blanket policy this table exists to
    prevent, wearing a different word.
    """
    assert field_policy.unexplained(model) == set()


def test_the_coverage_guard_actually_bites():
    """The guard's own regression.

    A coverage test that cannot fail is worth less than no coverage test, since
    it reads as proof. This adds a field to a stand-in named `Fingerprint` -
    which is how `unpoliced` finds the table - and asserts the diff reports it.
    """

    @dataclass
    class Fingerprint(object):            # noqa: F811 - a stand-in, on purpose
        os: str = ""
        webgpu_adapter: str = ""          # the field nobody wrote a policy for

    assert field_policy.unpoliced(Fingerprint) == {"webgpu_adapter"}


def test_every_rule_kind_in_the_table_is_one_the_validator_implements():
    """A typo in a `kind` would otherwise be a field that silently checks nothing.

    `_rule_problem` raises on an unknown kind rather than returning None, so
    this drives every entry through it once with a value of the wrong type and
    asserts the raise never happens.
    """
    for name, table in field_policy.TABLES.items():
        for field_name, rule in table.items():
            write_contract._rule_problem(rule, field_name, object(), "where",
                                         f"{name}.{field_name}")


def test_a_number_rule_resolves_to_exactly_one_set_of_bounds():
    """`cores` and `hardware_concurrency` are one limit under two names.

    They were two copies once, which is the only reason they could disagree -
    and they did, about `seed`. A rule naming a `dash` key must read that key's
    range rather than carry its own.
    """
    rule = field_policy.FINGERPRINT["hardware_concurrency"]

    assert rule.bounds() == field_policy.NUMBER_RANGES["cores"]
    assert rule.bounds() == write_contract.NUMBER_RANGES[rule.dash]


def test_the_policy_covers_the_fields_the_serialized_record_carries():
    """`to_dict()` is the shape a batch and a backup hold.

    A key it writes that no policy names would be a field checked on the object
    and unchecked on the mapping - the same rule applying to one shape only,
    which is the drift this module exists to end.
    """
    record = Profile(name="n", fingerprint=Fingerprint(
        os="windows", user_agent="UA", platform="Win32", ch_platform='"Windows"',
        chrome_version="128", screen_width=1920, screen_height=1080,
        viewport_width=1920, viewport_height=992, hardware_concurrency=8,
        device_memory=8, webgl_vendor="v", webgl_renderer="r", locale="en-US",
        timezone="UTC", languages=["en"])).to_dict()

    assert set(record) - set(field_policy.PROFILE) == set()
    assert set(record["fingerprint"]) - set(field_policy.FINGERPRINT) == set()


def test_a_dataclass_field_list_is_what_is_being_diffed():
    """Guards the guard's input: `fields()` over a real model, not a literal."""
    assert {f.name for f in fields(Proxy)} == set(field_policy.PROXY)
    assert "password" in field_policy.PROXY, \
        "the credential must be policed, and read-only - see field_policy.PROXY"
