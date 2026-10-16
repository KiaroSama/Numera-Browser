"""Untrusted archives must not write outside the directory they belong to.

Two separate instances of the same mistake:

* `ProfileStore.restore()` checked only that a member began with `user-data/`
  and then joined the rest onto the profile directory, so `user-data/../../x`
  landed two levels up.
* the Web Store unpacker compared *strings* - `str(target).startswith(str(dest))`
  - which passes for a sibling whose name merely begins the same way, so a `.crx`
  could write into `<ext>-evil/` beside the directory it was given.

Both are tested against real archives rather than by reading the guard.
"""
import io
import json
import zipfile

import pytest

from numera import generate
from numera.models import Profile
from numera.store import ProfileStore


def archive(path, members, profile=None):
    """A backup-shaped zip, including whatever hostile members are asked for."""
    with zipfile.ZipFile(path, "w") as z:
        z.writestr("profile.json", json.dumps(
            (profile or Profile(name="restored", fingerprint=generate(os="windows"))).to_dict()))
        for name, data in members.items():
            z.writestr(name, data)
    return path


# --- ProfileStore.restore --------------------------------------------------

def test_a_normal_backup_still_round_trips(tmp_path):
    store = ProfileStore(tmp_path / "data")
    p = Profile(name="alpha", fingerprint=generate(os="windows"))
    store.save(p)
    (store.user_data_path(p.id) / "Default").mkdir(parents=True, exist_ok=True)
    (store.user_data_path(p.id) / "Default" / "Cookies").write_text("jar", encoding="utf-8")
    dest = tmp_path / "backup.zip"
    store.backup(p.id, dest)

    other = ProfileStore(tmp_path / "restored")
    back = other.restore(dest)
    assert back.name == "alpha"
    assert (other.user_data_path(back.id) / "Default" / "Cookies").read_text(
        encoding="utf-8") == "jar"


def test_a_parent_traversal_member_is_refused(tmp_path):
    store = ProfileStore(tmp_path / "data")
    z = archive(tmp_path / "evil.zip", {"user-data/../../../pwned.txt": b"x"})
    with pytest.raises(ValueError) as e:
        store.restore(z)
    assert "outside" in str(e.value).lower() or "escape" in str(e.value).lower()
    assert not (tmp_path / "pwned.txt").exists()
    assert not (tmp_path.parent / "pwned.txt").exists()


def test_a_leading_slash_is_neutralised_rather_than_escaping(tmp_path):
    store = ProfileStore(tmp_path / "data")
    z = archive(tmp_path / "odd.zip", {"user-data//etc/pwned": b"x"})
    prof = store.restore(z)
    assert (store.user_data_path(prof.id) / "etc" / "pwned").is_file()
    assert not (tmp_path / "etc").exists()


def test_a_windows_drive_member_is_refused(tmp_path):
    """`Path('a') / 'C:/x'` is `C:/x` on Windows - the join throws the base away."""
    store = ProfileStore(tmp_path / "data")
    z = archive(tmp_path / "evil.zip", {"user-data/C:/pwned.txt": b"x"})
    with pytest.raises(ValueError):
        store.restore(z)


def test_a_hostile_archive_leaves_no_half_written_profile(tmp_path):
    """The old order saved the profile first and wrote files after, so a bad
    archive left a profile behind that nothing had asked for."""
    store = ProfileStore(tmp_path / "data")
    before = {p.id for p in store.list()}
    z = archive(tmp_path / "evil.zip", {
        "user-data/Default/ok.txt": b"fine",
        "user-data/../../../pwned.txt": b"x",
    })
    with pytest.raises(ValueError):
        store.restore(z)
    assert {p.id for p in store.list()} == before, "a profile was created anyway"


def test_a_malformed_profile_json_is_a_clear_error(tmp_path):
    store = ProfileStore(tmp_path / "data")
    path = tmp_path / "bad.zip"
    with zipfile.ZipFile(path, "w") as z:
        z.writestr("profile.json", "{ not json")
    with pytest.raises(ValueError):
        store.restore(path)


def test_an_archive_with_no_profile_json_is_a_clear_error(tmp_path):
    store = ProfileStore(tmp_path / "data")
    path = tmp_path / "bad.zip"
    with zipfile.ZipFile(path, "w") as z:
        z.writestr("user-data/Default/x", b"x")
    with pytest.raises(ValueError):
        store.restore(path)


