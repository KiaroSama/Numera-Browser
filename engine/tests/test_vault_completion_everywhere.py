"""One completion invariant, on every path - including the shortcuts.

`0.35.0` left one exception to it. When the config's salt matched the journal's,
`_finish_enable` ran `structural_problems` instead of `completion_problems`, on
the reasoning that a store already serving under that key is not choosing
anything. Two measured consequences:

* **a salt is not a descriptor.** Three credentials re-encrypted under a
  *different* key that happens to share the salt passed the check. The
  descriptor recovery then confirmed opened **0 of 3** of them, and the journal
  naming the key they were actually under was deleted.
* **only salts were compared**, so a config verifier that opens nothing
  survived a migration whose journal carried a good one.

A second hole was one level in: `_container_ok` checked the outer shape only,
so a pool of `[1, 2, "x"]` was "a list, therefore fine" - and `vault.holders`
skips every element that is not a dict, so the walk found nothing and the file
read as "no secrets here".

Everything here goes through the real `ProfileStore`, the real writers and the
real `vault.recover`. Fixtures are planted by writing the same files the
product writes, then read back by decrypting with the active descriptor - never
by trusting a return value.
"""

import json

import pytest

from numera import crypto, vault
from numera.store import ProfileStore

PW = "master-alpha"
OTHER = "master-beta"

#: One credential per registered site, so "every expected secret" is a real
#: claim rather than one file standing in for three.
SECRETS = {"engine": "engine-secret", "dash": "dash-secret", "pool": "pool-secret"}


def config_of(root):
    return json.loads((root / "config.json").read_text(encoding="utf-8"))


def write_config(root, cfg):
    (root / "config.json").write_text(json.dumps(cfg), encoding="utf-8")


def plant(root):
    """The three secret-bearing sites, in the clear, as the product writes them."""
    (root / "profiles").mkdir(parents=True, exist_ok=True)
    (root / "dashboard").mkdir(parents=True, exist_ok=True)
    (root / "profiles" / "p1.json").write_text(json.dumps(
        {"id": "p1", "name": "one",
         "proxy": {"host": "h", "port": 1, "password": SECRETS["engine"]}}),
        encoding="utf-8")
    (root / "dashboard" / "p1.json").write_text(json.dumps(
        {"id": "p1", "proxy": {"host": "h", "port": 1, "pass": SECRETS["dash"]}}),
        encoding="utf-8")
    (root / "proxies.json").write_text(json.dumps(
        [{"id": "x", "host": "h", "port": 1, "pass": SECRETS["pool"]}]),
        encoding="utf-8")
    write_config(root, {})


def site_of(path):
    return {"p1.json": "engine", "proxies.json": "pool"}.get(
        path.name, "dash") if path.parent.name != "dashboard" else "dash"


def decrypt_every_secret(root, password, descriptor):
    """`{site: plaintext}` read back through the active descriptor.

    The independent half of an accepted recovery: a `True` return says the
    product believes it finished, and this says whether the credentials it was
    protecting can still be read.
    """
    seen = {}
    for path, site in vault.secret_files(root):
        data = json.loads(path.read_text(encoding="utf-8"))
        for holder in vault.holders(data, site):
            value = holder[site.field]
            assert crypto.is_encrypted(value), f"{path.name} is still readable"
            seen[site_of(path)] = crypto.decrypt(password, descriptor["salt"], value)
    return seen


@pytest.fixture
def enabled(tmp_path):
    """A real store with the vault on under `PW`, and its descriptor."""
    plant(tmp_path)
    ProfileStore(tmp_path).enable_vault(PW)
    return tmp_path, dict(config_of(tmp_path)["vault"])


def journal_enable(root, descriptor):
    """An abandoned `enable` journal nobody owns."""
    vault.begin_journal(root, "enable", new=descriptor)
    record = json.loads(vault.journal_path(root).read_text(encoding="utf-8"))
    record["pid"] = None
    vault.journal_path(root).write_text(json.dumps(record), encoding="utf-8")


