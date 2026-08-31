"""Rotating a proxy has to prove the exit IP moved, not just that a URL answered.

Every provider that offers a change-IP endpoint returns 200 for a request it
then rate-limits, ignores, or applies on a timer. A button that reports success
on the HTTP status alone is the dead-control pattern this project keeps hitting
(B5, B14, B15) - so `rotate` re-checks and compares, and says so when nothing
moved.
"""
import http.server
import threading
import time

import pytest

from numera import proxypool as pp


class Endpoint:
    """A stand-in for the provider's change-IP URL, counting what it was sent."""

    def __init__(self, status=200, body=b"OK"):
        self.hits = 0
        self.status, self.body = status, body
        outer = self

        class H(http.server.BaseHTTPRequestHandler):
            def do_GET(self):
                outer.hits += 1
                self.send_response(outer.status)
                self.send_header("Content-Length", str(len(outer.body)))
                self.end_headers()
                self.wfile.write(outer.body)

            def log_message(self, *a):
                pass

        self.server = http.server.HTTPServer(("127.0.0.1", 0), H)
        self.url = f"http://127.0.0.1:{self.server.server_port}/rotate"

    def __enter__(self):
        threading.Thread(target=self.server.serve_forever, daemon=True).start()
        return self

    def __exit__(self, *exc):
        self.server.shutdown()
        self.server.server_close()


def proxy(**kw):
    base = {"id": "p1", "type": "HTTP", "host": "127.0.0.1", "port": 1,
            "user": None, "pass": None}
    return {**base, **kw}


def test_no_url_is_a_clear_refusal_not_a_silent_no_op():
    res = pp.rotate(proxy())
    assert res["rotated"] is False
    assert "change-IP URL" in res["error"]


def test_a_non_http_url_is_refused_before_anything_is_fetched():
    res = pp.rotate(proxy(rotateUrl="ftp://example.test/rotate"))
    assert res["rotated"] is False
    assert "http(s)" in res["error"]


def test_the_cooldown_refuses_instead_of_returning_the_same_ip():
    """Providers rate-limit this endpoint; hammering it hands back the address
    you already had, which reads as a broken button."""
    with Endpoint() as ep:
        res = pp.rotate(proxy(rotateUrl=ep.url, rotatedAt=time.time() - 2))
        assert res["rotated"] is False
        assert "rate-limit" in res["error"]
        assert ep.hits == 0, "it fetched the endpoint despite the cooldown"


def test_a_dead_endpoint_is_reported_and_the_proxy_is_not_marked_rotated():
    res = pp.rotate(proxy(rotateUrl="http://127.0.0.1:9/rotate"))
    assert res["rotated"] is False
    assert res["ok"] is False
    assert res["error"]


def test_an_endpoint_that_answers_but_does_not_move_the_ip_is_not_a_success(monkeypatch):
    """The whole point of the re-check. A 200 with an unchanged address must not
    be reported green."""
    monkeypatch.setattr(pp, "check", lambda *a, **k: {"ok": True, "ip": "5.5.5.5"})
    with Endpoint(body=b"rotation queued") as ep:
        res = pp.rotate(proxy(rotateUrl=ep.url, lastCheck={"ip": "5.5.5.5"}))
        assert ep.hits == 1
        assert res["rotated"] is False
        assert "still 5.5.5.5" in res["error"]
        assert "rotation queued" in res["error"], "the provider's reply is dropped"


def test_a_real_move_is_reported_with_both_addresses(monkeypatch):
    monkeypatch.setattr(pp, "check", lambda *a, **k: {"ok": True, "ip": "9.9.9.9"})
    with Endpoint() as ep:
        res = pp.rotate(proxy(rotateUrl=ep.url, lastCheck={"ip": "5.5.5.5"}))
        assert res["rotated"] is True
        assert res["previousIp"] == "5.5.5.5" and res["ip"] == "9.9.9.9"
        assert "error" not in res or not res.get("error")