def test_restore_replaces_the_session_rather_than_merging_into_it(tmp_path):
    """Merging leaves files from the session being replaced - a cookie jar that
    is half one identity and half another is worse than either."""
    store = ProfileStore(tmp_path / "data")
    p = Profile(name="alpha", fingerprint=generate(os="windows"))
    store.save(p)
    ud = store.user_data_path(p.id)
    ud.mkdir(parents=True, exist_ok=True)
    (ud / "stale.txt").write_text("from the old session", encoding="utf-8")

    z = archive(tmp_path / "b.zip", {"user-data/fresh.txt": b"new"}, profile=p)
    store.restore(z)
    assert (store.user_data_path(p.id) / "fresh.txt").exists()
    assert not (store.user_data_path(p.id) / "stale.txt").exists(), \
        "the previous session's files survived the restore"


# --- Web Store unpack ------------------------------------------------------

def test_a_sibling_prefix_member_cannot_escape_the_extension_folder(tmp_path):
    """`.../ext-evil/x` starts with `.../ext`, so a string comparison lets it
    through while a path comparison does not."""
    from numera import webstore

    dest = tmp_path / "ext"
    dest.mkdir()
    crx = tmp_path / "e.zip"
    with zipfile.ZipFile(crx, "w") as z:
        z.writestr("manifest.json", '{"name":"x","version":"1"}')
        z.writestr("../ext-evil/pwned.txt", b"x")
    with pytest.raises(webstore.WebStoreError, match="outside the folder"):
        webstore._unpack_zip(crx.read_bytes(), dest)
    assert not (tmp_path / "ext-evil").exists()


def test_a_parent_traversal_member_cannot_escape_the_extension_folder(tmp_path):
    from numera import webstore

    dest = tmp_path / "ext"
    dest.mkdir()
    crx = tmp_path / "e.zip"
    with zipfile.ZipFile(crx, "w") as z:
        z.writestr("manifest.json", '{"name":"x","version":"1"}')
        z.writestr("../../pwned.txt", b"x")
    with pytest.raises(webstore.WebStoreError, match="outside the folder"):
        webstore._unpack_zip(crx.read_bytes(), dest)
    assert not (tmp_path / "pwned.txt").exists()


def test_an_ordinary_extension_still_unpacks(tmp_path):
    from numera import webstore

    dest = tmp_path / "ext"
    dest.mkdir()
    crx = tmp_path / "e.zip"
    with zipfile.ZipFile(crx, "w") as z:
        z.writestr("manifest.json", '{"name":"x","version":"1"}')
        z.writestr("js/content.js", b"console.log(1)")
    webstore._unpack_zip(crx.read_bytes(), dest)
    assert (dest / "manifest.json").is_file()
    assert (dest / "js" / "content.js").is_file()


def test_an_icon_path_cannot_point_at_a_sibling_directory(tmp_path):
    """Same string-prefix bug, second site: the manifest's icon path is
    attacker-controlled too."""
    from numera import webstore

    folder = tmp_path / "ext"
    (folder / "js").mkdir(parents=True)
    evil = tmp_path / "ext-evil"
    evil.mkdir()
    (evil / "secret.png").write_bytes(b"x")
    assert webstore._icon_path(folder, "../ext-evil/secret.png") is None


# --- Windows device names are not filenames ---------------------------------

import pytest as _pytest

from numera.paths import safe_id as _safe_id


@_pytest.mark.parametrize("bad", [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM9", "LPT1", "LPT9", "con", "NuL",
])
def test_a_windows_device_name_is_not_a_usable_profile_id(bad):
    """`CON`, `NUL`, `COM1` and friends are devices on Windows, in every
    directory, in any case, with or without an extension. They contain nothing
    unusual so the character class let them through, and opening one succeeds -
    reading or writing the device instead of a profile. Server-minted ids never
    look like this; an import, a restore or the CLI can carry one in."""
    with _pytest.raises(ValueError, match="reserved device name"):
        _safe_id(bad)


@_pytest.mark.parametrize("bad", ["nul.json", "COM1.backup", "aux.tar.gz", "CON.txt"])
def test_a_device_name_with_an_extension_is_refused_too(bad):
    """Windows treats `nul.json` as the device as well. These are already
    refused a step earlier - the character class has no `.` - but the device
    check tests the stem too, so it stays correct if that class ever widens."""
    with _pytest.raises(ValueError):
        _safe_id(bad)


@_pytest.mark.parametrize("ok", [
    "CONS", "console", "COM", "COM10", "LPT10", "NULL", "auxiliary",
    "ab12cd34ef56", "a-b_c",
])
def test_names_that_merely_look_like_devices_are_fine(ok):
    """The rule is exact-stem, not substring: refusing `console` would reject
    real ids for no reason."""
    assert _safe_id(ok) == ok


