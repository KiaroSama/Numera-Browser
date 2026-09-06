"""Extension loading: the flags a profile's extensions turn into."""

from numera import generate
from numera.drivers import _extension_args
from numera.models import Profile
from numera.store import ProfileStore


def mk(paths):
    return Profile(name="x", fingerprint=generate(seed=1), extensions=paths)


def test_no_extensions_means_no_flags():
    assert _extension_args(mk([])) == []


def test_both_flags_are_emitted(tmp_path):
    ext = tmp_path / "ublock"
    ext.mkdir()
    args = _extension_args(mk([str(ext)]))
    # --load-extension alone would leave every other extension enabled, which
    # would leak identity between profiles.
    assert len(args) == 2
    assert args[0].startswith("--disable-extensions-except=")
    assert args[1].startswith("--load-extension=")
    assert str(ext) in args[1]


def test_multiple_paths_are_comma_joined(tmp_path):
    a, b = tmp_path / "a", tmp_path / "b"
    a.mkdir(); b.mkdir()
    args = _extension_args(mk([str(a), str(b)]))
    assert args[1].count(",") == 1


def test_missing_folders_are_dropped(tmp_path):
    real = tmp_path / "real"
    real.mkdir()
    args = _extension_args(mk([str(real), str(tmp_path / "gone")]))
    assert "gone" not in args[1]
    assert "real" in args[1]


def test_only_missing_folders_means_no_flags(tmp_path):
    assert _extension_args(mk([str(tmp_path / "nope")])) == []


def test_extensions_survive_a_save_round_trip():
    p = mk(["/some/ext"])
    assert Profile.from_dict(p.to_dict()).extensions == ["/some/ext"]


def test_profiles_saved_before_extensions_existed_still_load():
    d = mk([]).to_dict()
    del d["extensions"]
    assert Profile.from_dict(d).extensions == []


# ---- the global default set --------------------------------------------
# Chromium has no browser-wide extension store to hook, so "install once for
# every profile" has to mean "copy onto each profile as it is created".


def unpacked(root, name="ublock"):
    """A folder Chromium would actually load.

    The store validates what it is given now - the same rule the API applies -
    so a placeholder path is refused here exactly as it would be refused in the
    dashboard. These tests are about the round trip, not the refusal, so they
    need a real one.
    """
    d = root / name
    d.mkdir(parents=True, exist_ok=True)
    (d / "manifest.json").write_text(
        '{"manifest_version": 3, "name": "' + name + '", "version": "1.0"}',
        encoding="utf-8")
    return d


def test_default_extensions_round_trip(tmp_path):
    store = ProfileStore(tmp_path)
    assert store.get_default_extensions() == []
    ext = unpacked(tmp_path)
    store.set_default_extensions([str(ext)])
    assert ProfileStore(tmp_path).get_default_extensions() == [str(ext)]


def test_new_profiles_inherit_the_default_set(tmp_path):
    from numera.cli import build_parser
    store = ProfileStore(tmp_path)
    ext = unpacked(tmp_path, "shared-ext")
    store.set_default_extensions([str(ext)])

    args = build_parser().parse_args(["create", "inherits"])
    assert args.func(args, store) == 0
    assert store.find_by_name("inherits").extensions == [str(ext)]


# ---- and the per-profile list uses the same validator --------------------
# `numera ext add` checked `Path(path).is_dir()` and nothing else, while the
# API and the default set both required a genuinely loadable unpacked
# extension. So the CLI stored what the launcher then dropped: `_extension_args`
# filters out anything without a manifest, and the profile was shown loading an
# extension Chromium never saw, with no error at any point.

def add_extension(root, ref, path) -> int:
    from numera.cli import build_parser

    args = build_parser().parse_args(["ext", "add", ref, str(path)])
    return args.func(args, ProfileStore(root))


def a_profile_in(root, name="target"):
    from numera.repo import repo

    store = ProfileStore(root)
    return repo(store).create(Profile(name=name, fingerprint=generate(seed=3)))


def test_an_empty_folder_is_not_an_extension(tmp_path):
    """A folder with no manifest.json is exactly the case `is_dir()` let
    through - Chromium refuses it silently and the profile claims to load it."""
    prof = a_profile_in(tmp_path)
    empty = tmp_path / "not-an-extension"
    empty.mkdir()

    assert add_extension(tmp_path, prof.name, empty) != 0
    assert ProfileStore(tmp_path).get(prof.id).extensions == [], \
        "an unloadable folder was stored on the profile"


def test_a_web_store_url_is_not_an_extension_path(tmp_path):
    """The other half of the shared validator: a URL belongs to `ext install`,
    which downloads and unpacks it. Pasted into the list it does nothing."""
    prof = a_profile_in(tmp_path)

    assert add_extension(tmp_path, prof.name,
                         "https://chrome.google.com/webstore/detail/abc") != 0
    assert ProfileStore(tmp_path).get(prof.id).extensions == []


def test_a_real_unpacked_extension_is_still_added(tmp_path):
    prof = a_profile_in(tmp_path)
    ext = unpacked(tmp_path, "ublock")

    assert add_extension(tmp_path, prof.name, ext) == 0
    assert ProfileStore(tmp_path).get(prof.id).extensions == [str(ext)]


def test_bulk_apply_pushes_extensions_onto_existing_profiles(tmp_path):
    # Existing profiles predate the default set, so they need an explicit push.
    from numera.bulk import apply_patch
    prof = Profile(name="old", fingerprint=generate(seed=2))
    assert prof.extensions == []
    assert "extensions" in apply_patch(prof, {"extensions": ["/opt/ext/ublock"]})
    assert prof.extensions == ["/opt/ext/ublock"]
