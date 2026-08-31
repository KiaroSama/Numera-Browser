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
