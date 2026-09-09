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


def api(store):
    """A TestClient on this data root.

    The conflict tests used to call `DashboardStore.patch()` directly, which
    proves the store refuses and says nothing about what a client is told - and
    "the store refused" reached HTTP as a 500 for a while precisely because
    nothing tested the route.
    """
    pytest.importorskip("fastapi")
    from fastapi.testclient import TestClient

    from numera.server import create_app

    return TestClient(create_app(data_dir=str(store.root), start_scheduler=False))


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


# --- the divergence gate every opener shares -------------------------------

def diverged(store, name="alpha"):
    """A profile whose two copies hold different readable proxy passwords."""
    prof = repo(store).create(
        made(name, proxy=Proxy(server="http://h:1", username="u", password="engine")))
    raw = dash_raw(store, prof.id)
    raw["proxy"] = {**raw["proxy"], "pass": "dashboard"}
    write_dash(store, prof.id, raw)
    return prof


def test_a_direct_launch_is_refused_before_the_browser_starts(store):
    """The API refused this and a direct `numera launch` did not, because only
    the API called `materialize_current`. `open_session` is the gate every
    opener shares, so the refusal is now the same one everywhere."""
    from numera.drivers import open_session

    prof = diverged(store)
    fresh = ProfileStore(store.root)
    with pytest.raises(ProfileDiverged):
        with open_session(fresh.get(prof.id), fresh):
            pytest.fail("a browser was opened on an unresolved credential")


def test_the_refusal_leaves_no_claim_behind(store):
    """A gate that refuses must not leave the profile marked as owned."""
    from numera import sessions
    from numera.drivers import open_session

    prof = diverged(store)
    fresh = ProfileStore(store.root)
    with pytest.raises(ProfileDiverged):
        with open_session(fresh.get(prof.id), fresh):
            pass                                             # pragma: no cover
    assert sessions.busy(store.root, prof.id) is False, "the profile stayed claimed"


def test_an_ordinary_edit_cannot_settle_the_conflict(store):
    """The conflict Numera said it would not choose between was being settled
    by the next unrelated keystroke: `resolve` handed back the engine copy,
    `edit` wrote that whole object through both files, and the dashboard
    password was gone for good."""
    prof = diverged(store)
    before = dash_raw(store, prof.id)["proxy"]["pass"]

    with pytest.raises(ProfileDiverged):
        repo(ProfileStore(store.root)).edit(
            prof.id, lambda p: setattr(p, "notes", "an unrelated note") or p)

    assert dash_raw(store, prof.id)["proxy"]["pass"] == before, \
        "an unrelated edit overwrote the other copy's credential"
    assert engine_raw(store, prof.id)["proxy"]["password"] == "engine"
    assert ProfileStore(store.root).get(prof.id).notes == "", "the edit went through"


def test_a_dashboard_edit_cannot_settle_it_either(store):
    """Same rule from the other surface, and through the real route: a PUT that
    changes a note says nothing about the password, however much of the record
    it happens to echo back."""
    client = api(store)
    prof = diverged(store)

    answer = client.put(f"/api/profiles/{prof.id}", json={"notes": "hello"})

    assert answer.status_code == 409, answer.text
    assert dash_raw(store, prof.id)["proxy"]["pass"] == "dashboard"
    assert engine_raw(store, prof.id)["proxy"]["password"] == "engine"
    assert ProfileStore(store.root).get(prof.id).notes == ""


def test_replacing_the_password_resolves_it(store):
    """The way out. An edit that is *about* the credential is a decision, and
    it converges both copies."""
    prof = diverged(store)

    def replace(p):
        p.proxy.password = "chosen"
        return p

    repo(ProfileStore(store.root)).edit(prof.id, replace)

    assert engine_raw(store, prof.id)["proxy"]["password"] == "chosen"
    assert dash_raw(store, prof.id)["proxy"]["pass"] == "chosen"
    assert repo(ProfileStore(store.root)).divergence(prof.id) == []


def test_removing_the_proxy_resolves_it_too(store):
    prof = diverged(store)

    repo(ProfileStore(store.root)).edit(
        prof.id, lambda p: setattr(p, "proxy", None) or p)

    assert engine_raw(store, prof.id).get("proxy") is None
    assert dash_raw(store, prof.id).get("proxy") in (None, {})
    assert repo(ProfileStore(store.root)).divergence(prof.id) == []


