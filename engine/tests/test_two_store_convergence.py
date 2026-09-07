"""One profile, two copies, one answer - whichever surface asks.

A profile is stored twice: `profiles/<id>.json` for the engine and
`dashboard/<id>.json` for the GUI, which has fields the engine cannot model.
Every current write moves both, so the copies only drift apart through an older
version, a hand edit or a restored backup. What 0.24 did about that was
piecemeal: a dashboard-only profile was repaired when something happened to
resolve it by id or by name, and ignored by `ProfileStore.list()` - which
globbed `profiles/` alone, so `numera list` did not show a profile the GUI was
displaying at that moment. Two copies that disagreed about a field were not
reconciled at all: `find_by_name("beta")` found the id through the dashboard's
name and then returned the engine record, still called `alpha`.

The policy these tests pin down:

* listing is the union of both stores, and a one-sided record is repaired;
* for every field both shapes carry, the dashboard record is the authority -
  it is the superset, and the launch path has always derived the engine copy
  from it;
* a field only the engine models is not a disagreement, and a repair leaves it
  alone;
* the proxy password is never replaced by a derivation. A disagreement there is
  reported and the stored credential stands.
"""

import json
import warnings

import pytest

from numera.fingerprint import generate
from numera.models import Profile, Proxy
from numera.repo import ProfileDiverged, repo
from numera.store import ProfileStore


def made(name, **kw) -> Profile:
    return Profile(name=name, fingerprint=generate(seed=kw.pop("seed", 1)), **kw)


def dash_raw(store, pid) -> dict:
    return json.loads((store.root / "dashboard" / f"{pid}.json").read_text("utf-8"))


def write_dash(store, pid, record) -> None:
    (store.root / "dashboard" / f"{pid}.json").write_text(
        json.dumps(record), encoding="utf-8")


def engine_raw(store, pid) -> dict:
    return json.loads((store.root / "profiles" / f"{pid}.json").read_text("utf-8"))


def write_engine(store, pid, record) -> None:
    (store.root / "profiles" / f"{pid}.json").write_text(
        json.dumps(record), encoding="utf-8")


@pytest.fixture
def store(tmp_path):
    return ProfileStore(tmp_path)


# --- a record on one side only ---------------------------------------------

def test_a_dashboard_only_profile_is_listed(store):
    """It is visible in the GUI, launchable through the API and resolvable by
    id from here - and it was missing from `list()` and from `numera list`."""
    prof = repo(store).create(made("alpha"))
    (store.root / "profiles" / f"{prof.id}.json").unlink()

    listed = ProfileStore(store.root).list()

    assert [p.id for p in listed] == [prof.id], "a dashboard-only profile was skipped"
    assert listed[0].name == "alpha"
    assert (store.root / "profiles" / f"{prof.id}.json").exists(), \
        "the one-sided record was listed but not repaired"


def test_an_engine_only_profile_is_still_listed(store):
    """The other side of the union: a profile the CLI made before the dashboard
    directory existed."""
    prof = repo(store).create(made("alpha"))
    (store.root / "dashboard" / f"{prof.id}.json").unlink()

    listed = ProfileStore(store.root).list()

    assert [p.id for p in listed] == [prof.id]
    assert listed[0].name == "alpha"


def test_a_dashboard_only_profile_resolves_with_its_secret_readable(store):
    """The adoption path returned the materialised object, whose proxy password
    is whatever the dashboard stores - ciphertext, under a vault. Every other
    lookup returns a profile whose secret has been through the vault boundary,
    and a caller cannot tell which kind it was handed: a launch would have
    authenticated with the ciphertext."""
    pytest.importorskip("cryptography")
    prof = repo(store).create(
        made("alpha", proxy=Proxy(server="http://h:1", username="u", password="s3cret")))
    store.enable_vault("master")
    (store.root / "profiles" / f"{prof.id}.json").unlink()

    opened = ProfileStore(store.root, password="master")
    assert opened.resolve(prof.id).proxy.password == "s3cret"
    assert opened.find_by_name("alpha").proxy.password == "s3cret"


