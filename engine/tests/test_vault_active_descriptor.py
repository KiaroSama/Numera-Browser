"""Recovery completion is proved against the descriptor actually on disk."""
import copy
import errno
import json
from pathlib import Path

import pytest

from numera import crypto, vault, vault_recovery as recovery
from numera.paths import write_json

NEW_PASSWORD = "synthetic-new-password"
OLD_PASSWORD = "synthetic-old-password"
VALUES = ("synthetic-engine", "synthetic-dashboard", "synthetic-pool")


@pytest.fixture(scope="module")
def descriptors():
    def make(password):
        salt = crypto.new_salt()
        return {"salt": salt, "verifier": crypto.make_verifier(password, salt)}
    return make(NEW_PASSWORD), make(OLD_PASSWORD), make("unrelated-password")


def read_config(root):
    return json.loads((root / "config.json").read_text(encoding="utf-8"))


def snapshot(root):
    return {str(path.relative_to(root)): path.read_bytes()
            for path in root.rglob("*.json")}


def prepare(root, descriptors, *, op="rotate", active="new", plain=False,
            generation="new"):
    new, old, unrelated = copy.deepcopy(descriptors)
    active_value = {
        "new": new, "old": old, "foreign": unrelated,
        "bad-verifier": {**new, "verifier": "damaged"},
        "missing-verifier": {"salt": new["salt"]},
        "false": False, "list": [], "dict": {}, "null": None,
    }
    config = {} if active == "absent" else {"vault": active_value[active]}
    write_json(root / "config.json", config)
    desc, password = (old, OLD_PASSWORD) if generation == "old" else (new, NEW_PASSWORD)
    tokens = [crypto.encrypt(password, desc["salt"], value) for value in VALUES]
    if plain:
        tokens[2] = VALUES[2]
    write_json(root / "profiles/a.json", {"id": "a", "proxy": {"password": tokens[0]}})
    write_json(root / "dashboard/a.json", {"id": "a", "proxy": {"pass": tokens[1]}})
    write_json(root / "proxies.json", [{"id": "pool", "pass": tokens[2]}])
    write_json(root / "vault-migration.json", {
        "version": 1, "op": op, "new": new,
        "old": old if op == "rotate" else None, "pid": None,
    })


def recover(root, password=NEW_PASSWORD, old_password=OLD_PASSWORD, save=None):
    return vault.recover(root, lambda: read_config(root),
                         save or (lambda data: write_json(root / "config.json", data)),
                         password=password, old_password=old_password)


def assert_readable(root):
    active = read_config(root)["vault"]
    assert crypto.check_password(NEW_PASSWORD, active["salt"], active["verifier"])
    engine = json.loads((root / "profiles/a.json").read_text())["proxy"]["password"]
    dashboard = json.loads((root / "dashboard/a.json").read_text())["proxy"]["pass"]
    pool = json.loads((root / "proxies.json").read_text())[0]["pass"]
    # Independent values, not the implementation's plaintext scanner.
    assert tuple(crypto.decrypt(NEW_PASSWORD, active["salt"], token)
                 for token in (engine, dashboard, pool)) == VALUES
    assert all(crypto.is_encrypted(token) for token in (engine, dashboard, pool))
    assert json.loads((root / "profiles/a.json").read_text())["id"] == "a"
    assert json.loads((root / "proxies.json").read_text())[0]["id"] == "pool"


@pytest.mark.parametrize("active", ["bad-verifier", "missing-verifier", "foreign",
                                    "false", "list", "dict", "null", "absent"])
@pytest.mark.parametrize("plain", [False, True])
def test_rotation_never_confirms_or_overwrites_a_damaged_active_descriptor(
        tmp_path, descriptors, active, plain):
    prepare(tmp_path, descriptors, active=active, plain=plain)
    before = snapshot(tmp_path)
    with pytest.raises(vault.VaultRecoveryRequired):
        recover(tmp_path)
    assert snapshot(tmp_path) == before


@pytest.mark.parametrize("active", ["old", "foreign", "bad-verifier", "false", "list", "dict"])
@pytest.mark.parametrize("plain", [False, True])
def test_enable_checks_config_before_both_plaintext_and_cleanup_paths(
        tmp_path, descriptors, active, plain):
    prepare(tmp_path, descriptors, op="enable", active=active, plain=plain)
    before = snapshot(tmp_path)
    with pytest.raises(vault.VaultRecoveryRequired):
        recover(tmp_path)
    assert snapshot(tmp_path) == before


@pytest.mark.parametrize("op,active,generation,plain", [
    ("enable", "absent", "new", False),
    ("enable", "absent", "new", True),
    ("enable", "null", "new", True),
    ("enable", "new", "new", False),
    ("enable", "new", "new", True),
    ("rotate", "old", "old", False),
    ("rotate", "old", "old", True),
    ("rotate", "new", "new", False),
    ("rotate", "new", "old", True),
])
def test_valid_transitions_preserve_all_values_and_reopen_twice(
        tmp_path, descriptors, op, active, generation, plain):
    prepare(tmp_path, descriptors, op=op, active=active,
            generation=generation, plain=plain)
    assert recover(tmp_path) is True
    assert_readable(tmp_path)
    for _ in range(2):
        assert recover(tmp_path) is False
        assert_readable(tmp_path)