def test_a_name_lookup_never_answers_with_a_different_name(store):
    """`find_by_name` reaches an id *through* the other copy's name. With the
    repair refused, returning the engine record answered a question nobody
    asked: `resolve("beta")` handing back a profile called `alpha`."""
    prof = diverged(store)
    raw = dash_raw(store, prof.id)
    raw["name"] = "beta"
    write_dash(store, prof.id, raw)

    fresh = ProfileStore(store.root)
    with pytest.raises(ProfileDiverged):
        fresh.resolve("beta")
    assert fresh.resolve(prof.id).name == "alpha", "an id lookup should still answer"


# --- the other one-sided record --------------------------------------------

def test_an_engine_only_profile_gets_its_twin(store):
    """Listing it was half the invariant. Leaving `dashboard/<id>.json` missing
    made "two copies" depend on somebody opening the GUI, whose listing adopts
    the record - a CLI-only machine never did."""
    prof = repo(store).create(made("alpha", notes="kept", tags=["t"]))
    engine_before = engine_raw(store, prof.id)
    (store.root / "dashboard" / f"{prof.id}.json").unlink()

    listed = ProfileStore(store.root).list()

    assert [p.id for p in listed] == [prof.id]
    twin = dash_raw(store, prof.id)
    assert twin["id"] == prof.id and twin["name"] == "alpha"
    assert twin["notes"] == "kept" and twin["tags"] == ["t"]
    assert engine_raw(store, prof.id) == engine_before, \
        "the healthy engine record was rewritten to make the twin"
    assert repo(ProfileStore(store.root)).divergence(prof.id) == []


def test_adopting_an_engine_record_keeps_its_secret_encrypted(store):
    """A locked vault must be no obstacle to making the twin, and the twin must
    not be where the ciphertext turns back into a password."""
    pytest.importorskip("cryptography")
    prof = repo(store).create(
        made("alpha", proxy=Proxy(server="http://h:1", username="u", password="s3cret")))
    store.enable_vault("master")
    stored = engine_raw(store, prof.id)["proxy"]["password"]
    (store.root / "dashboard" / f"{prof.id}.json").unlink()

    ProfileStore(store.root).list()          # locked: no password given

    assert dash_raw(store, prof.id)["proxy"]["pass"] == stored, \
        "the twin was written with something other than the stored ciphertext"
    assert ProfileStore(store.root, password="master").get(prof.id).proxy.password == "s3cret"


def test_resolving_an_engine_only_profile_repairs_it_too(store):
    prof = repo(store).create(made("alpha"))
    (store.root / "dashboard" / f"{prof.id}.json").unlink()

    assert ProfileStore(store.root).resolve("alpha").id == prof.id
    assert (store.root / "dashboard" / f"{prof.id}.json").exists()


# --- write intent: withheld, replace, clear --------------------------------

def one_sided(store, engine_secret=None, dash_secret=None):
    """A profile whose two copies disagree because one of them holds nothing."""
    prof = repo(store).create(
        made("alpha", proxy=Proxy(server="http://h:1", username="u",
                                  password=engine_secret or "engine")))
    raw = dash_raw(store, prof.id)
    raw["proxy"] = {**raw["proxy"], "pass": dash_secret or ""}
    write_dash(store, prof.id, raw)
    if engine_secret is None:
        record = engine_raw(store, prof.id)
        record["proxy"]["password"] = ""
        write_engine(store, prof.id, record)
    return prof


def test_a_secret_the_dashboard_has_lost_is_not_erased_by_an_unrelated_edit(store):
    """`_secret_disagrees` answered "no disagreement" when one side was empty,
    because that is true for a *derivation* - which keeps the stored one. For a
    write it is false: the empty side is what gets mirrored, and the working
    credential is gone."""
    prof = one_sided(store, "working-secret", "")
    before = (engine_raw(store, prof.id)["proxy"]["password"],
              dash_raw(store, prof.id)["proxy"]["pass"])

    with pytest.raises(ProfileDiverged):
        repo(ProfileStore(store.root)).edit(
            prof.id, lambda p: setattr(p, "notes", "unrelated") or p)

    assert (engine_raw(store, prof.id)["proxy"]["password"],
            dash_raw(store, prof.id)["proxy"]["pass"]) == before, \
        "an unrelated edit changed a credential"


