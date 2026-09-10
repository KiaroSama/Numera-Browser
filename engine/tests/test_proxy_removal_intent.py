"""Removing the proxy is its own act, not a statement about its password.

0.27 gave a write three things it could say about `proxy.pass`: preserve,
clear, replace. `proxy: null` says none of them - it takes the proxy away, and
the credential with it - but it fell through to PRESERVE, and that read was
wrong in both directions at once.

**It refused what the user had just decided.** With two readable passwords the
copies disagree, and PRESERVE is exactly the write that must not settle that.
So `PUT {"proxy": null}` - delete this proxy - came back 409 as an unresolved
conflict, over a conflict the deletion resolves. The Python path did not:
`p.proxy = None` removed both without a word, so the two surfaces disagreed
about the same act.

**And it let through what it should have refused.** Under a locked vault an
explicit clear raises `VaultLocked`, because destroying a credential this
process cannot read is the one irreversible mistake here. A removal carries no
password field, so the guard that enforces that rule had nothing to look at and
the write went through - the same destruction by another name.

`REMOVE` is that act, named, and both surfaces answer it the same way.
"""

import json

import pytest

from numera.fingerprint import generate
from numera.models import Profile, Proxy
from numera.repo import CLEAR, PRESERVE, REMOVE, REPLACE, repo, secret_intent
from numera.store import ProfileStore


def made(name, **kw) -> Profile:
    return Profile(name=name, fingerprint=generate(seed=1), **kw)


def dash_raw(store, pid) -> dict:
    return json.loads((store.root / "dashboard" / f"{pid}.json").read_text("utf-8"))


def write_dash(store, pid, record) -> None:
    (store.root / "dashboard" / f"{pid}.json").write_text(
        json.dumps(record), encoding="utf-8")


def engine_raw(store, pid) -> dict:
    return json.loads((store.root / "profiles" / f"{pid}.json").read_text("utf-8"))


@pytest.fixture
def store(tmp_path):
    return ProfileStore(tmp_path)


def api(store, password=None):
    pytest.importorskip("fastapi")
    from fastapi.testclient import TestClient

    from numera.server import create_app

    return TestClient(create_app(data_dir=str(store.root), start_scheduler=False,
                                 password=password))


def with_proxy(store, name="alpha", password="secret"):
    return repo(store).create(made(
        name, proxy=Proxy(server="http://h:1", username="u", password=password)))


def diverged(store, name="alpha"):
    """Two readable passwords - the conflict a removal is entitled to settle."""
    prof = with_proxy(store, name, "engine")
    raw = dash_raw(store, prof.id)
    raw["proxy"] = {**raw["proxy"], "pass": "dashboard"}
    write_dash(store, prof.id, raw)
    return prof


def both_raw(store, pid) -> tuple:
    return engine_raw(store, pid), dash_raw(store, pid)


# --- what a write says -----------------------------------------------------

def test_the_four_things_a_write_can_say():
    """Absent and null are different statements, which is why this takes the
    whole record: the proxy alone cannot tell them apart."""
    assert secret_intent({"name": "x"}) == PRESERVE
    assert secret_intent({"proxy": None}) == REMOVE
    assert secret_intent({"proxy": {}}) == PRESERVE
    assert secret_intent({"proxy": {"pass": None}}) == PRESERVE
    assert secret_intent({"proxy": {"pass": ""}}) == CLEAR
    assert secret_intent({"proxy": {"pass": "x"}}) == REPLACE
    assert secret_intent(None) == PRESERVE
    assert secret_intent({"proxy": "nonsense"}) == PRESERVE


# --- the HTTP surface ------------------------------------------------------

def test_removing_a_proxy_removes_it_from_both_copies(store):
    client = api(store)
    prof = with_proxy(store)

    answer = client.put(f"/api/profiles/{prof.id}",
                        json={"id": prof.id, "name": "alpha", "proxy": None})

    assert answer.status_code == 200, answer.text
    engine, dash = both_raw(store, prof.id)
    assert not engine.get("proxy"), engine.get("proxy")
    assert not dash.get("proxy"), dash.get("proxy")
    assert ProfileStore(store.root).get(prof.id).proxy is None


def test_removing_a_divergent_proxy_is_allowed_and_settles_it(store):
    """The user chose deletion, which answers the question the conflict asks.

    PRESERVE is refused over a readable divergence precisely because it decides
    nothing; a removal decides everything, so refusing it made the conflict
    unresolvable through the surface that reported it.
    """
    client = api(store)
    prof = diverged(store)

    answer = client.put(f"/api/profiles/{prof.id}",
                        json={"id": prof.id, "name": "alpha", "proxy": None})

    assert answer.status_code == 200, answer.text
    engine, dash = both_raw(store, prof.id)
    assert not engine.get("proxy") and not dash.get("proxy")


def test_an_unrelated_edit_over_a_divergence_is_still_refused(store):
    """The control: making removal explicit must not soften preservation."""
    client = api(store)
    prof = diverged(store)
    before = both_raw(store, prof.id)

    answer = client.put(f"/api/profiles/{prof.id}",
                        json={"id": prof.id, "name": "alpha", "notes": "hello"})

    assert answer.status_code == 409, answer.text
    assert both_raw(store, prof.id) == before


