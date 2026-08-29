"""The dashboard is served by the API itself - one process, one port.

The risk this file exists for: the SPA needs a catch-all so a refresh on a
client-side route still returns index.html, and a catch-all is exactly the thing
that can silently swallow the API. A mistyped endpoint answering 200 with HTML
instead of 404 is far worse than a 404 - the dashboard would try to JSON.parse a
web page and fail somewhere unrelated.
"""

import pytest

pytest.importorskip("fastapi")
from fastapi.testclient import TestClient      # noqa: E402

from numera import server                       # noqa: E402


@pytest.fixture
def built(tmp_path, monkeypatch):
    """A minimal built dashboard, so the mount runs without a real npm build."""
    root = tmp_path / "dist"
    (root / "assets").mkdir(parents=True)
    (root / "index.html").write_text("<!doctype html><title>Numera</title>", encoding="utf-8")
    (root / "assets" / "index.js").write_text("export default 1;", encoding="utf-8")
    (root / "favicon.svg").write_text("<svg/>", encoding="utf-8")
    monkeypatch.setattr(server, "dashboard_dir", lambda: root)
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
    monkeypatch.setattr(server, "dashboard_dir", lambda: None)
    c = TestClient(server.create_app(str(tmp_path / "data2"), start_scheduler=False))
    assert c.get("/api/health").status_code == 200
    assert c.get("/").status_code == 404
