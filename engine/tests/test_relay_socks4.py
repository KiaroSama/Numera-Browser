"""The relay has to actually speak SOCKS4, not merely accept the scheme.

`proxypool.check()` used to answer `ok=None` for a SOCKS4 proxy with "the
protocol has no authentication to test". That was the wrong reason for the wrong
thing - a health check measures whether the proxy connects and which IP it exits
from, and neither involves authentication. The real gap was that the relay only
spoke SOCKS5, so the Test button on a SOCKS4 row reported a limitation that was
not the one it named.

A fake SOCKS4 proxy in front of a local HTTP server, so the handshake is
exercised byte for byte with no network.
"""
import socket
import threading
from http.server import BaseHTTPRequestHandler, HTTPServer
from urllib.request import ProxyHandler, build_opener

import pytest

from numera.models import Proxy
from numera.relay import ProxyRelay, ProxyRelayError, _tunnel

BODY = b"through the proxy"


class Site:
    def __enter__(self):
        class H(BaseHTTPRequestHandler):
            protocol_version = "HTTP/1.1"

            def do_GET(self):
                self.send_response(200)
                self.send_header("Content-Length", str(len(BODY)))
                self.end_headers()
                self.wfile.write(BODY)

            def log_message(self, *a):
                pass

        self.server = HTTPServer(("127.0.0.1", 0), H)
        self.port = self.server.server_port
        threading.Thread(target=self.server.serve_forever, daemon=True).start()
        return self

    def __exit__(self, *exc):
        self.server.shutdown()
        self.server.server_close()


class FakeSocks4:
    """Enough of RFC 1928's predecessor to answer one CONNECT.

    Records the request so the test can assert the SOCKS4a form was used - the
    bogus 0.0.0.x address plus a hostname - rather than a resolved address.
    """

    def __init__(self, *, grant=True, to_port=None):
        self.grant = grant
        self.to_port = to_port
        self.seen = {}

    def __enter__(self):
        self.sock = socket.socket()
        self.sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self.sock.bind(("127.0.0.1", 0))
        self.sock.listen(8)
        self.port = self.sock.getsockname()[1]
        threading.Thread(target=self._serve, daemon=True).start()
        return self

    def __exit__(self, *exc):
        try:
            self.sock.close()
        except OSError:
            pass

    def _serve(self):
        while True:
            try:
                client, _ = self.sock.accept()
            except OSError:
                return
            threading.Thread(target=self._one, args=(client,), daemon=True).start()

    def _one(self, client):
        try:
            head = _recv(client, 8)
            self.seen["version"] = head[0]
            self.seen["command"] = head[1]
            self.seen["port"] = int.from_bytes(head[2:4], "big")
            self.seen["address"] = tuple(head[4:8])
            self.seen["ident"] = _until_nul(client).decode()
            # SOCKS4a: address 0.0.0.x means a hostname follows the user id.
            if head[4:7] == b"\x00\x00\x00" and head[7] != 0:
                self.seen["host"] = _until_nul(client).decode()
            if not self.grant:
                client.sendall(b"\x00\x5b" + b"\x00" * 6)
                client.close()
                return
            client.sendall(b"\x00\x5a" + b"\x00" * 6)
            upstream = socket.create_connection(("127.0.0.1", self.to_port))
            # The relay's own pump. Writing a second one here was a harness bug,
            # not a product one: closing both sockets when either direction
            # ended cut the response between its headers and its body.
            _tunnel(client, upstream)
        except Exception:                                       # noqa: BLE001
            try:
                client.close()
            except OSError:
                pass


def _recv(sock, n):
    buf = b""
    while len(buf) < n:
        chunk = sock.recv(n - len(buf))
        if not chunk:
            raise ConnectionError("closed mid-handshake")
        buf += chunk
    return buf


def _until_nul(sock):
    buf = b""
    while True:
        b1 = sock.recv(1)
        if not b1 or b1 == b"\x00":
            return buf
        buf += b1


def fetch_through(relay_url, url, timeout=10):
    opener = build_opener(ProxyHandler({"http": relay_url, "https": relay_url}))
    with opener.open(url, timeout=timeout) as r:
        return r.read()


def test_a_socks4_proxy_carries_a_real_request(tmp_path):
    with Site() as site, FakeSocks4(to_port=site.port) as socks:
        upstream = Proxy(server=f"socks4://127.0.0.1:{socks.port}")
        with ProxyRelay(upstream).start() as relay:
            body = fetch_through(relay.url, f"http://127.0.0.1:{site.port}/")
    assert body == BODY
    assert socks.seen["version"] == 4
    assert socks.seen["command"] == 1


def test_the_destination_is_resolved_by_the_proxy_not_by_us(tmp_path):
    """SOCKS4a form. Resolving here would put a DNS lookup for every site on the
    local resolver, which is the leak the SOCKS5 path already avoids."""
    with Site() as site, FakeSocks4(to_port=site.port) as socks:
        upstream = Proxy(server=f"socks4://127.0.0.1:{socks.port}")
        with ProxyRelay(upstream).start() as relay:
            fetch_through(relay.url, f"http://localhost:{site.port}/")
    assert socks.seen["address"][:3] == (0, 0, 0), socks.seen["address"]
    assert socks.seen["address"][3] != 0
    assert socks.seen["host"] == "localhost"
    assert socks.seen["port"] == site.port


def test_the_user_id_goes_in_the_ident_field():
    """SOCKS4's one credential-ish field. Some providers key on it."""
    with Site() as site, FakeSocks4(to_port=site.port) as socks:
        upstream = Proxy(server=f"socks4://127.0.0.1:{socks.port}", username="tenant-7")
        with ProxyRelay(upstream).start() as relay:
            fetch_through(relay.url, f"http://127.0.0.1:{site.port}/")
    assert socks.seen["ident"] == "tenant-7"


def test_a_refusal_is_reported_as_a_refusal():
    """`ok=None` for everything is what this replaces: a rejected CONNECT has to
    read as a failed proxy, not as an untestable one."""
    with Site() as site, FakeSocks4(grant=False, to_port=site.port) as socks:
        upstream = Proxy(server=f"socks4://127.0.0.1:{socks.port}")
        with ProxyRelay(upstream).start() as relay:
            with pytest.raises(Exception):
                fetch_through(relay.url, f"http://127.0.0.1:{site.port}/", timeout=5)
            assert relay.last_error, "the relay recorded no reason"
            assert "SOCKS4" in relay.last_error


def test_a_socks4_proxy_with_a_password_is_refused_with_the_real_reason():
    """Not a gap in the relay: the protocol has a user-id field and no password
    field, so there is nowhere on the wire for one to go."""
    with pytest.raises(ProxyRelayError, match="no password in the protocol"):
        ProxyRelay(Proxy(server="socks4://1.2.3.4:1080",
                         username="u", password="p"))


def test_socks4_is_no_longer_reported_as_untestable():
    """The old `check()` short-circuited on the type before trying anything."""
    import inspect

    from numera import proxypool

    src = inspect.getsource(proxypool.check)
    assert "cannot be health-checked" not in src
    assert "no authentication to test" not in src
