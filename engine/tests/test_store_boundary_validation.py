"""Validity belongs to the store, not to whichever surface happened to call it.

Three places where a value could reach disk because the *route* validated it
and the store did not, or because a multi-field write committed the fields it
had already reached before rejecting the request.
"""
import json
import math

import pytest

from numera import cli
from numera import proxypool as pp
from numera.scheduler import ScheduleStore

pytest.importorskip("fastapi")

from fastapi.testclient import TestClient          # noqa: E402

from numera.server import create_app               # noqa: E402


def client(tmp_path):
    return TestClient(create_app(data_dir=str(tmp_path), start_scheduler=False))


def config_on_disk(tmp_path) -> dict:
    p = tmp_path / "config.json"
    return json.loads(p.read_text(encoding="utf-8")) if p.is_file() else {}


# --- 4. PUT /api/config is all or nothing -----------------------------------

def test_an_invalid_later_field_leaves_the_config_untouched(tmp_path):
    """It wrote field by field, so a request rejected on its *third* field had
    already committed the first two and then answered 400."""
    c = client(tmp_path)
    c.put("/api/config", json={"default_engine": "playwright"})
    before = config_on_disk(tmp_path)

    r = c.put("/api/config", json={
        "default_engine": "camoufox",
        "default_extensions": [str(tmp_path / "no-such-extension")],
    })

    assert r.status_code == 400, r.text
    assert config_on_disk(tmp_path) == before, \
        "a rejected request changed the config anyway"


def test_a_string_is_not_a_boolean(tmp_path):
    """`bool("false")` is True, so the JSON string "false" switched headless
    launching *on* - the opposite of the request."""
    c = client(tmp_path)
    r = c.put("/api/config", json={"headless_launch": "false"})
    assert r.status_code == 400
    assert "true or false" in r.json()["detail"]
    assert "headless_launch" not in config_on_disk(tmp_path)


def test_an_unknown_config_key_is_refused(tmp_path):
    c = client(tmp_path)
    r = c.put("/api/config", json={"root": "/somewhere/else"})
    assert r.status_code == 400, "a read-only key was silently ignored"
    assert "unknown config key" in r.json()["detail"]


def test_a_valid_multi_field_request_still_applies(tmp_path):
    c = client(tmp_path)
    r = c.put("/api/config", json={"default_engine": "camoufox", "headless_launch": True})
    assert r.status_code < 400, r.text
    stored = config_on_disk(tmp_path)
    assert stored["default_engine"] == "camoufox"
    assert stored["headless_launch"] is True


# --- 6. ScheduleStore.add validates like update and the API do --------------

@pytest.mark.parametrize("bad", [float("nan"), float("inf"), float("-inf"), -5, 0])
def test_the_store_refuses_a_schedule_it_cannot_serialise(tmp_path, bad):
    """`add()` converted with a bare `float()`. `nan` and `inf` are not JSON,
    and a negative interval asks a warm-up to browse for minus four minutes."""
    store = ScheduleStore(tmp_path)
    with pytest.raises(ValueError):
        store.add(profile_id="p", every_hours=6, minutes=bad)
    assert not (tmp_path / "schedules.json").exists() or \
        json.loads((tmp_path / "schedules.json").read_text(encoding="utf-8")) == []


def test_no_non_finite_number_reaches_the_file(tmp_path):
    store = ScheduleStore(tmp_path)
    store.add(profile_id="p", every_hours=6, minutes=5)
    for row in json.loads((tmp_path / "schedules.json").read_text(encoding="utf-8")):
        for key in ("everyHours", "minutes"):
            assert math.isfinite(row[key]), f"{key} is {row[key]}"


def test_the_cli_cannot_write_a_non_finite_schedule(tmp_path, capsys):
    """argparse accepts `nan` for a float option, and the CLI calls the store
    directly rather than through the HTTP route that pre-validates."""
    code = cli.main(["--data-dir", str(tmp_path), "schedule", "add", "p",
                     "--every", "6", "--minutes", "nan"])
    assert code != 0, "the CLI accepted a non-finite schedule"
    stored = tmp_path / "schedules.json"
    if stored.is_file():
        assert json.loads(stored.read_text(encoding="utf-8")) == []


def test_a_small_interval_is_still_clamped_not_refused(tmp_path):
    """Clamping is the long-standing behaviour; only non-numbers and
    non-positive values are refused."""
    store = ScheduleStore(tmp_path)
    row = store.add(profile_id="p", every_hours=0.001, minutes=5)
    assert row["everyHours"] > 0


# --- 9. rotateUrl is part of the canonical proxy ---------------------------

def test_a_structured_create_keeps_the_rotate_url(tmp_path):
    """`normalize()` returned only the core fields, so `POST /api/proxies`
    dropped a rotation endpoint that `PUT` would have kept."""
    c = client(tmp_path)
    url = "https://api.example.test/rotate?key=abc"
    r = c.post("/api/proxies", json={"proxies": [
        {"type": "HTTP", "host": "1.2.3.4", "port": "8080", "rotateUrl": url}]})
    assert r.status_code < 400, r.text

    listed = c.get("/api/proxies").json()
    assert listed, "nothing was created"
    assert listed[0].get("rotateUrl") == url, \
        f"the rotation endpoint was dropped: {listed[0]}"


@pytest.mark.parametrize("bad", [
    "ftp://example.test/rotate",
    "http://example.test:99999/rotate",
    "not-a-url",
    "http:///no-host",
])
def test_a_malformed_rotate_url_is_refused_not_a_traceback(tmp_path, bad):
    """`rotate()` reads `urlsplit(url).port`, which raises for an out-of-range
    port *outside* its network `try` - a stored bad endpoint became a 500."""
    c = client(tmp_path)
    r = c.post("/api/proxies", json={"proxies": [
        {"type": "HTTP", "host": "1.2.3.4", "port": "8080", "rotateUrl": bad}]})
    assert 400 <= r.status_code < 500, \
        f"{bad!r} produced {r.status_code}, not a refusal"


def test_the_canonical_shape_round_trips_through_normalize():
    url = "https://api.example.test/rotate"
    once = pp.normalize({"type": "HTTP", "host": "1.2.3.4", "port": "8080",
                         "rotateUrl": url})
    assert once["rotateUrl"] == url
    assert pp.normalize(once) == once, "normalize is not idempotent for rotateUrl"