# --- two copies that disagree ----------------------------------------------

def test_a_name_disagreement_converges_on_the_dashboard(store):
    """`profiles/A.json` says alpha, `dashboard/A.json` says beta. The name is
    a field both shapes carry, so the dashboard is the authority - and every
    surface has to agree, or `find_by_name` answers for a profile that then
    launches under a different name."""
    prof = repo(store).create(made("alpha"))
    write_dash(store, prof.id, {**dash_raw(store, prof.id), "name": "beta"})

    fresh = ProfileStore(store.root)
    assert fresh.resolve(prof.id).name == "beta", "resolve(id) kept the stale name"
    assert [p.name for p in ProfileStore(store.root).list()] == ["beta"]
    assert ProfileStore(store.root).resolve("beta").name == "beta"
    assert ProfileStore(store.root).find_by_name("alpha") is None
    assert engine_raw(store, prof.id)["name"] == "beta", "the engine copy was not repaired"


def test_a_shared_field_disagreement_converges_too(store):
    """Not just the name: the disagreement can be anywhere both shapes reach."""
    prof = repo(store).create(made("alpha", notes="old", tags=["x"]))
    write_dash(store, prof.id,
               {**dash_raw(store, prof.id), "notes": "new", "tags": ["y", "z"]})

    got = ProfileStore(store.root).resolve(prof.id)

    assert got.notes == "new"
    assert got.tags == ["y", "z"]
    assert engine_raw(store, prof.id)["notes"] == "new"


def test_a_healthy_profile_is_never_rewritten_by_a_listing(store):
    """The repair has to be for a real disagreement. Deriving the engine record
    from the dashboard shape recomputes the viewport - the one field the
    dashboard has no room for - so a version that repaired unconditionally
    rewrote every profile on every listing and changed its fingerprint doing it.
    """
    prof = repo(store).create(made("alpha"))
    before = engine_raw(store, prof.id)
    stamp = (store.root / "profiles" / f"{prof.id}.json").stat().st_mtime_ns

    ProfileStore(store.root).list()
    ProfileStore(store.root).resolve(prof.id)
    repo(ProfileStore(store.root)).materialize_current(prof.id)

    assert engine_raw(store, prof.id) == before, "a healthy profile was rewritten"
    assert (store.root / "profiles" / f"{prof.id}.json").stat().st_mtime_ns == stamp
    assert before["fingerprint"]["viewport_width"] != before["fingerprint"]["screen_width"], \
        "the fixture has no viewport inset, so this proves nothing"


def test_a_repair_does_not_reset_a_field_only_the_engine_models(store):
    """The dashboard is the authority for the fields it *has*. The viewport is
    not one of them, and a repair that reset it would change the identity the
    browser presents as a side effect of a name change."""
    prof = repo(store).create(made("alpha"))
    viewport = engine_raw(store, prof.id)["fingerprint"]["viewport_width"]
    write_dash(store, prof.id, {**dash_raw(store, prof.id), "name": "beta"})

    ProfileStore(store.root).resolve(prof.id)

    assert engine_raw(store, prof.id)["name"] == "beta", "the repair did not happen"
    assert engine_raw(store, prof.id)["fingerprint"]["viewport_width"] == viewport


# --- the one thing a derivation may not decide ------------------------------

