"""A malformed `rotateUrl` already on disk is a refusal, not a 500.

Write-time validation was added for new values, but it cannot reach what is
already stored: an endpoint from an older build, a hand-edited `proxies.json`
or a restored backup. `_public_destination()` read `parts.port`, and `urlsplit`
raises `ValueError` for an out-of-range port *when the attribute is read* -
outside `rotate()`'s network `try`, so the request came back as a traceback
instead of a message naming the problem.

These write the bad value straight to disk, as a restore would, and then use
the real API and the real CLI.
"""
import json

import pytest

from numera import proxypool as pp
from numera.server.stores import ProxyPool
from numera.store import ProfileStore

pytest.importorskip("fastapi")

from fastapi.testclient import TestClient        # noqa: E402

from numera.server import create_app             # noqa: E402

# Port 99999 is out of range: `urlsplit` accepts the string and raises when the
# port is read. `[oops` is a malformed authority, which raises on `.hostname`.
MALFORMED = ("http://example.test:99999/rotate", "http://[oops/rotate")


def seeded(root, rotate_url: str) -> str:
    """One pool entry whose `rotateUrl` never went through validation."""
    pool = ProxyPool(root, ProfileStore(root))
    pid = pool.add_many([{"host": "1.2.3.4", "port": 8080,
                          "protocol": "http"}])[0]["id"]
    path = root / "proxies.json"
    entries = json.loads(path.read_text(encoding="utf-8"))
    entries[0]["rotateUrl"] = rotate_url        # as a restore or an old build left it
    path.write_text(json.dumps(entries, indent=2), encoding="utf-8")
    return pid


@pytest.mark.parametrize("url", MALFORMED)
def test_the_destination_check_itself_fails_closed(url):
    problem, pinned = pp._public_destination(url)
    assert problem, f"{url} was accepted as a destination"
    assert pinned is None


@pytest.mark.parametrize("url", MALFORMED)
def test_the_api_answers_with_the_problem_and_no_traceback(tmp_path, url):
    pid = seeded(tmp_path, url)
    c = TestClient(create_app(data_dir=str(tmp_path), start_scheduler=False),
                   raise_server_exceptions=False)

    r = c.post(f"/api/proxies/{pid}/rotate")
    assert r.status_code == 200, r.text          # a refusal, not a crash
    body = r.json()
    assert body.get("rotated") is False, body
    assert body.get("error"), f"the refusal has to say what is wrong: {body}"


@pytest.mark.parametrize("url", MALFORMED)
def test_the_cli_reports_it_and_exits_nonzero(tmp_path, url, capsys):
    from numera import cli

    pid = seeded(tmp_path, url)
    code = cli.main(["--data-dir", str(tmp_path), "pool", "rotate", pid])
    assert code != 0, "a malformed endpoint must not report success"
    out = capsys.readouterr().out
    assert "Traceback" not in out