def test_a_secret_only_the_dashboard_has_is_propagated_not_refused(store):
    """The other direction, and it is deliberately not symmetrical.

    The dashboard record is the authority, so an engine copy that lost its
    password gets it back from the copy that still has it - nothing is
    destroyed and nothing is chosen between. Refusing here would strand a
    profile whose only credential is in the authoritative copy, and demand the
    user retype a password Numera is holding.

    Erasing is the direction that must be refused, and the test above is that.
    """
    prof = one_sided(store, None, "working-secret")

    repo(ProfileStore(store.root)).edit(
        prof.id, lambda p: setattr(p, "notes", "unrelated") or p)

    assert engine_raw(store, prof.id)["proxy"]["password"] == "working-secret",         "the only credential there was did not reach the engine copy"
    assert dash_raw(store, prof.id)["proxy"]["pass"] == "working-secret"
    assert ProfileStore(store.root).get(prof.id).notes == "unrelated"


def test_a_dashboard_edit_cannot_settle_a_one_sided_secret_either(store):
    client = api(store)
    prof = one_sided(store, "working-secret", "")

    answer = client.put(f"/api/profiles/{prof.id}", json={"notes": "hello"})

    assert answer.status_code == 409, answer.text
    assert engine_raw(store, prof.id)["proxy"]["password"] == "working-secret"


def test_a_locked_edit_leaves_each_ciphertext_exactly_as_it_found_it(store):
    """Two ciphertexts under a locked vault may be one password encrypted twice
    or two passwords. This process cannot tell, so it decides nothing: the
    non-secret edit goes through and neither blob moves."""
    pytest.importorskip("cryptography")
    from numera import crypto

    prof = repo(store).create(
        made("alpha", proxy=Proxy(server="http://h:1", username="u", password="first")))
    store.enable_vault("master")
    opened = ProfileStore(store.root, password="master")
    raw = dash_raw(store, prof.id)
    raw["proxy"] = {**raw["proxy"],
                    "pass": crypto.encrypt("master", opened.vault()["salt"], "second")}
    write_dash(store, prof.id, raw)
    before = (engine_raw(store, prof.id)["proxy"]["password"],
              dash_raw(store, prof.id)["proxy"]["pass"])

    locked = ProfileStore(store.root)                 # no password
    repo(locked).edit(prof.id, lambda p: setattr(p, "notes", "while locked") or p)

    assert (engine_raw(store, prof.id)["proxy"]["password"],
            dash_raw(store, prof.id)["proxy"]["pass"]) == before, \
        "a locked edit normalised the two copies to one blob"
    assert ProfileStore(store.root).get(prof.id).notes == "while locked"

    # Unlocked, the two are still distinct and the conflict is reported.
    unlocked = ProfileStore(store.root, password="master")
    assert repo(unlocked).divergence(prof.id) == ["proxy.password"]
    with pytest.raises(ProfileDiverged):
        repo(unlocked).edit(prof.id, lambda p: setattr(p, "notes", "now") or p)


def test_a_password_can_be_cleared_while_the_proxy_stays(store):
    """`encrypt_for_write` treated empty as "keep", so removing a proxy
    password while keeping the endpoint was not expressible at all."""
    prof = repo(store).create(
        made("alpha", proxy=Proxy(server="http://h:1", username="u", password="gone")))

    def clear(p):
        p.proxy.password = ""
        return p

    repo(ProfileStore(store.root)).edit(prof.id, clear)

    engine, dash = engine_raw(store, prof.id), dash_raw(store, prof.id)
    assert engine["proxy"]["password"] in (None, "")
    assert dash["proxy"]["pass"] in (None, "")
    assert engine["proxy"]["server"] == "http://h:1", "the proxy itself was removed"
    assert engine["proxy"]["username"] == "u"


def test_an_explicit_clear_resolves_a_real_conflict(store):
    prof = diverged(store)

    repo(ProfileStore(store.root)).edit(
        prof.id, lambda p: setattr(p.proxy, "password", "") or p)

    assert engine_raw(store, prof.id)["proxy"]["password"] in (None, "")
    assert dash_raw(store, prof.id)["proxy"]["pass"] in (None, "")
    assert repo(ProfileStore(store.root)).divergence(prof.id) == []


def test_clearing_a_password_needs_an_unlocked_vault(store):
    """Destroying a credential this process cannot read is the one
    irreversible thing to get wrong."""
    pytest.importorskip("cryptography")
    from numera import crypto

    prof = repo(store).create(
        made("alpha", proxy=Proxy(server="http://h:1", username="u", password="kept")))
    store.enable_vault("master")
    before = engine_raw(store, prof.id)["proxy"]["password"]

    locked = ProfileStore(store.root)
    with pytest.raises(crypto.VaultLocked):
        repo(locked).edit(prof.id, lambda p: setattr(p.proxy, "password", "") or p)
    assert engine_raw(store, prof.id)["proxy"]["password"] == before

    unlocked = ProfileStore(store.root, password="master")
    repo(unlocked).edit(prof.id, lambda p: setattr(p.proxy, "password", "") or p)
    assert engine_raw(store, prof.id)["proxy"]["password"] in (None, "")


