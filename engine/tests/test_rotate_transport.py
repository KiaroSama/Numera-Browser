"""Where the rotate control-plane request actually connects.

`rotateUrl` is the one address the engine fetches on the user's behalf, from the
server's own network position and deliberately *not* through the proxy. Two ways
that went wrong, both invisible from the response body:

* the redirect handler validated hop two's address and stored it on the new
  request, and the connection classes never read it - so the socket still went
  to hop one's IP. Validated one host, connected to another.
* `build_opener()` installs an environment ProxyHandler unless you say
  otherwise, so `HTTP_PROXY` routed the supposedly direct request elsewhere.

These tests inspect `socket.create_connection` rather than the response, because
the response looks identical either way - which is why neither bug showed up.
"""
import socket
import urllib.error
import urllib.request

import pytest

from numera import proxypool as pp

A_IP, B_IP = "8.8.8.8", "1.1.1.1"        # genuinely global, never dialled
A_URL, B_URL = "http://host-a.example/rotate", "http://host-b.example/next"


@pytest.fixture
def dialled(monkeypatch):
    """Record every address `create_connection` is asked for, and answer none."""
    seen = []

    def record(address, *a, **k):
        seen.append(address[0])
        raise OSError("no network in this test")

    monkeypatch.setattr(socket, "create_connection", record)
    return seen


@pytest.fixture
def resolves(monkeypatch):
    """host-a -> A_IP, host-b -> B_IP, both public as far as the guard knows."""
    import ipaddress

    table = {"host-a.example": A_IP, "host-b.example": B_IP}

    def fake(host, port):
        if host not in table:
            raise OSError(f"unknown host {host}")
        return [ipaddress.ip_address(table[host])]

    monkeypatch.setattr(pp, "_resolve", fake)
    return table            # the real `is_global` allow rule stays in force


def test_the_first_connection_uses_the_validated_address(resolves, dialled):
    problem, pin = pp._public_destination(A_URL)
    assert problem is None and str(pin) == A_IP

    opener = pp._pinned_opener(pin)
    with pytest.raises(Exception):
        opener.open(A_URL, timeout=1)

    assert dialled == [A_IP], f"connected to {dialled} instead of the checked {A_IP}"


def test_a_redirect_connects_to_the_hop_it_validated(resolves, dialled, monkeypatch):
    """The bug: hop two was validated, hop one's IP was dialled."""
    problem, pin = pp._public_destination(A_URL)
    assert problem is None
    opener = pp._pinned_opener(pin)

    # Build the redirected request exactly as the handler does, then open it.
    handler = [h for h in opener.handlers
               if isinstance(h, urllib.request.HTTPRedirectHandler)][0]
    original = urllib.request.Request(A_URL)
    redirected = handler.redirect_request(
        original, None, 302, "Found", {"location": B_URL}, B_URL)
    assert redirected is not None, "the redirect was refused outright"
    assert str(getattr(redirected, "pinned_ip", "")) == B_IP, \
        "the handler did not revalidate the new host"

    with pytest.raises(Exception):
        opener.open(redirected, timeout=1)

    assert dialled == [B_IP], (
        f"followed a redirect to host-b but connected to {dialled} - the pin for "
        f"the first hop was reused")


def test_a_redirect_to_a_private_address_is_refused(monkeypatch, dialled):
    """The existing refusal has to survive the per-request pinning."""
    import ipaddress

    table = {"host-a.example": A_IP, "internal.example": "127.0.0.1"}

    def fake(host, port):
        return [ipaddress.ip_address(table[host])]

    monkeypatch.setattr(pp, "_resolve", fake)
    problem, pin = pp._public_destination(A_URL)
    assert problem is None

    opener = pp._pinned_opener(pin)
    handler = [h for h in opener.handlers
               if isinstance(h, urllib.request.HTTPRedirectHandler)][0]
    with pytest.raises(urllib.error.URLError):
        handler.redirect_request(urllib.request.Request(A_URL), None, 302, "Found",
                                 {"location": "http://internal.example/x"},
                                 "http://internal.example/x")


def test_a_cgnat_address_is_still_refused(monkeypatch):
    """100.64.0.0/10 is neither private nor loopback; `is_global` is what
    catches it, and the allow rule must stay an allow rule."""
    import ipaddress

    monkeypatch.setattr(pp, "_resolve",
                        lambda host, port: [ipaddress.ip_address("100.64.1.1")])
    problem, pin = pp._public_destination(A_URL)
    assert problem and pin is None, "a carrier-grade NAT address was allowed"


def test_the_environment_proxy_is_not_used(resolves, dialled, monkeypatch):
    """`HTTP_PROXY` must not capture the one request that is direct by design."""
    monkeypatch.setenv("HTTP_PROXY", "http://192.0.2.99:3128")
    monkeypatch.setenv("HTTPS_PROXY", "http://192.0.2.99:3128")

    problem, pin = pp._public_destination(A_URL)
    opener = pp._pinned_opener(pin)
    assert not any(isinstance(h, urllib.request.ProxyHandler) and h.proxies
                   for h in opener.handlers), "an environment proxy was installed"

    with pytest.raises(Exception):
        opener.open(A_URL, timeout=1)

    assert dialled == [A_IP], f"the request went to {dialled}, not straight to {A_IP}"
