"""One completion rule, and what it refuses.

Every vault path used to decide "finished" its own cheap way, and each cheap way
was wrong in a different direction. All three were reproduced at the real
boundary with real crypto before anything changed:

* **Foreign ciphertext.** The enable shortcut asked only "is any plaintext
  left". Three secrets re-encrypted under a *different* descriptor answered no,
  so recovery committed a descriptor that opened none of them and deleted the
  journal - the only record of the salt they were actually under. Measured:
  `recover()` returned `True`, and afterwards nothing on disk opened with any
  password at all.
* **A damaged verifier.** A nonempty verifier that decrypts to nothing passed
  the shape check and was committed without ever being used, leaving a vault
  whose password could not be checked by anyone. Measured: `recover()` returned
  `True` and `check_password` rejected the committed verifier.
* **Plaintext skipped by a rotation.** `rekey_all` moves ciphertext and steps
  over plaintext, which is right for a rotation and wrong for a resume - so a
  pool password an interrupted enable never reached stayed readable and the
  rotation reported success. Measured in both shapes: with the config already
  naming the new key, and without.

The oracle here is never the production scanner. Every credential is decrypted
with the descriptor the config actually names and compared to the value the
fixture planted; the verifier is checked; every registered site is swept for
unprotected values; and the store is reopened twice.

**Cost.** Real decryptable credentials are the whole point, and PBKDF2 at
600,000 rounds is ~0.5s per distinct salt. The module pays for exactly two, in
a module-scoped fixture, and every test copies that directory instead of
building a vault. `crypto._derive_key` is cached on (password, salt), so after
the fixture no test pays any PBKDF2 at all.
"""

import json
import shutil

import pytest

pytest.importorskip("cryptography")

from numera import crypto, vault                      # noqa: E402
from numera.paths import write_json                   # noqa: E402
from numera.store import ProfileStore                 # noqa: E402

PW_A, PW_B = "master-alpha", "master-bravo"

#: One credential per site in `vault.SITES`, so every commit boundary is
#: checked on all three - the engine profile, the dashboard's copy of it and
#: the shared pool - rather than on whichever the migration reached first.
SECRETS = {"engine": "engine-secret", "dash": "dash-secret", "pool": "pool-secret"}


def _site_key(path) -> str:
    return {"profiles": "engine", "dashboard": "dash"}.get(path.parent.name, "pool")


def plant(root) -> None:
    """The three secret-bearing files, through the writer the product uses."""
    (root / "profiles").mkdir(parents=True, exist_ok=True)
    (root / "dashboard").mkdir(parents=True, exist_ok=True)
    write_json(root / "profiles" / "p1.json",
               {"id": "p1", "name": "one",
                "proxy": {"server": "http://h:1", "password": SECRETS["engine"]}})
    write_json(root / "dashboard" / "p1.json",
               {"id": "p1", "name": "one",
                "proxy": {"server": "http://h:1", "pass": SECRETS["dash"]}})
    write_json(root / "proxies.json",
               [{"id": "x", "server": "http://h:2", "pass": SECRETS["pool"]}])


def config_of(root) -> dict:
    try:
        return json.loads((root / "config.json").read_text(encoding="utf-8"))
    except OSError:
        return {}


def recover(root, **kw):
    def save(new_cfg):
        write_json(root / "config.json", new_cfg)
        return new_cfg

    return vault.recover(root, lambda: config_of(root), save, **kw)


def assert_fully_recovered(root, password) -> None:
    """The independent oracle. Never `plaintext_secrets` or `unconverted`.

    Decrypts every stored credential with the descriptor the config names and
    compares it to the value the fixture planted, checks the verifier, counts
    the holders, and sweeps every registered site for anything unprotected.
    """
    descriptor = config_of(root).get("vault") or {}
    assert descriptor, "no vault descriptor was committed"
    assert crypto.check_password(password, descriptor["salt"],
                                 descriptor.get("verifier")), \
        "the committed verifier does not open with this password"

    seen = {}
    for path, site in vault.secret_files(root):
        data = json.loads(path.read_text(encoding="utf-8"))
        for holder in vault.holders(data, site):
            key = _site_key(path)
            value = holder[site.field]
            assert crypto.is_encrypted(value), \
                f"{path.name} still holds an unprotected value for {key}"
            seen[key] = crypto.decrypt(password, descriptor["salt"], value)
    assert seen == SECRETS, (
        f"the stored credentials are not the ones that were planted: {seen}")