# --- what a client is told, and what a bulk write may not do ---------------

def test_the_api_answers_a_divergence_with_409(store):
    """The store's refusals had no HTTP meaning, so a request it deliberately
    declined came back as a 500 - which tells a client that Numera broke."""
    client = api(store)
    prof = diverged(store)

    for call in (lambda: client.put(f"/api/profiles/{prof.id}", json={"notes": "x"}),
                 lambda: client.post("/api/profiles/bulk",
                                     json={"ids": [prof.id], "patch": {"notes": "x"}})):
        answer = call()
        assert answer.status_code == 409, answer.text
        assert "proxy password" in answer.json()["detail"]
    assert engine_raw(store, prof.id)["proxy"]["password"] == "engine"
    assert dash_raw(store, prof.id)["proxy"]["pass"] == "dashboard"
    assert ProfileStore(store.root).get(prof.id).notes == ""


def test_the_api_answers_a_locked_clear_with_423(store):
    pytest.importorskip("cryptography")
    prof = repo(store).create(
        made("alpha", proxy=Proxy(server="http://h:1", username="u", password="kept")))
    store.enable_vault("master")
    before = engine_raw(store, prof.id)["proxy"]["password"]
    client = api(store)                       # no password: locked

    answer = client.put(f"/api/profiles/{prof.id}",
                        json={"proxy": {"type": "HTTP", "host": "h", "port": "1",
                                        "user": "u", "pass": ""}})

    assert answer.status_code == 423, answer.text
    assert engine_raw(store, prof.id)["proxy"]["password"] == before


def test_a_bulk_edit_cannot_settle_a_conflict_and_stays_atomic(store):
    """`POST /api/profiles/bulk` went through a second write path that had none
    of the single write's safeguards - so a notes edit across a selection could
    do what one profile at a time could not."""
    healthy = repo(store).create(made("healthy", seed=3))
    prof = diverged(store)
    client = api(store)

    answer = client.post("/api/profiles/bulk",
                         json={"ids": [healthy.id, prof.id], "patch": {"tags": ["t"]}})

    assert answer.status_code == 409, answer.text
    assert ProfileStore(store.root).get(healthy.id).tags == [], \
        "a refused bulk edit still changed another profile in the selection"
    assert engine_raw(store, prof.id)["proxy"]["password"] == "engine"
    assert dash_raw(store, prof.id)["proxy"]["pass"] == "dashboard"


def test_a_locked_bulk_edit_leaves_each_ciphertext_alone(store):
    pytest.importorskip("cryptography")
    from numera import crypto

    prof = repo(store).create(
        made("alpha", proxy=Proxy(server="http://h:1", username="u", password="first")))
    store.enable_vault("master")
    opened = ProfileStore(store.root, password="master")
    raw = dash_raw(store, prof.id)
    raw["proxy"] = {**raw["proxy"],
                    "pass": crypto.encrypt("master", opened.vault()["salt"], "second")}
    write_dash(store, prof.id, raw)
    before = (engine_raw(store, prof.id)["proxy"]["password"],
              dash_raw(store, prof.id)["proxy"]["pass"])

    answer = api(store).post("/api/profiles/bulk",
                             json={"ids": [prof.id], "patch": {"notes": "bulk"}})

    assert answer.status_code == 200, answer.text
    assert (engine_raw(store, prof.id)["proxy"]["password"],
            dash_raw(store, prof.id)["proxy"]["pass"]) == before, \
        "a bulk edit normalised the two copies to one blob"
    assert ProfileStore(store.root).get(prof.id).notes == "bulk"


def test_a_bulk_edit_cannot_erase_a_one_sided_secret_either(store):
    """The bulk path has to refuse everything the single path refuses. Here the
    engine copy is the only one holding a working credential, and mirroring the
    empty side across is how it would have been lost."""
    prof = one_sided(store, "working-secret", "")
    before = (engine_raw(store, prof.id)["proxy"]["password"],
              dash_raw(store, prof.id)["proxy"]["pass"])

    answer = api(store).post("/api/profiles/bulk",
                             json={"ids": [prof.id], "patch": {"notes": "bulk"}})

    assert answer.status_code == 409, answer.text
    assert (engine_raw(store, prof.id)["proxy"]["password"],
            dash_raw(store, prof.id)["proxy"]["pass"]) == before
    assert ProfileStore(store.root).get(prof.id).notes == ""


