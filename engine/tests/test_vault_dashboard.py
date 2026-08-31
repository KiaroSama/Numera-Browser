"""A vault has to cover both copies of a profile, not one.

`DashboardStore` writes a second file for the same profile. It was writing the
proxy password there in plaintext regardless of the vault, so turning the vault
on encrypted `profiles/<id>.json` and left the same secret readable in
`dashboard/<id>.json` beside it.
"""
import json

import pytest

pytest.importorskip("cryptography")

from numera.server.stores import DashboardStore  # noqa: E402
from numera import crypto
from numera.store import ProfileStore  # noqa: E402

SECRET = "hunter2-the-proxy-password"


def stores(tmp_path):
    engine = ProfileStore(tmp_path)
    return engine, DashboardStore(tmp_path, engine)


def on_disk(tmp_path, pid):
    return (tmp_path / "dashboard" / f"{pid}.json").read_text(encoding="utf-8")


def profile(**kw):
    return {"name": "a", "os": "Windows",
            "proxy": {"type": "HTTP", "host": "1.2.3.4", "port": "8080",
                      "user": "u", "pass": SECRET}, **kw}


def test_without_a_vault_nothing_changes(tmp_path):
    engine, dash = stores(tmp_path)
    saved = dash.save(profile())
    assert SECRET in on_disk(tmp_path, saved["id"])


def test_an_unlocked_vault_encrypts_the_dashboard_copy_too(tmp_path):
    engine, dash = stores(tmp_path)
    engine.enable_vault("pw")
    saved = dash.save(profile())
    text = on_disk(tmp_path, saved["id"])
    assert SECRET not in text, "the secret is readable in the dashboard file"
    assert saved["proxy"]["pass"] != SECRET


def test_a_locked_vault_refuses_a_new_secret_rather_than_writing_it(tmp_path):
    engine, dash = stores(tmp_path)
    engine.enable_vault("pw")
    locked = ProfileStore(tmp_path)                 # no password: locked
    dash_locked = DashboardStore(tmp_path, locked)
    assert not locked.unlocked()
    # `crypto.VaultLocked`, not `PermissionError`. The two stores used to raise
    # different types for the same condition; they share one rule now
    # (`vault.encrypt_for_write`) and the engine store's type won.
    with pytest.raises(crypto.VaultLocked) as e:
        dash_locked.save(profile())
    assert "locked" in str(e.value)


def test_editing_a_non_secret_field_while_locked_keeps_the_password(tmp_path):
    """The editor sends back the redacted value it was shown, so a save used to
    replace the stored secret with the placeholder - losing it for good."""
    engine, dash = stores(tmp_path)
    engine.enable_vault("pw")
    saved = dash.save(profile())
    ciphertext = saved["proxy"]["pass"]

    locked = ProfileStore(tmp_path)
    dash_locked = DashboardStore(tmp_path, locked)
    again = dash_locked.save({**saved, "name": "renamed while locked"})

    assert again["name"] == "renamed while locked"
    assert again["proxy"]["pass"] == ciphertext, "the encrypted password was lost"
    assert SECRET not in on_disk(tmp_path, saved["id"])


def test_an_already_encrypted_value_is_not_encrypted_twice(tmp_path):
    engine, dash = stores(tmp_path)
    engine.enable_vault("pw")
    saved = dash.save(profile())
    again = dash.save(saved)
    assert again["proxy"]["pass"] == saved["proxy"]["pass"]


def test_a_profile_with_no_proxy_password_is_untouched(tmp_path):
    engine, dash = stores(tmp_path)
    engine.enable_vault("pw")
    saved = dash.save({"name": "no proxy", "os": "Windows"})
    assert saved["name"] == "no proxy"
