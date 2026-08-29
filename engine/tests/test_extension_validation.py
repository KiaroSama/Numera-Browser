"""Extension entries the launcher cannot use must be refused, not stored.

`drivers._extension_args` filters out anything that is not a directory. That is
correct defensively, but it meant a bad entry was accepted by the API, displayed
as saved, and then did nothing at launch, with no error anywhere. A user pasted a
Chrome Web Store URL and the UI reported success.

That is this project's recurring failure: a control that accepts input, reports
success and changes nothing. The check belongs at the write boundary so the
dashboard, the CLI and any API client all get the same refusal.
"""

import pytest

pytest.importorskip("fastapi")
from fastapi.testclient import TestClient      # noqa: E402

from numera import server                       # noqa: E402


@pytest.fixture
def client(tmp_path):
    return TestClient(server.create_app(str(tmp_path / "data"), start_scheduler=False))


@pytest.fixture
def unpacked(tmp_path):
    """A minimal but genuinely loadable unpacked extension."""
    d = tmp_path / "ext"
    d.mkdir()
    (d / "manifest.json").write_text(
        '{"manifest_version": 3, "name": "probe", "version": "1.0"}', encoding="utf-8")
    return d


def _put(client, paths):
    return client.put("/api/config", json={"default_extensions": [str(p) for p in paths]})


def test_a_web_store_url_in_the_folder_list_points_at_the_installer(client):
    """A URL is still not a folder - but the answer changed.

    It used to be "download and unpack the crx yourself", which was true and
    useless. `POST /api/extensions/install` now does exactly that, so the message
    has to point there instead. The assertion that matters is unchanged: refusing
    without saying what to do only moves the confusion."""
    r = _put(client, ["https://chromewebstore.google.com/detail/x/abcdefg?hl=en"])
    assert r.status_code == 400
    detail = r.json()["detail"]
    assert "URL" in detail
    assert "ext install" in detail and "Extensions page" in detail


def test_a_path_that_does_not_exist_is_refused(client, tmp_path):
    r = _put(client, [tmp_path / "nope"])
    assert r.status_code == 400
    assert "not a folder" in r.json()["detail"]


def test_a_folder_without_a_manifest_is_refused(client, tmp_path):
    empty = tmp_path / "empty"
    empty.mkdir()
    r = _put(client, [empty])
    assert r.status_code == 400
    assert "manifest.json" in r.json()["detail"]


def test_a_real_unpacked_extension_is_accepted(client, unpacked):
    """The other half: the guard must not reject what does work."""
    r = _put(client, [unpacked])
    assert r.status_code == 200
    assert str(unpacked) in r.json()["default_extensions"]


def test_nothing_is_stored_when_one_entry_is_bad(client, unpacked, tmp_path):
    """A partial write would be worse than a refusal: the user would see some of
    their list saved and have no idea which."""
    _put(client, [unpacked])
    before = client.get("/api/config").json()["default_extensions"]

    r = _put(client, [unpacked, tmp_path / "missing"])
    assert r.status_code == 400
    assert client.get("/api/config").json()["default_extensions"] == before


def test_the_launcher_would_have_dropped_exactly_what_we_now_refuse(tmp_path, unpacked):
    """Ties the guard to the behaviour it exists for.

    If `_extension_args` ever starts accepting URLs, this fails and the guard
    should be revisited rather than left refusing something that works."""
    from dataclasses import replace

    from numera import generate
    from numera.drivers import _extension_args
    from numera.models import Profile

    fp = generate(seed=3)
    bad = Profile(name="b", fingerprint=fp,
                  extensions=["https://chromewebstore.google.com/detail/x/y",
                              str(tmp_path / "missing")])
    assert _extension_args(bad) == [], "the launcher silently drops these - hence the guard"

    good = Profile(name="g", fingerprint=replace(fp), extensions=[str(unpacked)])
    args = _extension_args(good)
    assert any("--load-extension=" in a for a in args)