def test_a_healthy_bulk_edit_still_works(store):
    a = repo(store).create(made("a", seed=1))
    b = repo(store).create(made("b", seed=2))

    answer = api(store).post("/api/profiles/bulk",
                             json={"ids": [a.id, b.id], "patch": {"tags": ["ok"]}})

    assert answer.status_code == 200, answer.text
    assert ProfileStore(store.root).get(a.id).tags == ["ok"]
    assert ProfileStore(store.root).get(b.id).tags == ["ok"]


def test_either_side_of_a_conflict_can_be_chosen_explicitly(store):
    """The value alone cannot say whether a caller chose it or echoed it, so
    the model records the assignment. Before, choosing the password one of the
    copies already held was indistinguishable from an unrelated edit and was
    refused - the conflict could only be resolved with a third password."""
    for chosen in ("engine", "dashboard"):
        prof = diverged(store, name=f"pick-{chosen}")

        repo(ProfileStore(store.root)).edit(
            prof.id, lambda p: setattr(p.proxy, "password", chosen) or p)

        assert engine_raw(store, prof.id)["proxy"]["password"] == chosen
        assert dash_raw(store, prof.id)["proxy"]["pass"] == chosen
        assert repo(ProfileStore(store.root)).divergence(prof.id) == []


def test_the_shape_the_dashboard_sends_preserves_the_secret(store):
    """GET -> change notes -> PUT the whole object back, as the GUI does.

    The dashboard loads profiles with the proxy password decrypted, so PUTting
    the object it was given sent that password again - and a password in a
    write means REPLACE. An ordinary notes edit therefore *chose* the dashboard
    copy of a divergent credential and converged both files, where the server
    owes a 409. It is a client-side defect by construction: the server cannot
    tell "the user retyped the same password" from "the field was never
    touched", which is why `api.js` omits the field unless the editor says the
    user set one (`forWrite`, and its own tests).

    This mirrors that payload, so a regression in the boundary fails here too.
    """
    client = api(store)
    prof = diverged(store)

    # The GUI has no single-profile GET; it lists, and edits one of those.
    loaded = next(x for x in client.get("/api/profiles").json()
                  if x["id"] == prof.id)
    assert loaded["proxy"]["pass"] == "dashboard",         "the listing stopped handing out a readable divergent credential"
    edited = {**loaded, "notes": "just a note"}
    edited["proxy"] = {k: v for k, v in edited["proxy"].items() if k != "pass"}

    answer = client.put(f"/api/profiles/{prof.id}", json=edited)

    assert answer.status_code == 409, answer.text
    assert engine_raw(store, prof.id)["proxy"]["password"] == "engine"
    assert dash_raw(store, prof.id)["proxy"]["pass"] == "dashboard"
    assert ProfileStore(store.root).get(prof.id).notes == ""


def test_the_dashboards_other_proxy_edits_still_go_through(store):
    """Omitting the password must not cost the user the rest of the form."""
    client = api(store)
    prof = repo(store).create(
        made("alpha", proxy=Proxy(server="http://h:1", username="u", password="kept")))

    loaded = next(x for x in client.get("/api/profiles").json()
                  if x["id"] == prof.id)
    edited = {**loaded, "proxy": {**loaded["proxy"], "host": "moved.example"}}
    edited["proxy"] = {k: v for k, v in edited["proxy"].items() if k != "pass"}

    assert client.put(f"/api/profiles/{prof.id}", json=edited).status_code == 200

    after = ProfileStore(store.root).get(prof.id)
    assert "moved.example" in after.proxy.server, after.proxy.server
    assert after.proxy.password == "kept", "the address edit took the password with it"


def test_the_api_can_choose_a_side_too(store):
    prof = diverged(store)

    answer = api(store).put(f"/api/profiles/{prof.id}",
                            json={"proxy": {"type": "HTTP", "host": "h", "port": "1",
                                            "user": "u", "pass": "dashboard"}})

    assert answer.status_code == 200, answer.text
    assert engine_raw(store, prof.id)["proxy"]["password"] == "dashboard"
    assert dash_raw(store, prof.id)["proxy"]["pass"] == "dashboard"