@pytest.mark.parametrize("op,active", [("enable", "absent"), ("enable", "new"),
                                      ("rotate", "old"), ("rotate", "new")])
def test_missing_password_preserves_evidence(tmp_path, descriptors, op, active):
    prepare(tmp_path, descriptors, op=op, active=active)
    before = snapshot(tmp_path)
    with pytest.raises((vault.VaultRecoveryRequired, crypto.VaultLocked)):
        recover(tmp_path, password=None, old_password=None)
    assert snapshot(tmp_path) == before


@pytest.mark.parametrize("op,active", [("enable", "absent"), ("rotate", "old")])
@pytest.mark.parametrize("bad_save", ["noop", "damaged-verifier"])
def test_successful_save_callback_must_really_persist_the_expected_descriptor(
        tmp_path, descriptors, op, active, bad_save):
    prepare(tmp_path, descriptors, op=op, active=active, plain=True)
    before = snapshot(tmp_path)
    def save(data):
        if bad_save == "damaged-verifier":
            data = copy.deepcopy(data)
            data["vault"]["verifier"] = "damaged-after-save"
            write_json(tmp_path / "config.json", data)
    with pytest.raises(vault.VaultRecoveryRequired):
        recover(tmp_path, save=save)
    assert snapshot(tmp_path) == before
    assert recover(tmp_path) is True
    assert_readable(tmp_path)


@pytest.mark.parametrize("target", ["vault-migration.json", "proxies.json"])
def test_invalid_utf8_refuses_without_losing_recovery_evidence(tmp_path, descriptors, target):
    prepare(tmp_path, descriptors)
    (tmp_path / target).write_bytes(b"\xff\xfe")
    before = snapshot(tmp_path)
    with pytest.raises(vault.VaultRecoveryRequired):
        recover(tmp_path)
    assert snapshot(tmp_path) == before


def test_journal_unlink_failure_is_not_silently_reported_as_completion(
        tmp_path, descriptors, monkeypatch):
    prepare(tmp_path, descriptors)
    original = Path.unlink
    def refuse(path, *args, **kwargs):
        if path == tmp_path / "vault-migration.json":
            raise PermissionError(errno.EACCES, "injected cleanup failure")
        return original(path, *args, **kwargs)
    with monkeypatch.context() as patch:
        patch.setattr(Path, "unlink", refuse)
        with pytest.raises(vault.VaultRecoveryRequired, match="cleanup"):
            recover(tmp_path)
    assert (tmp_path / "vault-migration.json").exists()
    assert_readable(tmp_path)
    assert recover(tmp_path) is True
    assert_readable(tmp_path)


@pytest.mark.parametrize("body", [b"\xff", b"{bad", b"[]", b"null"])
def test_damaged_config_is_not_treated_as_an_empty_fresh_install(
        tmp_path, descriptors, body):
    prepare(tmp_path, descriptors, op="enable", active="absent", plain=True)
    (tmp_path / "config.json").write_bytes(body)
    before = snapshot(tmp_path)
    # ConfigStore's read contract can return {} for unreadable settings. The
    # recovery boundary must inspect the persisted object before changing it.
    with pytest.raises(vault.VaultRecoveryRequired):
        vault.recover(tmp_path, lambda: {},
                      lambda data: write_json(tmp_path / "config.json", data),
                      password=NEW_PASSWORD)
    assert snapshot(tmp_path) == before


@pytest.mark.parametrize("checkpoint", ["before-recovery", "profiles", "dashboard",
    "proxies.json", "before-config", "after-config", "before-config-sync",
    "after-config-sync", "before-cleanup", "after-cleanup"])
def test_abrupt_exit_can_recover_all_three_sites_and_reopen_twice(
        tmp_path, descriptors, checkpoint):
    import os
    import subprocess
    import sys
    prepare(tmp_path, descriptors, active="old", generation="old", plain=True)
    program = r'''
import json, os, sys
from pathlib import Path
from numera import vault, vault_recovery as recovery
from numera.paths import write_json
root, point = Path(sys.argv[1]), sys.argv[2]
def at(name):
    if name == point:
        os._exit(73)
original_write = vault._Batch.write
def write(batch, path, data):
    original_write(batch, path, data)
    path = Path(path)
    at(path.parent.name if path.parent.name in ("profiles", "dashboard") else path.name)
vault._Batch.write = write
original_sync = recovery.sync_config
def sync(root):
    at("before-config-sync")
    original_sync(root)
    at("after-config-sync")
recovery.sync_config = sync
original_end = vault.end_journal
def end(root):
    at("before-cleanup")
    original_end(root)
    at("after-cleanup")
vault.end_journal = end
def save(data):
    at("before-config")
    write_json(root / "config.json", data)
    at("after-config")
at("before-recovery")
vault.recover(root, lambda: json.loads((root / "config.json").read_text()), save,
              password="synthetic-new-password", old_password="synthetic-old-password")
'''
    worker = subprocess.run([sys.executable, "-c", program, str(tmp_path), checkpoint],
                            env=dict(os.environ), capture_output=True, timeout=30)
    assert worker.returncode == 73, worker.stderr.decode(errors="replace")
    recover(tmp_path)
    assert_readable(tmp_path)
    for _ in range(2):
        assert recover(tmp_path) is False
        assert_readable(tmp_path)