@pytest.fixture(scope="module")
def bench(tmp_path_factory):
    """One enabled vault under A, and one independent descriptor B.

    Both salts are fixed for the module, so the key cache is warm after this
    and no later test pays PBKDF2.
    """
    root = tmp_path_factory.mktemp("vaulted")
    plant(root)
    ProfileStore(root).enable_vault(PW_A)
    salt_b = crypto.new_salt()
    return {
        "pristine": root,
        "a": dict(config_of(root)["vault"]),
        "b": {"salt": salt_b, "verifier": crypto.make_verifier(PW_B, salt_b)},
    }


@pytest.fixture
def root(tmp_path, bench):
    made = tmp_path / "root"
    shutil.copytree(bench["pristine"], made)
    return made


def journal(root, op, **kw) -> None:
    """Write a journal nobody owns, so recovery treats it as abandoned."""
    vault.begin_journal(root, op, **kw)
    record = json.loads(vault.journal_path(root).read_text(encoding="utf-8"))
    record["pid"] = None
    write_json(vault.journal_path(root), record)


def reencrypt_all(root, password, salt) -> None:
    for path, site in vault.secret_files(root):
        data = json.loads(path.read_text(encoding="utf-8"))
        for holder in vault.holders(data, site):
            holder[site.field] = crypto.encrypt(password, salt,
                                                SECRETS[_site_key(path)])
        write_json(path, data)


def drop_vault(root) -> None:
    cfg = config_of(root)
    cfg.pop("vault", None)
    write_json(root / "config.json", cfg)



def assert_refused(call, *, cause, root):
    """Run `call`, and assert the refusal by its **cause**, not its sentence.

    Pinning wording is the defect this replaces. `completion_problems` can
    return several reasons and which one surfaces first depends on the order
    they are appended, so a test that matches a phrase passes wherever that
    phrase happens to be produced and fails where an equally correct refusal
    arrives by another route - and the routes differ by what is on disk. That
    is green exactly where it is least useful.

    What is asserted instead is the contract: it refused, it named this cause,
    it committed nothing, and it kept the evidence.

    "Committed nothing" is the config being *unchanged*, not the config having
    no vault. A resumed rotation starts from a root that is legitimately
    serving under the old descriptor, so "no vault in the config" is an
    invariant only an interrupted enable has, and asserting it here would have
    made the rotation cases pass for the wrong reason - or, as it did, fail for
    one.
    """
    before = config_of(root)

    with pytest.raises(vault.VaultIncomplete) as caught:
        call()

    assert cause in caught.value.codes, (
        f"expected cause {cause!r}, got {sorted(caught.value.codes)} - "
        f"{caught.value}")
    assert config_of(root) == before, \
        "a refused migration changed the config it promised not to touch"
    assert vault.journal_path(root).exists(), \
        "the refusal deleted the record of the descriptor"
    return caught.value


# ---------------------------------------------------------------------------
# N02.A - an envelope prefix is not a generation
# ---------------------------------------------------------------------------

def test_ciphertext_from_a_foreign_descriptor_is_refused(root, bench):
    """The regression. Every secret is encrypted, and none of it is ours."""
    reencrypt_all(root, PW_B, bench["b"]["salt"])
    drop_vault(root)
    journal(root, "enable", new=bench["a"])

    with pytest.raises(vault.VaultRecoveryRequired) as caught:
        recover(root, password=PW_A)

    assert "does not open" in str(caught.value)
    assert "proxies.json" in str(caught.value), \
        f"the refusal does not name the files: {caught.value}"
    assert config_of(root).get("vault") is None, \
        "a descriptor was committed for data it does not describe"
    assert vault.journal_path(root).exists(), \
        "the record of the descriptor those secrets belong to was deleted"