def recover(root, **kw):
    cfg = config_of(root)

    def save(new):
        cfg.update(new)
        write_config(root, cfg)

    return vault.recover(root, lambda: cfg, save, **kw)


# ---------------------------------------------------------------------------
# The same-salt exception
# ---------------------------------------------------------------------------

def test_same_salt_foreign_ciphertext_is_refused_with_the_password(enabled):
    """The reproduction. Same salt, different key, correct password supplied.

    The shortcut confirmed the descriptor and deleted the journal; the three
    credentials then opened under neither.
    """
    root, good = enabled
    # Re-encrypt every secret under a *different* password, same salt.
    for path, site in vault.secret_files(root):
        data = json.loads(path.read_text(encoding="utf-8"))
        for holder in vault.holders(data, site):
            holder[site.field] = crypto.encrypt(
                OTHER, good["salt"], SECRETS[site_of(path)])
        path.write_text(json.dumps(data), encoding="utf-8")
    journal_enable(root, good)

    with pytest.raises(vault.VaultIncomplete) as caught:
        recover(root, password=PW)

    assert vault.WRONG_GENERATION in caught.value.codes, caught.value
    assert vault.journal_path(root).exists(), "the evidence was deleted"
    # The data is untouched, so it is still recoverable by whoever has `OTHER`.
    assert decrypt_every_secret(root, OTHER, good) == SECRETS


def test_same_salt_foreign_ciphertext_is_refused_without_a_password(enabled):
    """No key at all: the refusal names what is missing rather than guessing."""
    root, good = enabled
    for path, site in vault.secret_files(root):
        data = json.loads(path.read_text(encoding="utf-8"))
        for holder in vault.holders(data, site):
            holder[site.field] = crypto.encrypt(
                OTHER, good["salt"], SECRETS[site_of(path)])
        path.write_text(json.dumps(data), encoding="utf-8")
    journal_enable(root, good)

    with pytest.raises(vault.VaultIncomplete) as caught:
        recover(root)

    assert vault.NO_PASSWORD in caught.value.codes, caught.value
    assert vault.journal_path(root).exists()


def test_a_damaged_config_verifier_is_not_hidden_by_a_matching_salt(enabled):
    """Salts equal, verifiers not - which the old comparison called identical.

    The journal's verifier was the good one, and deleting the journal was how
    the only readable copy of it went.
    """
    root, good = enabled
    journal_enable(root, good)
    cfg = config_of(root)
    cfg["vault"] = {"salt": good["salt"], "verifier": "enc:v2:opens-nothing"}
    write_config(root, cfg)

    with pytest.raises(vault.VaultIncomplete) as caught:
        recover(root, password=PW)

    assert vault.DESCRIPTOR_DISAGREEMENT in caught.value.codes, caught.value
    assert vault.journal_path(root).exists(), \
        "the journal holding the good verifier was deleted"
    assert config_of(root)["vault"]["verifier"] == "enc:v2:opens-nothing", \
        "a refusal rewrote the config"


def test_the_ordinary_already_committed_case_completes_with_the_key(enabled):
    """The control, and the price of the fix.

    Nothing is wrong here: the config names the journal's descriptor, the data
    is in that generation, the password opens it. It completes - but only
    because the key was supplied, which is the part that used to be optional.
    """
    root, good = enabled
    journal_enable(root, good)

    assert recover(root, password=PW) is True
    assert not vault.journal_path(root).exists()
    assert config_of(root)["vault"] == good
    assert decrypt_every_secret(root, PW, good) == SECRETS, \
        "an accepted recovery left a credential unreadable"
    # Reopening twice must not resurrect a migration.
    assert ProfileStore(root).vault() == good
    assert ProfileStore(root).vault() == good


