"""A damaged recovery record is not an absent one, and a file you cannot read
is not a file with nothing in it.

`0.32.0` made an interrupted migration finish or refuse. It decided *whether*
there was one by asking a single question - does this file parse into a dict
with a salt - which collapses four states into two:

* absent - nothing happened, or everything finished;
* valid - a record this build can act on;
* **unreadable** - invalid JSON, `[]`, `{}`, a salt that is missing or not a
  string, a rotation with no old descriptor;
* **unsupported** - a format or operation a newer build wrote.

The last two answered "nothing to do", so a store holding an unresolved
migration opened as though nothing had happened - and calling `recover`
directly on one would delete it, destroying the only record of the salt its
ciphertext is under.

The second defect is in the completion check. `plaintext_secrets` skips a
secret file it cannot parse, which is right for its own job ("prove nothing
readable is left") and wrong for deciding whether an interrupted `enable`
finished: a corrupt `proxies.json` made recovery conclude the migration was
complete and commit the descriptor over it.

The third is durability. `_fsync` returned quietly on any `OSError`, so a
failing disk produced a migration that reported success with nothing durably
written, and only the journal was ever synced - never the ciphertext it
describes, nor the descriptor before the journal was removed.

**These run in-process on purpose.** The kill-barrier cases in
`test_vault_recovery_completeness.py` need real `os._exit` children because
they are about what `except` does not run. Nothing here is: every case is about
how recovery *reads* state, so a temporary directory and a written file are the
whole fixture and the file costs milliseconds rather than a Python interpreter
per case.
"""

import errno
import json

import pytest

from numera import vault, vault_recovery

GOOD = {"salt": "c2FsdA==", "verifier": "v"}


def journal(tmp_path, payload):
    """Write `payload` as the journal; a str goes down verbatim."""
    path = vault.journal_path(tmp_path)
    path.write_text(payload if isinstance(payload, str) else json.dumps(payload),
                    encoding="utf-8")
    return path


def refuse(tmp_path, **kw):
    """Run recovery against a config with no vault, and return what it raised."""
    cfg = {}
    with pytest.raises(vault.VaultRecoveryRequired) as caught:
        vault.recover(tmp_path, lambda: cfg, lambda c: cfg.update(c), **kw)
    return caught.value


# ---------------------------------------------------------------------------
# 1. Four states, not two
# ---------------------------------------------------------------------------

@pytest.mark.parametrize("payload,why", [
    ("{not json at all", "invalid JSON"),
    ("[]", "a list"),
    ("{}", "an empty object"),
    ("null", "a null"),
    ({"op": "enable"}, "no salt"),
    ({"op": "enable", "salt": 17}, "a non-string salt"),
    ({"op": "enable", "salt": ""}, "an empty salt"),
    ({"op": "rotate", "salt": "c2FsdA==", "new": GOOD, "old": None}, "a rotation with no old key"),
])
def test_a_damaged_journal_is_unreadable_not_absent(tmp_path, payload, why):
    """The regression. Each of these used to read as 'nothing to recover'."""
    journal(tmp_path, payload)

    state, _ = vault.classify(tmp_path)

    assert state == "unreadable", f"a journal with {why} was classified {state!r}"
    assert vault.needs_recovery(tmp_path) is True


@pytest.mark.parametrize("payload", [
    {"version": 99, "op": "enable", "salt": "c2FsdA==", "new": GOOD},
    {"version": 1, "op": "reshard", "salt": "c2FsdA==", "new": GOOD},
])
def test_a_newer_journal_is_unsupported_not_absent(tmp_path, payload):
    """Acting on a record written by rules this build does not have is worse
    than refusing it."""
    journal(tmp_path, payload)

    state, _ = vault.classify(tmp_path)

    assert state == "unsupported"
    assert vault.needs_recovery(tmp_path) is True


def test_a_valid_journal_still_classifies_as_valid(tmp_path):
    """The control: none of the above may turn an ordinary recovery into a
    refusal."""
    journal(tmp_path, {"version": 1, "op": "enable", "salt": GOOD["salt"],
                       "new": GOOD, "old": None})

    assert vault.classify(tmp_path)[0] == "valid"


def test_no_journal_is_still_absent(tmp_path):
    assert vault.classify(tmp_path) == ("absent", None)
    assert vault.needs_recovery(tmp_path) is False


