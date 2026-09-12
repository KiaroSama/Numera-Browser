"""A migration commits data and descriptor together, or neither.

`0.32.0` gave recovery a journal, four journal states and a durability order.
What it did not give it was a *commit point*. The descriptor was written by a
caller-supplied `save_config` callback that sat outside the migration's undo
batch, so the two halves of one transaction could be rolled back separately:

    with vault.migration(root, keep_journal=True) as step:
        step.rekey_all(...)          # in the batch - undone on failure
        _sync_secret_files(root)
        save_config({... "vault": new})   # NOT in the batch - survives failure
        _sync_config(root)                # <- raise here

The files go back to the old generation and the config keeps naming the new
one. Then the retry asks the wrong question - "does config already name the
new salt?" - answers yes, drops the journal and reports success, over records
no password in existence can open.

The other half is what "finished" was allowed to mean. A salt that matches is
not a generation that was converted; a secret file that *parses* is not one
whose shape the product can read (`proxies.json = null` parses); a journal with
a salt is not a descriptor (no verifier, no version check); and `fsync_path`
treated `EACCES`/`EBADF` on an ordinary file as "this platform cannot sync
that", which is only true of a directory handle on Windows.

**Cost.** Everything here needs real, decryptable credentials - a `verifier="v"`
fixture would prove nothing about whether a generation converted. PBKDF2 at
600,000 rounds is ~0.5 s per distinct salt, so the module pays for exactly two
(one enable, one rotation target) in a module-scoped fixture and every test
copies that directory instead of building a vault. `crypto._derive_key` is
cached on (password, salt), so after the fixture no test pays any PBKDF2 at
all: ~1.6 s once, ~2 ms per test.
"""

import errno
import json
import shutil
from pathlib import Path

import pytest

pytest.importorskip("cryptography")

from numera import crypto, vault, vault_recovery       # noqa: E402
from numera.store import ProfileStore                  # noqa: E402

OLD_PW = "master-one"
NEW_PW = "master-two"

#: One credential per site in `vault.SITES`, so every commit boundary is
#: checked on all three - the engine profile, the dashboard's copy of it and
#: the shared pool - rather than on whichever one the migration happened to
#: reach first.
SECRETS = {"engine": "engine-secret", "dash": "dash-secret", "pool": "pool-secret"}


def _plant(root: Path) -> None:
    """The three secret-bearing files, in the shapes `vault.SITES` declares."""
    (root / "profiles").mkdir(parents=True, exist_ok=True)
    (root / "dashboard").mkdir(parents=True, exist_ok=True)
    (root / "profiles" / "p1.json").write_text(json.dumps(
        {"id": "p1", "name": "one",
         "proxy": {"server": "http://h:1", "password": SECRETS["engine"]}}),
        encoding="utf-8")
    (root / "dashboard" / "p1.json").write_text(json.dumps(
        {"id": "p1", "name": "one",
         "proxy": {"server": "http://h:1", "pass": SECRETS["dash"]}}),
        encoding="utf-8")
    (root / "proxies.json").write_text(json.dumps(
        [{"id": "x", "server": "http://h:2", "pass": SECRETS["pool"]}]),
        encoding="utf-8")


def stored(root: Path) -> dict:
    """Every stored secret, keyed by site, whatever state it is in."""
    out = {}
    for path, site in vault.secret_files(Path(root)):
        data = json.loads(path.read_text(encoding="utf-8"))
        for holder in vault.holders(data, site):
            out[{"profiles": "engine", "dashboard": "dash"}.get(
                path.parent.name, "pool")] = holder[site.field]
    return out


def unopenable(root: Path, password: str) -> list:
    """Sites whose secret does not open with the descriptor config names.

    The question the retry never asked. A store is consistent when this is
    empty and the plaintext is what was planted; anything else is a generation
    the active descriptor does not describe.
    """
    desc = (ProfileStore.get_config(_At(root)) or {}).get("vault") or {}
    bad = []
    for site, value in stored(root).items():
        try:
            if crypto.decrypt(password, desc["salt"], value) != SECRETS[site]:
                bad.append(site)
        except Exception:                                      # noqa: BLE001
            bad.append(site)
    return sorted(bad)


class _At:
    """The two attributes `ConfigStore.get_config` needs, without a store.

    Building a `ProfileStore` runs recovery, which is the thing under test -
    reading the config must not be able to change what the test is looking at.
    """

    def __init__(self, root):
        self.root = Path(root)
        self.config_path = Path(root) / "config.json"


def config_of(root) -> dict:
    return ProfileStore.get_config(_At(root))