def test_the_foreign_data_is_still_recoverable_afterwards(root, bench):
    """Refusing must not be destructive: the ciphertext is intact and still
    opens with the key it was written under."""
    reencrypt_all(root, PW_B, bench["b"]["salt"])
    drop_vault(root)
    journal(root, "enable", new=bench["a"])
    with pytest.raises(vault.VaultRecoveryRequired):
        recover(root, password=PW_A)

    for path, site in vault.secret_files(root):
        data = json.loads(path.read_text(encoding="utf-8"))
        for holder in vault.holders(data, site):
            assert crypto.decrypt(PW_B, bench["b"]["salt"],
                                  holder[site.field]) == SECRETS[_site_key(path)]


def test_an_enable_whose_data_really_is_ours_completes(root, bench):
    """The control. The stricter rule must not refuse a healthy finish."""
    drop_vault(root)
    journal(root, "enable", new=bench["a"])

    assert recover(root, password=PW_A) is True

    assert_fully_recovered(root, PW_A)
    assert not vault.journal_path(root).exists()


def test_without_a_password_an_interrupted_enable_refuses(root, bench):
    """A key-free completion proof would have to establish the generation, and
    a prefix scan cannot. So it refuses, keeps the evidence, and says why."""
    drop_vault(root)
    journal(root, "enable", new=bench["a"])

    assert_refused(lambda: recover(root),
                   cause=vault.NO_PASSWORD, root=root)


# ---------------------------------------------------------------------------
# N02.B - a descriptor has to be usable, not merely present
# ---------------------------------------------------------------------------

def test_a_damaged_verifier_is_never_committed(root, bench):
    """Nonempty, well-formed, and it opens nothing. It used to be committed
    without being used once."""
    drop_vault(root)
    damaged = dict(bench["a"])
    damaged["verifier"] = crypto.encrypt(PW_B, bench["b"]["salt"],
                                         "numera-vault-ok")
    assert not crypto.check_password(PW_A, damaged["salt"], damaged["verifier"])
    journal(root, "enable", new=damaged)

    assert_refused(lambda: recover(root, password=PW_A),
                   cause=vault.VERIFIER_MISMATCH, root=root)


@pytest.mark.parametrize("mutate,why", [
    (lambda d: d.update(verifier=""), "an empty verifier"),
    (lambda d: d.update(verifier=None), "a null verifier"),
    (lambda d: d.update(verifier=17), "a non-string verifier"),
    (lambda d: d.update(salt="not-base64!!"), "a salt that is not base64"),
])
def test_an_unusable_descriptor_is_refused(root, bench, mutate, why):
    """Refused before the completion check, and refused *harder*.

    `classify` rejects these while reading the journal, so `recover` never
    reaches the key check and raises `JournalUnreadable` rather than
    `VaultIncomplete`. That is the stronger refusal and the right one: a
    descriptor with no usable salt or verifier cannot be finished by supplying
    a password - the thing that is missing is not the key - so offering one is
    the wrong instruction to give.
    """
    drop_vault(root)
    descriptor = dict(bench["a"])
    mutate(descriptor)
    journal(root, "enable", new=descriptor)
    before = config_of(root)

    with pytest.raises(vault.JournalUnreadable):
        recover(root, password=PW_A)

    assert config_of(root) == before, f"{why} was committed anyway"
    assert vault.journal_path(root).exists(), \
        f"{why} had its evidence deleted"

    # The cause code still has to exist and name itself, because
    # `structural_problems` is public: a caller holding a descriptor from
    # somewhere other than a journal reaches this branch without `classify`
    # having filtered it first, and everything below it would otherwise run
    # `check_password` against a salt that does not decode.
    problems = vault.completion_problems(root, descriptor, PW_A)
    assert vault.codes(problems) == {vault.DESCRIPTOR_UNUSABLE}, \
        f"{why} produced {problems}"


# ---------------------------------------------------------------------------
# N02.C - a rotation may not step over plaintext
# ---------------------------------------------------------------------------

@pytest.mark.parametrize("config_already_new", [False, True],
                         ids=["ordinary resume", "config already names the new key"])