# ---------------------------------------------------------------------------
# 2. Evidence is never destroyed by inspecting or refusing
# ---------------------------------------------------------------------------

@pytest.mark.parametrize("payload", ["{not json", "[]", "{}"])
def test_recovery_refuses_a_damaged_journal_and_keeps_it(tmp_path, payload):
    """`recover` used to delete a wrong-shaped journal on sight."""
    path = journal(tmp_path, payload)

    err = refuse(tmp_path)

    assert path.exists(), "recovery deleted the damaged recovery record"
    assert isinstance(err, vault.JournalUnreadable)
    assert "backup" in str(err).lower() or "restore" in str(err).lower(), \
        f"the refusal gives the user nothing to do: {err}"


def test_classification_never_writes(tmp_path):
    """Inspection is read-only, so the damaged case cannot lose evidence to
    the act of looking at it."""
    path = journal(tmp_path, "{not json")
    before = path.read_bytes()

    vault.classify(tmp_path)
    vault.needs_recovery(tmp_path)

    assert path.read_bytes() == before


# ---------------------------------------------------------------------------
# 3. An unreadable secret file is not an encrypted one
# ---------------------------------------------------------------------------

def test_an_unreadable_secret_file_blocks_completion(tmp_path):
    """The scanner skips what it cannot parse; completion must not.

    Every parseable secret here is already ciphertext, so the old check saw
    "no plaintext left" and committed the descriptor - over a pool file whose
    contents nobody could establish.
    """
    (tmp_path / "profiles").mkdir()
    (tmp_path / "profiles" / "a.json").write_text(json.dumps(
        {"id": "a", "proxy": {"password": "enc:v2:abc"}}), encoding="utf-8")
    (tmp_path / "proxies.json").write_text("{truncated", encoding="utf-8")
    journal(tmp_path, {"version": 1, "op": "enable", "salt": GOOD["salt"], "new": GOOD})

    assert vault.plaintext_secrets(tmp_path) == [], "the fixture is not the one described"
    assert [p.name for p in vault.unreadable_secret_files(tmp_path)] == ["proxies.json"]

    err = refuse(tmp_path)

    assert "proxies.json" in str(err), f"the refusal does not name the file: {err}"
    assert vault.journal_path(tmp_path).exists(), "the evidence was cleared anyway"


def test_an_already_committed_descriptor_is_not_a_receipt_without_a_key(tmp_path):
    """0.36.0 removed the last shortcut, and this is what it cost.

    The reasoning behind it was that when the config already names the
    journal's descriptor nothing is being *chosen* - the store was serving
    under this key before the crash, so only the record is outstanding. It was
    wrong twice over, and both were measured:

    * a salt is not a descriptor. Ciphertext written under a different key with
      the same salt passed this path, and the descriptor it confirmed opened
      **0 of 3** fixture credentials;
    * only salts were compared, so a config verifier that opens nothing
      survived while the journal carrying the good one was deleted.

    So finishing needs the key, on every path. That is a real cost - this case
    used to complete unattended - and it is the only version that does not
    sometimes destroy the record of the generation the data is actually in.
    """
    (tmp_path / "profiles").mkdir()
    (tmp_path / "profiles" / "a.json").write_text(json.dumps(
        {"id": "a", "proxy": {"password": "enc:v2:abc"}}), encoding="utf-8")
    journal(tmp_path, {"version": 1, "op": "enable", "salt": GOOD["salt"], "new": GOOD})
    cfg = {"vault": dict(GOOD)}

    with pytest.raises(vault.VaultIncomplete) as caught:
        vault.recover(tmp_path, lambda: cfg, lambda c: cfg.update(c))

    assert vault.NO_PASSWORD in caught.value.codes, caught.value
    assert vault.journal_path(tmp_path).exists(), \
        "the record of the real descriptor was deleted on a refusal"
    assert cfg["vault"] == GOOD, "a refused recovery rewrote the config"


