"""The CLI is a fourth door into the profile store, and it had no guard.

The API import route validates every writable field before it writes. `numera
import-from` does not: it goes `load_export` -> `profile_from_export` ->
`repo(store).save_many`, and `check_fields` is nowhere on that path. A vendor
export carrying `"name": 123` therefore wrote a numeric name into
`profiles/<id>.json`, where nothing objected - and the next dashboard listing
*adopted* that record into its own store and answered `name: 123` to a client
that calls `p.name.toLowerCase()` on it.

Guarding the adoption read instead would be the wrong seam twice over: it would
make `GET /api/profiles` drop or fail on a profile the CLI legitimately
created, which `test_a_corrupt_record_does_not_take_an_unrelated_view_down`
forbids, and it would leave the bad record on disk for the next reader.

So the guard sits on the write path, at `ProfileRepo._require_writable`, which
both public write entries call before the name is used for anything - it is a
dictionary key three lines later, and a list or dict name raised
`TypeError: unhashable type` out of the uniqueness check instead of being
refused by name. The dashboard mirror is the one write that does not go through
`repo` (`_write_both` writes the engine file straight through its own batch),
so `ProfileStore._save_engine` repeats the rule for that door.

Not in `Profile.__post_init__`: that would refuse to *load* a record written
before the check existed, turning one bad stored profile into an unreadable
store - the opposite of what `routes_profiles.readable` is for. Reading stays
tolerant and repairs; writing refuses.

A name is not an ordinary field: `resolve(name)`, the uniqueness check and
every view treat it as the profile's identity, and all of them assume text.
"""

import json

import pytest

from numera import generate
from numera.models import Profile
from numera.store import ProfileStore


@pytest.mark.parametrize("bad", [123, 12.5, ["a"], {"x": 1}, None, True])
def test_a_profile_name_that_is_not_text_is_refused(tmp_path, bad):
    """The guard itself, at the primitive every door reaches."""
    store = ProfileStore(tmp_path)

    with pytest.raises(ValueError, match="name must be text"):
        store.save(Profile(name=bad, fingerprint=generate(seed=1)))

    assert list((tmp_path / "profiles").glob("*.json")) == [], \
        "the refusal still wrote the profile"


def test_an_ordinary_name_is_untouched(tmp_path):
    """The control. The guard is about type, not content - an empty name and a
    name full of punctuation are both legitimate."""
    store = ProfileStore(tmp_path)

    for name in ("alpha", "", "  spaces  ", "Ünïcodé ✓", "123"):
        saved = store.save(Profile(name=name, fingerprint=generate(seed=2)))
        assert saved.name == name


def test_the_cli_import_door_is_closed(tmp_path):
    """The regression end to end, through the path the CLI actually uses."""
    from numera import importers

    export = tmp_path / "vendor.json"
    export.write_text(json.dumps([{
        "name": 123, "os": "win", "userAgent": "Mozilla/5.0", "resolution": "1920x1080",
    }]), encoding="utf-8")
    store = ProfileStore(tmp_path / "root")

    with pytest.raises(Exception):
        importers.import_file(export, store)

    assert store.list() == [], "a numeric name reached the engine store"


def test_a_good_vendor_export_still_imports_from_the_cli(tmp_path):
    """The guard must not cost the feature it is guarding."""
    from numera import importers

    export = tmp_path / "vendor.json"
    export.write_text(json.dumps([{
        "name": "from-gologin", "os": "win", "userAgent": "Mozilla/5.0",
        "resolution": "1920x1080",
    }]), encoding="utf-8")
    store = ProfileStore(tmp_path / "root")

    importers.import_file(export, store)

    assert [p.name for p in store.list()] == ["from-gologin"]


def test_the_dashboard_never_adopts_a_name_it_cannot_render(tmp_path):
    """The consequence the user actually saw: a listing that blanks the page.

    Asserted against the store rather than the route, because the defect was
    that the *engine* record existed at all - the adoption was downstream and
    behaving correctly on the data it was given.
    """
    store = ProfileStore(tmp_path)
    store.save(Profile(name="fine", fingerprint=generate(seed=3)))

    for prof in store.list():
        assert isinstance(prof.name, str)

    for path in (tmp_path / "profiles").glob("*.json"):
        assert isinstance(json.loads(path.read_text("utf-8"))["name"], str)