@pytest.fixture(scope="module")
def bench(tmp_path_factory):
    """One enabled vault and one rotation target, both paid for once.

    Returns the pristine post-`enable` root plus both descriptors. Tests copy
    the directory; nothing re-derives a key.
    """
    root = tmp_path_factory.mktemp("vaulted")
    _plant(root)
    store = ProfileStore(root)
    store.enable_vault(OLD_PW)
    old = dict(config_of(root)["vault"])

    new_salt = crypto.new_salt()
    new = {"salt": new_salt, "verifier": crypto.make_verifier(NEW_PW, new_salt)}
    return {"pristine": root, "old": old, "new": new}


@pytest.fixture
def rotating(tmp_path, bench):
    """A rotation that was journalled and then died before it moved anything.

    Every secret is still under the old key, the config still names the old
    descriptor, and the journal holds both - the state a resume is for.
    """
    root = tmp_path / "root"
    shutil.copytree(bench["pristine"], root)
    vault.begin_journal(root, "rotate", new=bench["new"], old=bench["old"])
    record = json.loads(vault.journal_path(root).read_text(encoding="utf-8"))
    record["pid"] = None                 # nobody is finishing it
    vault.journal_path(root).write_text(json.dumps(record), encoding="utf-8")
    return root


def resume(root, **kw):
    """Run recovery the way the store's constructor does."""
    cfg = config_of(root)

    def save(new_cfg):
        (Path(root) / "config.json").write_text(
            json.dumps(new_cfg), encoding="utf-8")
        return new_cfg

    return vault.recover(root, lambda: config_of(root), save, **kw)


def break_sync(monkeypatch, name: str, code: int = errno.EIO):
    """Make a real disk error come out of syncing one file, persistently.

    A device that fails to flush usually keeps failing, so the injection is not
    one-shot: the rollback's own sync hits it too, and the honest outcome there
    is `RollbackFailed` with the journal kept. Either way the invariant under
    test is the same one - the descriptor on disk describes the data on disk.
    """
    real = vault_recovery.fsync_path

    def injected(path, **kw):
        if Path(path).name == name:
            raise OSError(code, "injected I/O error")
        return real(path, **kw)

    monkeypatch.setattr(vault_recovery, "fsync_path", injected)


def failed_resume(root, **kw) -> BaseException:
    """Run a resume that is expected to fail, and hand back what it raised."""
    try:
        resume(root, **kw)
    except Exception as exc:                                   # noqa: BLE001
        return exc
    raise AssertionError("the injected disk error did not surface at all")


# ---------------------------------------------------------------------------
# 1. The reproduction: config and data are one transaction
# ---------------------------------------------------------------------------

def test_a_sync_failure_after_the_descriptor_lands_does_not_split_the_two(
        rotating, bench, monkeypatch):
    """The regression, at its first move.

    `save_config` had already replaced the descriptor when the sync failed.
    Rolling the files back alone left the config naming a generation the disk
    was not in.
    """
    break_sync(monkeypatch, "config.json")

    failed_resume(rotating, password=NEW_PW, old_password=OLD_PW)

    active = config_of(rotating).get("vault") or {}
    assert active.get("salt") in (bench["old"]["salt"], bench["new"]["salt"])
    password = OLD_PW if active.get("salt") == bench["old"]["salt"] else NEW_PW
    assert unopenable(rotating, password) == [], (
        "the descriptor in config does not describe the files on disk: "
        f"config names {active.get('salt', '')[:8]!r}")


def test_the_retry_after_that_failure_does_not_report_success(
        rotating, bench, monkeypatch):
    """The second move, and the one that loses the data.

    A matching salt is not a completed transaction. Recovery used to take it as
    one, drop the journal and answer True - after which nothing on disk could
    ever be opened again, and the only record of the salt it was under was
    gone.
    """
    break_sync(monkeypatch, "config.json")
    failed_resume(rotating, password=NEW_PW, old_password=OLD_PW)
    monkeypatch.undo()

    # The retry a restart performs: no password to hand.
    try:
        resume(rotating)
    except vault.VaultRecoveryRequired:
        assert vault.journal_path(rotating).exists(), \
            "the refusal threw away the record of the salt"
    else:
        assert unopenable(rotating, NEW_PW) == [], \
            "recovery reported success over records that do not open"


def test_two_restarts_converge_on_one_readable_generation(
        rotating, bench, monkeypatch):
    """Whatever the first restart decides, the second must agree with it.

    The acceptance case: the outcome of any interrupted migration is one
    consistent generation or an explicit refusal - never a store that answers
    differently depending on how many times it has been opened.
    """
    break_sync(monkeypatch, "config.json")
    failed_resume(rotating, password=NEW_PW, old_password=OLD_PW)
    monkeypatch.undo()

    first = resume(rotating, password=NEW_PW, old_password=OLD_PW)
    second = resume(rotating, password=NEW_PW, old_password=OLD_PW)

    assert (first, second) == (True, False) or (first, second) == (True, True)
    assert not vault.journal_path(rotating).exists()
    assert unopenable(rotating, NEW_PW) == []
    assert config_of(rotating)["vault"]["salt"] == bench["new"]["salt"]