def test_a_config_that_disagrees_with_the_journal_is_named_not_guessed(tmp_path):
    """Same salt, different verifier - which used to read as "the same".

    Nothing here can choose between them: committing the journal's overwrites a
    descriptor the store may be serving, and accepting the config's discards
    the record of the one the data may be under. So the disagreement is stated
    and both are preserved, under a cause of its own because no password
    resolves it.
    """
    (tmp_path / "profiles").mkdir()
    (tmp_path / "profiles" / "a.json").write_text(json.dumps(
        {"id": "a", "proxy": {"password": "enc:v2:abc"}}), encoding="utf-8")
    journal(tmp_path, {"version": 1, "op": "enable", "salt": GOOD["salt"], "new": GOOD})
    stale = {"salt": GOOD["salt"], "verifier": "enc:v2:opens-nothing"}
    cfg = {"vault": stale}

    with pytest.raises(vault.VaultIncomplete) as caught:
        vault.recover(tmp_path, lambda: cfg, lambda c: cfg.update(c))

    assert vault.DESCRIPTOR_DISAGREEMENT in caught.value.codes, caught.value
    assert vault.journal_path(tmp_path).exists()
    assert cfg["vault"] == stale, "a refused recovery rewrote the config"


def test_an_uncommitted_descriptor_is_not_adopted_without_a_key(tmp_path):
    """The other half of the narrowing, kept here beside its control.

    No vault in the config means committing this descriptor is a decision, and
    "everything looks encrypted" does not establish that it is encrypted under
    *this* key.
    """
    (tmp_path / "profiles").mkdir()
    (tmp_path / "profiles" / "a.json").write_text(json.dumps(
        {"id": "a", "proxy": {"password": "enc:v2:abc"}}), encoding="utf-8")
    journal(tmp_path, {"version": 1, "op": "enable", "salt": GOOD["salt"], "new": GOOD})
    cfg = {}

    with pytest.raises(vault.VaultRecoveryRequired, match="no master password"):
        vault.recover(tmp_path, lambda: cfg, lambda c: cfg.update(c))

    assert "vault" not in cfg
    assert vault.journal_path(tmp_path).exists()


# ---------------------------------------------------------------------------
# 4. A sync failure is a failure
# ---------------------------------------------------------------------------

def test_a_real_sync_error_propagates(tmp_path, monkeypatch):
    """A full disk or a disconnected volume is not something to return from."""
    target = tmp_path / "f.json"
    target.write_text("{}", encoding="utf-8")

    def full(fd):
        raise OSError(errno.ENOSPC, "no space left on device")

    monkeypatch.setattr(vault_recovery.os, "fsync", full)

    with pytest.raises(OSError) as caught:
        vault.fsync_path(target)
    assert caught.value.errno == errno.ENOSPC


def test_an_unsupported_sync_is_tolerated(tmp_path, monkeypatch):
    """The one case that is genuinely not an error: a platform or filesystem
    that does not implement the call for this object."""
    target = tmp_path / "f.json"
    target.write_text("{}", encoding="utf-8")

    def unsupported(fd):
        raise OSError(errno.EINVAL, "invalid argument")

    monkeypatch.setattr(vault_recovery.os, "fsync", unsupported)

    vault.fsync_path(target)          # must not raise


def test_beginning_a_journal_syncs_it(tmp_path):
    """Durability ordering starts here: the plan is on stable storage before
    the first destructive write."""
    synced = []
    real = vault_recovery.fsync_path

    def watched(path, **kw):
        synced.append(str(path))
        return real(path, **kw)

    vault_recovery.fsync_path = watched
    try:
        vault.begin_journal(tmp_path, "enable", new=GOOD)
    finally:
        vault_recovery.fsync_path = real

    assert any("vault-migration" in s for s in synced), \
        "the journal was written without being pushed to stable storage"


def test_the_journal_records_its_format_version(tmp_path):
    """So a future build can refuse this record by name rather than misread it."""
    vault.begin_journal(tmp_path, "enable", new=GOOD)

    record = json.loads(vault.journal_path(tmp_path).read_text("utf-8"))

    assert record["version"] == vault.JOURNAL_VERSION
    assert record["op"] == "enable"


# ---------------------------------------------------------------------------
# 5. An already-open store stops using an unresolved root
# ---------------------------------------------------------------------------

def test_a_store_that_is_already_open_refuses_after_a_migration_dies(tmp_path):
    """The constructor check cannot see this: the store was built before the
    other process started."""
    from numera.store import ProfileStore

    store = ProfileStore(tmp_path)
    assert store.vault() is None, "the fixture started with a vault"

    # Another process begins a migration and dies. `pid` is this interpreter so
    # the liveness check cannot mistake it for a live owner elsewhere.
    journal(tmp_path, {"version": 1, "op": "enable", "salt": GOOD["salt"],
                       "new": GOOD, "pid": None})

    with pytest.raises(vault.VaultRecoveryRequired):
        store.vault()