@_pytest.mark.parametrize('bad', ['valid\n', 'valid\r\n', '../escape', 'a/b', 'a\\b'])
def test_profile_id_requires_a_complete_portable_filename(bad):
    with _pytest.raises(ValueError):
        _safe_id(bad)


def test_json_replacement_never_overwrites_an_unowned_temporary_file(tmp_path):
    import os
    import threading
    from numera.paths import write_json

    target = tmp_path / 'profile.json'
    sentinel = tmp_path / 'unrelated.json'
    sentinel.write_bytes(b'preserve')
    temporary = target.with_name(f'{target.name}.tmp-{os.getpid()}-{threading.get_ident():x}')
    # A preexisting ordinary file exercises the exclusive-create contract on
    # every host. Symlink refusal uses the same O_EXCL path without privilege.
    temporary.write_bytes(b'preexisting owner')
    with pytest.raises(FileExistsError):
        write_json(target, {'name': 'fixture'})
    assert sentinel.read_bytes() == b'preserve'
    assert not target.exists()
    assert temporary.read_bytes() == b'preexisting owner', 'an unowned temporary file changed'


def test_profile_path_never_resolves_one_id_into_another(tmp_path, monkeypatch):
    from pathlib import Path
    from numera.paths import id_path

    target = tmp_path / 'first.json'
    original = Path.is_symlink
    monkeypatch.setattr(Path, 'is_symlink', lambda path: path == target or original(path))
    with pytest.raises(ValueError, match='linked'):
        id_path(tmp_path, 'first', '.json')


def test_a_linked_session_claim_stays_owned_instead_of_appearing_absent(tmp_path, monkeypatch):
    from pathlib import Path
    from numera import sessions
    from numera.paths import SessionBusy

    directory = tmp_path / 'sessions'
    directory.mkdir()
    claim = directory / 'fixture.json'
    claim.write_bytes(b'preserve uncertain owner')
    original = Path.is_symlink
    monkeypatch.setattr(Path, 'is_symlink', lambda path: path == claim or original(path))
    assert sessions.read(tmp_path, 'fixture') is sessions.UNREADABLE
    assert sessions.busy(tmp_path, 'fixture') is True
    with pytest.raises(SessionBusy):
        sessions.refuse_if_busy(tmp_path, 'fixture')
    assert claim.read_bytes() == b'preserve uncertain owner'
    assert sessions.read(tmp_path, '../escape') is None


@pytest.mark.parametrize('member', ['../outside', '../records-evil/file', 'child/../../outside'])
def test_archive_path_is_refused_before_probing_an_outside_target(tmp_path, monkeypatch, member):
    from pathlib import Path
    from numera.paths import safe_join

    root = tmp_path / 'records'
    root.mkdir()
    original = Path.resolve
    probed = []
    def resolve(path, *args, **kwargs):
        if path != root:
            probed.append(path)
            raise AssertionError('An escaping member reached filesystem resolution')
        return original(path, *args, **kwargs)
    monkeypatch.setattr(Path, 'resolve', resolve)
    with pytest.raises(ValueError, match='escapes'):
        safe_join(root, member)
    assert not probed


@pytest.mark.parametrize('kind', ['archive', 'profile'])
def test_physical_check_keeps_links_before_parent_components(tmp_path, monkeypatch, kind):
    from pathlib import Path
    from numera.paths import id_path, safe_join

    root = tmp_path / 'records'
    root.mkdir()
    member = root / 'linked' / '..' / 'target'
    original = Path.resolve

    def resolve(path, *args, **kwargs):
        if path == member:
            return tmp_path / 'outside' / 'target'
        return original(path, *args, **kwargs)

    monkeypatch.setattr(Path, 'resolve', resolve)
    with pytest.raises(ValueError, match='escapes'):
        if kind == 'archive':
            safe_join(root, 'linked/../target')
        else:
            id_path(root, 'linked', '/../target')


def test_profile_path_rejects_an_escaping_suffix_before_filesystem_access(tmp_path, monkeypatch):
    from pathlib import Path
    from numera.paths import id_path

    monkeypatch.setattr(Path, 'is_symlink', lambda path: (_ for _ in ()).throw(
        AssertionError('An escaping suffix reached a filesystem probe')))
    with pytest.raises(ValueError, match='escapes'):
        id_path(tmp_path, 'fixture', '/../../outside')


def test_json_replacement_uses_private_file_permissions_on_posix(tmp_path):
    import os
    from numera.paths import write_json

    target = tmp_path / 'credentials.json'
    write_json(target, {'fixture': 'synthetic'})
    assert json.loads(target.read_text(encoding='utf-8')) == {'fixture': 'synthetic'}
    if os.name != 'nt':
        assert target.stat().st_mode & 0o777 == 0o600