def test_two_different_proxy_passwords_are_reported_not_resolved(store):
    """Picking a side destroys a credential the user may have nowhere else."""
    prof = repo(store).create(
        made("alpha", proxy=Proxy(server="http://h:1", username="u", password="engine")))
    raw = dash_raw(store, prof.id)
    raw["proxy"] = {**raw["proxy"], "pass": "dashboard"}
    write_dash(store, prof.id, raw)

    with pytest.raises(ProfileDiverged, match="different"):
        repo(ProfileStore(store.root)).materialize_current(prof.id)

    with warnings.catch_warnings(record=True) as seen:
        warnings.simplefilter("always")
        listed = ProfileStore(store.root).list()
    assert [p.id for p in listed] == [prof.id], "the store became unlistable"
    assert any("different proxy passwords" in str(w.message) for w in seen), \
        f"the divergence was not reported: {[str(w.message) for w in seen]}"
    assert engine_raw(store, prof.id)["proxy"]["password"] == "engine", \
        "the stored credential was overwritten by a derivation"


def test_a_locked_vault_keeps_both_ciphertexts_exactly(store):
    """Locked, neither copy can be read, so nothing may be rewritten from the
    other - and the reconciliation still has to leave the store usable."""
    pytest.importorskip("cryptography")
    prof = repo(store).create(
        made("alpha", proxy=Proxy(server="http://h:1", username="u", password="s3cret")))
    store.enable_vault("master")
    stored = engine_raw(store, prof.id)["proxy"]["password"]
    dash_secret = dash_raw(store, prof.id)["proxy"]["pass"]
    # A name disagreement as well, so there is a real repair to make.
    write_dash(store, prof.id, {**dash_raw(store, prof.id), "name": "beta"})

    locked = ProfileStore(store.root)          # no password
    with warnings.catch_warnings(record=True) as seen:
        warnings.simplefilter("always")
        assert locked.resolve(prof.id).name == "beta"
    # `vault enable` encrypts each file on its own, so the two copies of one
    # password are two *different* ciphertexts in every encrypted store. A
    # locked engine cannot tell that from a real conflict, and calling it one
    # would report every profile in the store as diverged and repair none.
    assert [str(w.message) for w in seen] == [], "a healthy encrypted store was flagged"

    assert engine_raw(store, prof.id)["proxy"]["password"] == stored, \
        "a locked repair touched the ciphertext"
    assert dash_raw(store, prof.id)["proxy"]["pass"] == dash_secret
    assert ProfileStore(store.root, password="master").get(prof.id).proxy.password == "s3cret"


def test_the_same_secret_encrypted_twice_is_not_a_divergence(store):
    """Every encryption uses a fresh nonce, so one password encrypted twice
    gives two different strings. Comparing them as text would call a healthy
    profile diverged and refuse to repair anything else about it."""
    pytest.importorskip("cryptography")
    prof = repo(store).create(
        made("alpha", proxy=Proxy(server="http://h:1", username="u", password="s3cret")))
    store.enable_vault("master")
    opened = ProfileStore(store.root, password="master")
    from numera import crypto

    raw = dash_raw(store, prof.id)
    raw["proxy"] = {**raw["proxy"],
                    "pass": crypto.encrypt("master", opened.vault()["salt"], "s3cret")}
    raw["name"] = "beta"
    write_dash(store, prof.id, raw)
    assert raw["proxy"]["pass"] != engine_raw(store, prof.id)["proxy"]["password"]

    assert opened.resolve(prof.id).name == "beta", "a re-encryption blocked the repair"
    assert ProfileStore(store.root, password="master").get(prof.id).proxy.password == "s3cret"


# --- the API and the launch path agree with all of the above ----------------

def test_the_api_listing_shows_the_reconciled_profile(store):
    pytest.importorskip("fastapi")
    from fastapi.testclient import TestClient

    from numera.server import create_app

    prof = repo(store).create(made("alpha"))
    write_dash(store, prof.id, {**dash_raw(store, prof.id), "name": "beta"})

    client = TestClient(create_app(data_dir=str(store.root), start_scheduler=False))
    rows = client.get("/api/profiles").json()

    assert [r["name"] for r in rows] == ["beta"]
    assert engine_raw(store, prof.id)["name"] == "beta", \
        "the API listing left the two copies disagreeing"
