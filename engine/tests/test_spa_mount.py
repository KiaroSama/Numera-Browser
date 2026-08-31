"""The dashboard is served by the API itself - one process, one port.

The risk this file exists for: the SPA needs a catch-all so a refresh on a
client-side route still returns index.html, and a catch-all is exactly the thing
that can silently swallow the API. A mistyped endpoint answering 200 with HTML
instead of 404 is far worse than a 404 - the dashboard would try to JSON.parse a
web page and fail somewhere unrelated.
"""

from pathlib import Path

import pytest

pytest.importorskip("fastapi")
from fastapi.testclient import TestClient      # noqa: E402

from numera import server                       # noqa: E402
from numera.server import spa                   # noqa: E402


@pytest.fixture
def built(tmp_path, monkeypatch):
    """A minimal built dashboard, so the mount runs without a real npm build."""
    root = tmp_path / "dist"
    (root / "assets").mkdir(parents=True)
    (root / "index.html").write_text("<!doctype html><title>Numera</title>", encoding="utf-8")
    (root / "assets" / "index.js").write_text("export default 1;", encoding="utf-8")
    (root / "favicon.svg").write_text("<svg/>", encoding="utf-8")
    # Patch where it is defined, not where it is re-exported. `server.dashboard_dir`
    # is a name bound in the package's __init__; `_mount_dashboard` looks the
    # function up in its own module, so rebinding the re-export changes nothing.
    # (Before server.py became a package the two were the same object, which is
    # why this used to work.)
    monkeypatch.setattr(spa, "dashboard_dir", lambda: root)
    return root


@pytest.fixture
def client(built, tmp_path):
    return TestClient(server.create_app(str(tmp_path / "data"), start_scheduler=False))


def test_root_serves_the_dashboard(client):
    r = client.get("/")
    assert r.status_code == 200
    assert "Numera" in r.text


def test_a_client_side_route_falls_back_to_index(client):
    """Refreshing on /proxies must not 404 - the SPA routes it in the browser."""
    r = client.get("/proxies")
    assert r.status_code == 200
    assert "<!doctype html>" in r.text.lower()


def test_a_real_file_wins_over_the_fallback(client):
    r = client.get("/favicon.svg")
    assert r.status_code == 200
    assert "<svg" in r.text


def test_assets_are_served(client):
    assert client.get("/assets/index.js").status_code == 200


def test_the_api_still_answers(client):
    r = client.get("/api/health")
    assert r.status_code == 200
    assert r.headers["content-type"].startswith("application/json")


def test_an_unknown_api_path_404s_instead_of_returning_html(client):
    """The whole point of the guard. Without it this returns 200 + index.html."""
    r = client.get("/api/definitely-not-a-real-endpoint")
    assert r.status_code == 404
    assert "<!doctype html" not in r.text.lower()


def test_the_app_still_starts_with_no_dashboard_build(tmp_path, monkeypatch):
    """A CLI-only / server install has no dist directory; that must not crash."""
    monkeypatch.setattr(spa, "dashboard_dir", lambda: None)
    c = TestClient(server.create_app(str(tmp_path / "data2"), start_scheduler=False))
    assert c.get("/api/health").status_code == 200
    assert c.get("/").status_code == 404


def test_the_dashboard_is_found_in_a_source_checkout():
    """Not "does it return a path" - does it find the *real* build.

    This is the check the rest of this file could not make: every other test
    monkeypatches `dashboard_dir`, so all four kept passing while the real one
    returned None and the served app 404'd on every page.

    It broke because the lookup counted parent directories - `here.parents[2]`
    was the repo root while this code lived in `numera/server.py`, and became
    `engine/` the moment it moved into `numera/server/`. Counting levels is a
    hidden dependency on a file's own location; searching upward is not.
    """
    root = Path(__file__).resolve().parents[2]
    built = root / "dashboard" / "dist" / "index.html"
    if not built.is_file():
        pytest.skip("the dashboard has not been built in this checkout")

    found = spa.dashboard_dir()
    assert found is not None, (
        "dashboard_dir() found nothing although dashboard/dist/index.html exists - "
        "the API would serve a 404 for every page")
    assert (found / "index.html").resolve() == built.resolve(), found


def test_dashboard_dir_survives_being_moved_deeper(tmp_path, monkeypatch):
    """The regression itself: the same layout, one directory further down.

    Simulated by pointing the lookup at a file nested deeper inside a fake
    checkout. A level-counting implementation fails this; an upward search
    passes.
    """
    root = tmp_path / "repo"
    (root / "dashboard" / "dist").mkdir(parents=True)
    (root / "dashboard" / "dist" / "index.html").write_text("<!doctype html>", encoding="utf-8")
    deep = root / "engine" / "numera" / "server" / "sub" / "deeper"
    deep.mkdir(parents=True)
    fake = deep / "spa.py"
    fake.write_text("", encoding="utf-8")

    monkeypatch.setattr(spa, "__file__", str(fake))
    found = spa.dashboard_dir()
    assert found is not None, "a deeper module could not find the build above it"
    assert found == root / "dashboard" / "dist", found
