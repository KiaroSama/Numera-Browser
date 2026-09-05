"""A zone reported under its other name is not a contradiction.

The tz database keeps an old id as a link when a place is renamed, and CLDR -
which is what Chromium answers `Intl.DateTimeFormat().resolvedOptions().timeZone`
from - did not follow every rename. A profile set to `Asia/Kolkata` is reported
by the browser as `Asia/Calcutta`, so the trust report compared two names for one
zone and called it a mismatch. A real Chrome on a machine set to Kolkata says
`Asia/Calcutta` too: the profile was coherent and the report was wrong.
"""
import pytest

from numera.devices import TZ_ALIASES, same_timezone


@pytest.mark.parametrize("a, b", [
    ("Asia/Kolkata", "Asia/Calcutta"),
    ("Asia/Calcutta", "Asia/Kolkata"),          # either name may be the stored one
    ("Europe/Kyiv", "Europe/Kiev"),
    ("America/Nuuk", "America/Godthab"),
    ("Asia/Ho_Chi_Minh", "Asia/Saigon"),
    ("Pacific/Chuuk", "Pacific/Truk"),
    ("America/Argentina/Buenos_Aires", "America/Buenos_Aires"),
])
def test_two_names_for_one_zone_match(a, b):
    assert same_timezone(a, b)


def test_the_same_name_matches_itself():
    assert same_timezone("Europe/Berlin", "Europe/Berlin")


def test_case_does_not_decide_it():
    assert same_timezone("asia/kolkata", "Asia/Calcutta")
    assert same_timezone("  Asia/Kolkata  ", "Asia/Calcutta")


@pytest.mark.parametrize("a, b", [
    # Same offsets all year and even the same transition dates - and still two
    # different zones, which a fingerprinter can tell apart by the string alone.
    # An offset comparison would call these equal and hide a real contradiction.
    ("Europe/London", "Europe/Lisbon"),
    ("Europe/Berlin", "Europe/Paris"),
    ("Asia/Kolkata", "Asia/Colombo"),           # both +05:30, different zones
    ("Europe/London", "Asia/Kolkata"),
])
def test_different_zones_do_not_match(a, b):
    assert not same_timezone(a, b)


@pytest.mark.parametrize("a, b", [
    ("", "Asia/Kolkata"), ("Asia/Kolkata", ""), (None, "Asia/Kolkata"),
    ("Asia/Kolkata", None), (None, None),
])
def test_a_missing_name_is_never_a_match(a, b):
    """Nothing is known about a profile with no timezone, and "unknown equals
    unknown" would report a check as passing that was never made."""
    assert not same_timezone(a, b)


def test_the_table_is_symmetric():
    """Both directions are registered, so which name the profile stores and
    which the browser reports cannot change the answer."""
    for name, other in TZ_ALIASES.items():
        assert other.lower() in TZ_ALIASES, f"{other} has no reverse entry"
        assert TZ_ALIASES[other.lower()].lower() == name


def test_the_probe_compares_with_the_alias_rule():
    """Both timezone checks in the trust/leak reports, not just one of them -
    the exit-location check compared strings the same way."""
    import pathlib

    from numera import probe

    # The whole package, not `inspect.getsource(probe)` - that returns only
    # `__init__.py`, and the two comparisons live in `network.py` and
    # `trust.py`. Reading one file would have passed this vacuously.
    src = "\n".join(p.read_text(encoding="utf-8")
                    for p in sorted(pathlib.Path(probe.__path__[0]).glob("*.py")))
    assert 'info["timezone"] == fp.timezone' not in src
    assert 'seen["timezone"] == fp.timezone' not in src
    assert src.count("_same_timezone(") >= 2