# ---------------------------------------------------------------------------
# Nested shapes: parsing the container is not inspecting the records
# ---------------------------------------------------------------------------

@pytest.mark.parametrize("body,why", [
    ('[1, 2, "x"]', "a pool of non-records"),
    ('[{"id": "x", "pass": 5}]', "a secret that is not a string"),
    ('[{"id": "x", "pass": {"nested": "no"}}]', "a secret that is an object"),
])
def test_an_uninspectable_pool_entry_blocks_completion(enabled, body, why):
    """`_container_ok` said "it is a list, therefore fine".

    `vault.holders` then skipped every element it could not walk, so the file
    reported no secrets and completion discarded the journal over records
    nobody had looked at.
    """
    root, good = enabled
    (root / "proxies.json").write_text(body, encoding="utf-8")
    journal_enable(root, good)

    assert (root / "proxies.json") in vault.unreadable_secret_files(root), \
        f"{why} was not even recognised as unreadable"

    with pytest.raises(vault.VaultIncomplete) as caught:
        recover(root, password=PW)

    assert vault.UNREADABLE_FILES in caught.value.codes, caught.value
    assert vault.journal_path(root).exists()


@pytest.mark.parametrize("body,why", [
    ('[]', "an empty pool"),
    ('[{"id": "x", "host": "h"}]', "a pool entry with no secret at all"),
    ('[{"id": "x", "pass": ""}]', "an explicitly empty secret"),
])
def test_a_legitimately_empty_pool_still_completes(enabled, body, why):
    """The control that keeps the check honest.

    Absent and empty are not unreadable. A validator that refuses a pool with
    nothing to protect would block every store that has no proxies.
    """
    root, good = enabled
    (root / "proxies.json").write_text(body, encoding="utf-8")
    journal_enable(root, good)

    assert (root / "proxies.json") not in vault.unreadable_secret_files(root), \
        f"{why} was wrongly called unreadable"
    assert recover(root, password=PW) is True, why
    assert not vault.journal_path(root).exists()


@pytest.mark.parametrize("body,why", [
    ('{"id": "p1", "proxy": 7}', "a proxy node that is not an object"),
    ('{"id": "p1", "proxy": {"password": 5}}', "a secret that is not a string"),
    ('{"id": "p1", "proxy": {"password": ["a"]}}', "a secret that is a list"),
])
def test_an_uninspectable_profile_node_blocks_completion(enabled, body, why):
    """The same hole at the other kind of site.

    A profile is an object rather than a list, so `_container_ok` passed it on
    the outer shape alone; `vault.holders` then required the node to be a dict
    and skipped it silently when it was not. The record participates in the
    migration and nobody looked at it.
    """
    root, good = enabled
    (root / "profiles" / "p1.json").write_text(body, encoding="utf-8")
    journal_enable(root, good)

    assert (root / "profiles" / "p1.json") in vault.unreadable_secret_files(root), \
        f"{why} was not even recognised as unreadable"

    with pytest.raises(vault.VaultIncomplete) as caught:
        recover(root, password=PW)

    assert vault.UNREADABLE_FILES in caught.value.codes, caught.value
    assert vault.journal_path(root).exists()


@pytest.mark.parametrize("body,why", [
    ('{"id": "p1", "name": "one"}', "a profile with no proxy at all"),
    ('{"id": "p1", "proxy": {"host": "h", "port": 1}}', "a proxy with no password"),
    ('{"id": "p1", "proxy": {"password": ""}}', "an explicitly empty password"),
])
def test_a_profile_with_nothing_to_protect_still_completes(enabled, body, why):
    """Absent and empty stay legitimate at this site too."""
    root, good = enabled
    (root / "profiles" / "p1.json").write_text(body, encoding="utf-8")
    journal_enable(root, good)

    assert (root / "profiles" / "p1.json") not in vault.unreadable_secret_files(root), \
        f"{why} was wrongly called unreadable"
    assert recover(root, password=PW) is True, why