# ---------------------------------------------------------------------------
# 2. A clean resume, and the partial one
# ---------------------------------------------------------------------------

def test_a_clean_resume_moves_every_site(rotating, bench):
    """The control. Three credentials, three sites, one generation."""
    assert resume(rotating, password=NEW_PW, old_password=OLD_PW) is True

    assert config_of(rotating)["vault"]["salt"] == bench["new"]["salt"]
    assert unopenable(rotating, NEW_PW) == []
    assert not vault.journal_path(rotating).exists()


def test_a_half_moved_rotation_finishes_the_rest(rotating, bench):
    """Records already under the new key are indistinguishable from waiting
    ones - both are ciphertext - so a resume must try the new key first."""
    pool = Path(rotating) / "proxies.json"
    data = json.loads(pool.read_text(encoding="utf-8"))
    data[0]["pass"] = crypto.encrypt(NEW_PW, bench["new"]["salt"], SECRETS["pool"])
    pool.write_text(json.dumps(data), encoding="utf-8")

    assert resume(rotating, password=NEW_PW, old_password=OLD_PW) is True
    assert unopenable(rotating, NEW_PW) == []


@pytest.mark.parametrize("password,old_password,fragment", [
    (NEW_PW, "wrong-old", "moving away from"),
    ("wrong-new", OLD_PW, "new master password"),
])
def test_a_wrong_password_refuses_and_keeps_the_record(
        rotating, password, old_password, fragment):
    with pytest.raises(crypto.VaultLocked, match=fragment):
        resume(rotating, password=password, old_password=old_password)

    assert vault.journal_path(rotating).exists()
    assert unopenable(rotating, OLD_PW) == [], "a refusal changed the data"


# ---------------------------------------------------------------------------
# 3. Durability ordering, on the ordinary paths too
# ---------------------------------------------------------------------------

def _record_syncs(monkeypatch) -> list:
    seen = []
    real = vault_recovery.fsync_path

    def watched(path, **kw):
        seen.append(Path(path).name)
        return real(path, **kw)

    monkeypatch.setattr(vault_recovery, "fsync_path", watched)
    return seen


def test_enable_syncs_the_data_and_the_descriptor_not_only_the_journal(
        tmp_path, monkeypatch):
    """The ordinary caller had the journal's guarantee and none of the rest.

    Syncing the plan proves the plan survived a power cut, not the work it
    described: the descriptor could commit while the ciphertext it refers to
    was still in the page cache.
    """
    root = tmp_path / "fresh"
    _plant(root)
    seen = _record_syncs(monkeypatch)

    ProfileStore(root).enable_vault(OLD_PW)

    assert "vault-migration.json" in seen
    assert "proxies.json" in seen, f"the data was never synced: {seen}"
    assert "config.json" in seen, f"the descriptor was never synced: {seen}"
    assert seen.index("proxies.json") < seen.index("config.json"), \
        f"the descriptor was pushed to disk before the data it names: {seen}"


def test_rotation_syncs_the_data_and_the_descriptor(tmp_path, bench, monkeypatch):
    root = tmp_path / "rot"
    shutil.copytree(bench["pristine"], root)
    seen = _record_syncs(monkeypatch)

    ProfileStore(root, password=OLD_PW).change_vault_password(OLD_PW, NEW_PW)

    assert {"vault-migration.json", "proxies.json", "config.json"} <= set(seen)
    assert seen.index("proxies.json") < seen.index("config.json")


# ---------------------------------------------------------------------------
# 4. What a sync error is, and is not
# ---------------------------------------------------------------------------

@pytest.mark.parametrize("code", [errno.EACCES, errno.EBADF, errno.EIO])
def test_a_regular_file_sync_failure_is_a_failure(tmp_path, monkeypatch, code):
    """`EACCES` and `EBADF` are the documented answer for a *directory handle*
    on Windows. On an ordinary file they are a real error, and tolerating them
    there is how a migration reported durability it did not have."""
    target = tmp_path / "f.json"
    target.write_text("{}", encoding="utf-8")

    def fails(fd):
        raise OSError(code, "injected")

    monkeypatch.setattr(vault_recovery.os, "fsync", fails)

    with pytest.raises(OSError) as caught:
        vault.fsync_path(target, directory=False)
    assert caught.value.errno == code


