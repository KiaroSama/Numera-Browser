"""Turning the vault on must protect every secret in the data root, not one file.

`enable_vault()` walked the engine's own `profiles/*.json` and stopped there,
because that is the store it belongs to. The same proxy password is written to
two more places under the same root - the dashboard's copy of each profile, and
the shared proxy pool - and both kept it in the clear. The command reported
success and the product claimed proxy passwords were encrypted at rest.

The check that matters here is not "did the code run" but "is anything still
readable", so most of these grep the whole directory tree.
"""
import json

import pytest

from numera import generate, vault
from numera.models import Profile, Proxy
from numera.server.stores import DashboardStore, ProxyPool
from numera.store import ProfileStore

pytest.importorskip("cryptography")

ENGINE_SECRET = "ENGINE-SECRET"
DASH_SECRET = "DASHBOARD-SECRET"
POOL_SECRET = "POOL-SECRET"


def populated(root):
    """One data root with a secret in each of the three stores."""
    engine = ProfileStore(root)
    dash = DashboardStore(root, engine_store=engine)
    pool = ProxyPool(root, engine)
    engine.save(Profile(name="engine-one", fingerprint=generate(seed=1),
                        proxy=Proxy(server="http://1.2.3.4:8080", username="u",
                                    password=ENGINE_SECRET)))
    dash.save({"name": "dash-one", "tags": [],
               "proxy": {"type": "HTTP", "host": "5.6.7.8", "port": "8080",
                         "user": "u", "pass": DASH_SECRET}})
    pool.add_many([{"type": "SOCKS5", "host": "9.9.9.9", "port": "1080",
                    "user": "u", "pass": POOL_SECRET}])
    return engine, dash, pool


def readable(root, needle):
    """Every file under `root` whose bytes contain `needle`."""
    hits = []
    for f in sorted(root.rglob("*")):
        if not f.is_file():
            continue
        try:
            if needle in f.read_text(encoding="utf-8"):
                hits.append(str(f.relative_to(root)))
        except (OSError, UnicodeDecodeError):
            continue
    return hits


# --- the guarantee ---------------------------------------------------------

def test_no_secret_is_readable_anywhere_after_enabling(tmp_path):
    """The whole point. Before this, two of the three were still in the clear."""
    engine, _, _ = populated(tmp_path)
    engine.enable_vault("master")

    for secret in (ENGINE_SECRET, DASH_SECRET, POOL_SECRET):
        assert readable(tmp_path, secret) == [], \
            f"{secret} is still readable in {readable(tmp_path, secret)}"


def test_the_root_reports_no_plaintext_secrets_left(tmp_path):
    """`vault.plaintext_secrets()` exists so the claim can be checked rather
    than asserted - it is what a future store addition will fail against."""
    engine, _, _ = populated(tmp_path)
    assert vault.plaintext_secrets(tmp_path), "the fixture stored nothing to protect"
    engine.enable_vault("master")
    assert vault.plaintext_secrets(tmp_path) == []


def test_every_secret_bearing_file_is_in_the_site_table(tmp_path):
    """A store whose file is not in `SITES` is invisible to the migration, and
    that is exactly how the dashboard copy and the pool were missed."""
    populated(tmp_path)
    covered = {p for p, _ in vault.secret_files(tmp_path)}
    assert (tmp_path / "proxies.json") in covered
    assert any(p.parent.name == "dashboard" for p in covered)
    assert any(p.parent.name == "profiles" for p in covered)


# --- all or nothing --------------------------------------------------------

def test_a_failure_part_way_leaves_no_vault_and_nothing_converted(tmp_path, monkeypatch):
    """A half-migrated store is the one state nothing downstream can read."""
    from numera import crypto

    engine, _, _ = populated(tmp_path)
    before = {f: f.read_bytes() for f in sorted(tmp_path.rglob("*.json")) if f.is_file()}
    real = crypto.encrypt
    calls = {"n": 0}

    def fail_on_the_third(*a, **k):
        calls["n"] += 1
        if calls["n"] == 3:
            raise OSError("disk full")
        return real(*a, **k)

    monkeypatch.setattr(crypto, "encrypt", fail_on_the_third)
    with pytest.raises(OSError):
        engine.enable_vault("master")

    assert not ProfileStore(tmp_path).vault_enabled(), "a failed migration switched it on"
    for f, original in before.items():
        assert f.read_bytes() == original, f"{f.name} was left converted"


def test_a_config_write_failure_also_rolls_the_secrets_back(tmp_path, monkeypatch):
    """The descriptor is written last and inside the same block, so the vault
    becoming visible is the step that commits everything before it."""
    engine, _, _ = populated(tmp_path)
    before = {f: f.read_bytes() for f in sorted(tmp_path.rglob("*.json")) if f.is_file()}
    monkeypatch.setattr(ProfileStore, "save_config",
                        lambda self, cfg: (_ for _ in ()).throw(OSError("read-only")))
    with pytest.raises(OSError):
        engine.enable_vault("master")

    assert not ProfileStore(tmp_path).vault_enabled()
    for f, original in before.items():
        assert f.read_bytes() == original, f"{f.name} stayed encrypted after a rollback"


# --- and everything keeps working ------------------------------------------