def test_a_resume_does_not_finish_over_a_readable_credential(
        root, bench, config_already_new):
    """Both shapes of the same defect.

    The pool password is still in the clear - an interrupted enable never
    reached it - and the rotation used to move everything else around it and
    report success.
    """
    pool = json.loads((root / "proxies.json").read_text(encoding="utf-8"))
    pool[0]["pass"] = SECRETS["pool"]
    write_json(root / "proxies.json", pool)
    journal(root, "rotate", new=bench["b"], old=bench["a"])
    if config_already_new:
        cfg = config_of(root)
        cfg["vault"] = dict(bench["b"])
        write_json(root / "config.json", cfg)

    assert recover(root, password=PW_B, old_password=PW_A) is True

    assert_fully_recovered(root, PW_B)
    assert not vault.journal_path(root).exists()


def test_a_clean_resume_moves_every_site(root, bench):
    """The control: all three credentials, one generation, verified by value."""
    journal(root, "rotate", new=bench["b"], old=bench["a"])

    assert recover(root, password=PW_B, old_password=PW_A) is True

    assert_fully_recovered(root, PW_B)


@pytest.mark.parametrize("password,old_password,role", [
    (PW_B, "wrong-old", "old"),
    ("wrong-new", PW_A, "new"),
])
def test_a_wrong_password_refuses_and_changes_nothing(
        root, bench, password, old_password, role):
    """Which password was rejected is read off the error, not its sentence."""
    journal(root, "rotate", new=bench["b"], old=bench["a"])

    with pytest.raises(crypto.VaultLocked) as caught:
        recover(root, password=password, old_password=old_password)

    assert getattr(caught.value, "role", None) == role, (
        f"expected the {role!r} password to be the rejected one, got "
        f"{getattr(caught.value, 'role', None)!r}: {caught.value}")

    assert vault.journal_path(root).exists()
    assert_fully_recovered(root, PW_A)      # still the old generation, intact


def test_a_resume_with_no_passwords_refuses_and_keeps_the_salt(root, bench):
    journal(root, "rotate", new=bench["b"], old=bench["a"])

    error = assert_refused(lambda: recover(root),
                           cause=vault.NEEDS_BOTH_PASSWORDS, root=root)
    assert "resume" in str(error), \
        "the refusal does not tell the user which command finishes it"


# ---------------------------------------------------------------------------
# Divergence, containers, and reopening
# ---------------------------------------------------------------------------

def test_a_new_config_over_old_data_is_not_a_finished_rotation(root, bench):
    """The descriptor says one generation and every file is in the other. A
    matching salt was the whole evidence before."""
    cfg = config_of(root)
    cfg["vault"] = dict(bench["b"])         # names B; every secret is under A
    write_json(root / "config.json", cfg)
    journal(root, "rotate", new=bench["b"], old=bench["a"])

    assert recover(root, password=PW_B, old_password=PW_A) is True

    assert_fully_recovered(root, PW_B)


@pytest.mark.parametrize("name,body", [
    ("proxies.json", "null"),
    ("proxies.json", '{"not": "a list"}'),
    ("profiles/p1.json", "[]"),
])
def test_a_malformed_holder_container_blocks_completion(root, bench, name, body):
    """`null` parses, holds no secrets to find, and read as a finished
    migration."""
    drop_vault(root)
    (root / name).write_text(body, encoding="utf-8")
    journal(root, "enable", new=bench["a"])

    assert_refused(lambda: recover(root, password=PW_A),
                   cause=vault.UNREADABLE_FILES, root=root)


def test_a_root_with_no_secrets_at_all_completes(tmp_path, bench):
    """The empty control: nothing to encrypt is a legitimate finished enable."""
    bare = tmp_path / "bare"
    (bare / "profiles").mkdir(parents=True)
    write_json(bare / "config.json", {})
    journal(bare, "enable", new=bench["a"])

    assert recover(bare, password=PW_A) is True

    assert config_of(bare)["vault"]["salt"] == bench["a"]["salt"]
    assert not vault.journal_path(bare).exists()


def test_the_store_opens_twice_after_the_protocol_settles(root, bench):
    """Reopened twice, through the real constructor, as the acceptance asks -
    a second open must not find anything the first one left behind."""
    journal(root, "rotate", new=bench["b"], old=bench["a"])
    recover(root, password=PW_B, old_password=PW_A)

    first = ProfileStore(root, password=PW_B)
    second = ProfileStore(root, password=PW_B)

    assert first.unlocked() and second.unlocked()
    assert second.vault()["salt"] == bench["b"]["salt"]
    assert_fully_recovered(root, PW_B)