@pytest.mark.parametrize("code", [errno.EINVAL, errno.ENOTSUP])
def test_a_filesystem_that_cannot_sync_this_object_is_tolerated(
        tmp_path, monkeypatch, code):
    target = tmp_path / "f.json"
    target.write_text("{}", encoding="utf-8")
    monkeypatch.setattr(vault_recovery.os, "fsync",
                        lambda fd: (_ for _ in ()).throw(OSError(code, "no")))

    vault.fsync_path(target, directory=False)          # must not raise


# ---------------------------------------------------------------------------
# 5. What the journal and the secret files have to look like
# ---------------------------------------------------------------------------

@pytest.mark.parametrize("mutate,why", [
    (lambda r: r["new"].pop("verifier"), "a descriptor with no verifier"),
    (lambda r: r["new"].update(verifier=7), "a non-string verifier"),
    (lambda r: r.update(version=0), "version 0"),
    (lambda r: r.update(version=True), "a boolean version"),
    (lambda r: r.update(version="1"), "a string version"),
])
def test_a_journal_this_build_cannot_trust_is_not_acted_on(
        rotating, mutate, why):
    """A salt is not a descriptor. Accepting one that carries nothing else let
    recovery commit a vault no password could be checked against."""
    path = vault.journal_path(rotating)
    record = json.loads(path.read_text(encoding="utf-8"))
    mutate(record)
    path.write_text(json.dumps(record), encoding="utf-8")

    state, _ = vault.classify(rotating)

    assert state in ("unreadable", "unsupported"), f"{why} classified {state!r}"
    with pytest.raises(vault.JournalUnreadable):
        resume(rotating, password=NEW_PW, old_password=OLD_PW)
    assert path.exists(), "the evidence was deleted"


@pytest.mark.parametrize("name,body", [
    ("proxies.json", "null"),
    ("proxies.json", '{"not": "a list"}'),
    ("profiles/p1.json", "[]"),
    ("profiles/p1.json", "12"),
])
def test_a_secret_file_of_the_wrong_shape_blocks_completion(
        rotating, name, body):
    """`null` parses. It is not a pool, and a scan that walks it finds no
    secrets - which read as "the migration finished" over a file whose contents
    nobody could establish."""
    (Path(rotating) / name).write_text(body, encoding="utf-8")

    assert [p.name for p in vault.unreadable_secret_files(rotating)], \
        "a container of the wrong shape was accepted as readable"
    with pytest.raises(vault.VaultRecoveryRequired, match=Path(name).name):
        resume(rotating, password=NEW_PW, old_password=OLD_PW)
    assert vault.journal_path(rotating).exists()


# ---------------------------------------------------------------------------
# 6. Rollback and cleanup
# ---------------------------------------------------------------------------

def test_a_failed_rollback_keeps_the_journal_and_names_the_files(
        rotating, monkeypatch):
    """The one state worse than a failed migration: half of it restored, with
    the journal - the only copy of the salt - thrown away."""
    from numera import paths

    real = paths._restore_bytes

    def cannot(path, data):
        if Path(path).name == "proxies.json":
            raise OSError(errno.EACCES, "read-only")
        return real(path, data)

    monkeypatch.setattr(paths, "_restore_bytes", cannot)
    break_sync(monkeypatch, "config.json")

    with pytest.raises(paths.RollbackFailed) as caught:
        resume(rotating, password=NEW_PW, old_password=OLD_PW)

    assert "proxies.json" in str(caught.value)
    assert vault.journal_path(rotating).exists(), \
        "a failed rollback deleted the record of the salt"


def test_a_migration_that_only_missed_its_cleanup_finishes_quietly(
        rotating, bench):
    """Committed, synced, then killed before the journal went. There is nothing
    left to do but prove it and drop the record."""
    resume(rotating, password=NEW_PW, old_password=OLD_PW)
    vault.begin_journal(rotating, "rotate", new=bench["new"], old=bench["old"])
    record = json.loads(vault.journal_path(rotating).read_text(encoding="utf-8"))
    record["pid"] = None
    vault.journal_path(rotating).write_text(json.dumps(record), encoding="utf-8")

    assert resume(rotating, password=NEW_PW) is True
    assert not vault.journal_path(rotating).exists()
    assert unopenable(rotating, NEW_PW) == []


def test_a_store_opens_normally_after_the_protocol_settles(rotating, bench):
    """The end-to-end control: the real constructor, the real descriptor, and
    a secret that actually decrypts through the store's own reader."""
    resume(rotating, password=NEW_PW, old_password=OLD_PW)

    store = ProfileStore(rotating, password=NEW_PW)

    assert store.unlocked() is True
    assert store.vault()["salt"] == bench["new"]["salt"]
    assert vault.plaintext_secrets(rotating) == []