def test_unlocked_reads_give_back_the_real_secrets(tmp_path):
    engine, dash, pool = populated(tmp_path)
    engine.enable_vault("master")

    unlocked = ProfileStore(tmp_path, password="master")
    assert unlocked.unlocked()
    assert unlocked.resolve("engine-one").proxy.password == ENGINE_SECRET

    d = DashboardStore(tmp_path, engine_store=unlocked)
    got = next(p for p in d.list() if p["name"] == "dash-one")
    assert got["proxy"]["pass"] == DASH_SECRET

    pl = ProxyPool(tmp_path, unlocked)
    assert pl.list()[0]["pass"] == POOL_SECRET


def test_a_locked_read_yields_nothing_rather_than_the_ciphertext(tmp_path):
    """Handing the encrypted blob back would send it onwards as if it were the
    password: the launcher would authenticate with it and the dashboard would
    print it into a password field."""
    from numera import crypto

    engine, _, _ = populated(tmp_path)
    engine.enable_vault("master")

    locked = ProfileStore(tmp_path)
    assert not locked.unlocked()
    d = DashboardStore(tmp_path, engine_store=locked)
    got = next(p for p in d.list() if p["name"] == "dash-one")
    assert got["proxy"]["pass"] is None
    assert not crypto.is_encrypted(got["proxy"]["pass"] or "")

    entry = ProxyPool(tmp_path, locked).list()[0]
    assert entry["pass"] is None


def test_saving_again_does_not_encrypt_twice(tmp_path):
    engine, dash, pool = populated(tmp_path)
    engine.enable_vault("master")
    unlocked = ProfileStore(tmp_path, password="master")

    d = DashboardStore(tmp_path, engine_store=unlocked)
    prof = next(p for p in d.list() if p["name"] == "dash-one")
    d.save({**prof, "notes": "edited"})
    assert next(p for p in d.list() if p["name"] == "dash-one")["proxy"]["pass"] == DASH_SECRET

    pl = ProxyPool(tmp_path, unlocked)
    entry = pl.list()[0]
    pl.update(entry["id"], {"country": "DE"})
    assert pl.list()[0]["pass"] == POOL_SECRET
    assert readable(tmp_path, POOL_SECRET) == [], "an edit wrote the password back in the clear"


def test_a_locked_edit_keeps_the_stored_secret(tmp_path):
    """A locked read gives `None`, so an ordinary edit comes back carrying
    `None`; writing that would destroy the password."""
    engine, _, _ = populated(tmp_path)
    engine.enable_vault("master")

    locked = ProfileStore(tmp_path)
    pl = ProxyPool(tmp_path, locked)
    entry = pl.list()[0]
    assert entry["pass"] is None
    pl.update(entry["id"], {"country": "FR"})

    unlocked = ProxyPool(tmp_path, ProfileStore(tmp_path, password="master"))
    assert unlocked.list()[0]["pass"] == POOL_SECRET, "the locked edit lost the password"
    assert unlocked.list()[0]["country"] == "FR", "the locked edit did not apply"


def test_pool_identity_survives_encryption(tmp_path):
    """Encryption is randomised, so the same password stored twice gives two
    different ciphertexts. Keying dedupe on the stored bytes would make every
    proxy look unique and every usage count fall to zero."""
    engine, _, pool = populated(tmp_path)
    engine.enable_vault("master")
    pl = ProxyPool(tmp_path, ProfileStore(tmp_path, password="master"))

    same = {"type": "SOCKS5", "host": "9.9.9.9", "port": "1080",
            "user": "u", "pass": POOL_SECRET}
    assert pl.add_many([same]) == [], "the same proxy was added a second time"
    assert len(pl.list()) == 1


def test_a_new_secret_while_locked_is_refused_not_written(tmp_path):
    from numera import crypto

    engine, _, _ = populated(tmp_path)
    engine.enable_vault("master")
    pl = ProxyPool(tmp_path, ProfileStore(tmp_path))
    with pytest.raises(crypto.VaultLocked):
        pl.add_many([{"type": "HTTP", "host": "1.1.1.1", "port": "80",
                      "user": "u", "pass": "NEW-PLAINTEXT"}])
    assert readable(tmp_path, "NEW-PLAINTEXT") == []


def test_a_pool_without_an_engine_store_still_works(tmp_path):
    """The two real call sites pass one; a caller that has no data root of its
    own keeps the old plaintext behaviour rather than crashing."""
    pool = ProxyPool(tmp_path)
    pool.add_many([{"type": "HTTP", "host": "2.2.2.2", "port": "80", "pass": "plain"}])
    assert pool.list()[0]["pass"] == "plain"


def test_both_real_call_sites_give_the_pool_a_vault():
    """Forgetting the argument is silent: the pool would write plaintext under
    an enabled vault and nothing would say so."""
    from pathlib import Path

    root = Path(__file__).resolve().parents[1] / "numera"
    cli = (root / "cli" / "pool.py").read_text(encoding="utf-8")
    server = (root / "server" / "__init__.py").read_text(encoding="utf-8")
    assert "ProxyPool(store.root, store)" in cli, "the CLI pool has no vault"
    assert "ProxyPool(engine_store.root, engine_store)" in server, "the API pool has no vault"