def test_a_locked_vault_refuses_a_removal_it_cannot_read(store):
    """The rule an explicit clear already obeys, reached by the other door."""
    pytest.importorskip("cryptography")
    prof = with_proxy(store)
    store.enable_vault("master")
    before = both_raw(store, prof.id)
    assert before[0]["proxy"]["password"] != "secret", "the vault sealed nothing"

    answer = api(store).put(f"/api/profiles/{prof.id}",
                            json={"id": prof.id, "name": "alpha", "proxy": None})

    assert answer.status_code == 423, answer.text
    assert both_raw(store, prof.id) == before, "a refused removal still wrote"


def test_an_unlocked_vault_removes_the_sealed_credential(store):
    pytest.importorskip("cryptography")
    prof = with_proxy(store)
    store.enable_vault("master")

    answer = api(store, password="master").put(
        f"/api/profiles/{prof.id}",
        json={"id": prof.id, "name": "alpha", "proxy": None})

    assert answer.status_code == 200, answer.text
    engine, dash = both_raw(store, prof.id)
    assert not engine.get("proxy") and not dash.get("proxy")


def test_a_profile_that_never_had_a_proxy_can_still_be_saved_locked(store):
    """`proxy: null` over nothing stored destroys nothing, so it is allowed
    even locked - the rule is about a credential, not about the word."""
    pytest.importorskip("cryptography")
    prof = repo(store).create(made("alpha"))
    store.enable_vault("master")

    answer = api(store).put(f"/api/profiles/{prof.id}",
                            json={"id": prof.id, "name": "alpha", "proxy": None})

    assert answer.status_code == 200, answer.text


# --- the same act through Python -------------------------------------------

def _drop_proxy(profile):
    profile.proxy = None
    return profile

def test_the_python_path_refuses_the_same_removal(store):
    """Both surfaces or neither: the asymmetry was the defect."""
    pytest.importorskip("cryptography")
    from numera import crypto

    prof = with_proxy(store)
    store.enable_vault("master")
    before = both_raw(store, prof.id)

    with pytest.raises(crypto.VaultLocked):
        repo(ProfileStore(store.root)).edit(prof.id, _drop_proxy)

    assert both_raw(store, prof.id) == before, "a refused removal still wrote"


def test_the_python_path_allows_it_unlocked(store):
    pytest.importorskip("cryptography")
    prof = with_proxy(store)
    store.enable_vault("master")

    repo(ProfileStore(store.root, password="master")).edit(prof.id, _drop_proxy)

    engine, dash = both_raw(store, prof.id)
    assert not engine.get("proxy") and not dash.get("proxy")


def test_the_python_path_removes_a_divergent_proxy_too(store):
    prof = diverged(store)

    repo(ProfileStore(store.root)).edit(prof.id, _drop_proxy)

    engine, dash = both_raw(store, prof.id)
    assert not engine.get("proxy") and not dash.get("proxy")


# --- the payload the Dashboard's toggle actually sends ----------------------

@pytest.mark.parametrize("username,password", [(None, None), ("u", None),
                                               ("u", "secret")])
def test_a_proxy_the_caller_never_saw_may_not_be_removed(store, username, password):
    """The trap making removal explicit opens, and where it has to stop.

    A one-sided proxy - engine copy only - is not shown by the listing at all.
    A client that sends the whole record back therefore sends `null` for a
    proxy it never knew existed, and reading that as "delete it" would destroy
    a credential on an unrelated notes edit. The same shape as the password
    echo, and the same answer: not a decision, so not honoured.
    """
    client = api(store)
    prof = repo(store).create(made(
        "alpha", proxy=Proxy(server="http://h:1", username=username,
                             password=password)))
    raw = dash_raw(store, prof.id)
    raw.pop("proxy", None)
    write_dash(store, prof.id, raw)
    paths = [store.root / folder / f"{prof.id}.json"
             for folder in ("profiles", "dashboard")]
    before = [path.read_bytes() for path in paths]

    loaded = next(x for x in client.get("/api/profiles").json()
                  if x["id"] == prof.id)
    assert not loaded.get("proxy"), "the listing started showing it after all"
    assert [path.read_bytes() for path in paths] == before, \
        "listing erased the hidden engine proxy before the save"

    answer = client.put(f"/api/profiles/{prof.id}",
                        json={**loaded, "notes": "hi", "proxy": None})

    assert answer.status_code == 409, answer.text
    assert [path.read_bytes() for path in paths] == before, \
        "an edit changed a proxy the caller was never shown"


@pytest.mark.parametrize("password", [None, "secret"])
def test_the_dashboard_toggle_off_removes_the_proxy(store, password):
    """List, turn the proxy off, PUT the whole object - what the GUI does."""
    client = api(store)
    prof = with_proxy(store, password=password)

    loaded = next(x for x in client.get("/api/profiles").json()
                  if x["id"] == prof.id)
    assert loaded["proxy"], "the listing stopped handing out a proxy"

    answer = client.put(f"/api/profiles/{prof.id}", json={**loaded, "proxy": None})

    assert answer.status_code == 200, answer.text
    engine, dash = both_raw(store, prof.id)
    assert not engine.get("proxy") and not dash.get("proxy")