def test_a_first_rotate_with_no_previous_ip_counts_as_moved(monkeypatch):
    """Nothing to compare against, but an address did come back - refusing to
    call that a rotation would make the first click always look like a failure."""
    monkeypatch.setattr(pp, "check", lambda *a, **k: {"ok": True, "ip": "9.9.9.9"})
    with Endpoint() as ep:
        assert pp.rotate(proxy(rotateUrl=ep.url))["rotated"] is True


def test_the_endpoint_is_fetched_directly_not_through_the_proxy(monkeypatch):
    """The change-IP URL is the provider's control plane. Routing it through the
    proxy being rotated is how the response gets lost mid-flight - and a dead
    proxy would then make every rotate fail for the wrong reason.

    `check` is the only part that should build a proxy opener, so with it stubbed
    out nothing may reach `build_opener` at all.
    """
    built = []
    monkeypatch.setattr(pp, "check", lambda *a, **k: {"ok": True, "ip": "9.9.9.9"})
    monkeypatch.setattr(pp.urllib.request, "build_opener",
                        lambda *a, **k: built.append(a) or (_ for _ in ()).throw(
                            AssertionError("rotate routed its own request through a proxy")))
    with Endpoint() as ep:
        res = pp.rotate(proxy(rotateUrl=ep.url))
    assert ep.hits == 1 and res["rotated"] is True
    assert not built


# --- one canonical proxy identity ------------------------------------------

import pytest as _pt


@_pt.mark.parametrize("kind", ["HTTP", "HTTPS", "SOCKS5", "SOCKS4"])
def test_the_protocol_survives_a_round_trip(kind):
    """The dash->engine map had no SOCKS4 entry and fell back to `http`, so a
    SOCKS4 profile launched a browser speaking a different protocol than the
    profile claimed - silently."""
    from numera.server.convert import to_dashboard_profile, to_engine_profile

    e = to_engine_profile({"name": "x", "os": "Windows",
                           "proxy": {"type": kind, "host": "1.2.3.4", "port": "8080"}})
    assert e.proxy.server.startswith(kind.lower() + "://"), e.proxy.server
    assert to_dashboard_profile(e)["proxy"]["type"] == kind


def test_an_unsupported_protocol_is_refused_not_downgraded():
    from numera.proxypool import server_url

    with _pt.raises(ValueError):
        server_url("QUIC", "1.2.3.4", "8080")


def test_ipv6_survives_both_directions():
    from numera.server.convert import to_dashboard_profile, to_engine_profile

    e = to_engine_profile({"name": "x", "os": "Windows",
                           "proxy": {"type": "SOCKS5", "host": "2001:db8::1",
                                     "port": "1080"}})
    assert e.proxy.server == "socks5://[2001:db8::1]:1080"
    back = to_dashboard_profile(e)["proxy"]
    assert back["host"] == "2001:db8::1" and back["port"] == "1080"


def test_credential_variants_are_different_proxies():
    """Sticky-session vendors hand out one host:port for a whole pool and pick
    the exit node from the credentials, so keying on host:port made every one of
    them look like the same proxy - and "used by N profiles" counted them as one.
    """
    from numera.proxypool import identity

    base = {"type": "HTTP", "host": "gw.vendor.test", "port": "8000"}
    a = identity({**base, "user": "session-a", "pass": "x"})
    b = identity({**base, "user": "session-b", "pass": "x"})
    c = identity({**base, "user": "session-a", "pass": "y"})
    assert len({a, b, c}) == 3


@_pt.mark.parametrize("server,want", [
    ("http://1.2.3.4:8080", ("http", "1.2.3.4", "8080")),
    ("socks5://[2001:db8::1]:1080", ("socks5", "2001:db8::1", "1080")),
    ("1.2.3.4:3128", ("http", "1.2.3.4", "3128")),
    ("[fe80::1]:8080", ("http", "fe80::1", "8080")),
])
def test_split_server_understands_ipv6(server, want):
    from numera.proxypool import split_server

    assert split_server(server) == want
