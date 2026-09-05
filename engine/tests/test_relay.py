"""Proof that the proxy-auth relay actually carries traffic.

The relay exists so a browser that cannot authenticate a proxy still gets one
(see relay.py). "It started and returned a URL" would prove nothing about that,
so every test here runs real bytes through it into a stub upstream written in
this file: an HTTP proxy that answers 407 without the right
``Proxy-Authorization``, and a SOCKS5 proxy that rejects the wrong password.

The stubs deliberately share no code with ``relay`` — a stub built from the
relay's own parsing could agree with a broken relay. The first test proves the
stub really does refuse unauthenticated requests, so the rest cannot pass
vacuously.
"""

import base64
import http.client
import socket
import socketserver
import threading
import time

import pytest

from numera.models import Proxy
from numera import relay as relay_mod
from numera.relay import ProxyRelay, ProxyRelayError

USER = "relay-user"
PASSWORD = "s3cr3t pass"          # a space, so the base64 is not a trivial token
MARKER = "UPSTREAM-SERVED"


# --------------------------------------------------------------------------
# Stub servers
# --------------------------------------------------------------------------
class _Stub(socketserver.ThreadingTCPServer):
    daemon_threads = True         # test teardown must never wait on a stub


def _head(sock):
    buf = b""
    while b"\r\n\r\n" not in buf:
        chunk = sock.recv(4096)
        if not chunk:
            return b"", b""
        buf += chunk
    head, _, rest = buf.partition(b"\r\n\r\n")
    return head, rest


def _exact(sock, n):
    buf = b""
    while len(buf) < n:
        chunk = sock.recv(n - len(buf))
        if not chunk:
            raise ConnectionError("stub: short read")
        buf += chunk
    return buf


def _respond(sock, body):
    sock.sendall(b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\n"
                 + f"Content-Length: {len(body)}\r\n".encode()
                 + b"Connection: close\r\n\r\n" + body)


def _pipe_both(a, b):
    def pump(src, dst):
        try:
            while True:
                data = src.recv(65536)
                if not data:
                    break
                dst.sendall(data)
        except OSError:
            pass
        try:
            dst.shutdown(socket.SHUT_WR)
        except OSError:
            pass

    other = threading.Thread(target=pump, args=(b, a), daemon=True)
    other.start()
    pump(a, b)
    other.join(timeout=5)


class _UpstreamProxy(socketserver.BaseRequestHandler):
    """An HTTP proxy that serves nothing until it is authenticated."""

    def handle(self):
        head, _rest = _head(self.request)
        if not head:
            return
        lines = head.split(b"\r\n")
        request_line = lines[0].decode("latin-1")
        creds = None
        for line in lines[1:]:
            if line.lower().startswith(b"proxy-authorization: basic "):
                creds = base64.b64decode(line.rsplit(b" ", 1)[1]).decode("utf-8")
        self.server.seen.append((request_line, creds))

        if creds != f"{USER}:{PASSWORD}":
            self.request.sendall(
                b"HTTP/1.1 407 Proxy Authentication Required\r\n"
                b'Proxy-Authenticate: Basic realm="stub"\r\n'
                b"Content-Length: 0\r\nConnection: close\r\n\r\n")
            return

        parts = request_line.split()
        if parts[0] == "CONNECT":
            host, _, port = parts[1].rpartition(":")
            try:
                dest = socket.create_connection((host, int(port)), 5)
            except OSError:
                self.request.sendall(
                    b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n")
                return
            self.request.sendall(b"HTTP/1.1 200 Connection established\r\n\r\n")
            _pipe_both(self.request, dest)
            dest.close()
            return
        _respond(self.request, f"{MARKER} {parts[1]}".encode())


class _UpstreamSocks(socketserver.BaseRequestHandler):
    """A SOCKS5 proxy that requires username/password and domain targets.

    Refusing address types other than 3 is deliberate: it pins the claim that
    the relay hands hostnames over unresolved instead of leaking DNS locally.
    """

    def handle(self):
        sock = self.request
        _version, nmethods = _exact(sock, 2)
        methods = _exact(sock, nmethods)
        if 2 not in methods:
            self.server.seen.append(("no-password-method-offered", None))
            sock.sendall(b"\x05\xff")
            return
        sock.sendall(b"\x05\x02")

        _exact(sock, 1)                                   # auth subnegotiation version
        user = _exact(sock, _exact(sock, 1)[0]).decode("utf-8")
        password = _exact(sock, _exact(sock, 1)[0]).decode("utf-8")
        ok = (user, password) == (USER, PASSWORD)
        self.server.seen.append(("auth", f"{user}:{password}"))
        sock.sendall(b"\x01\x00" if ok else b"\x01\x01")
        if not ok:
            return

        _version, _cmd, _rsv, atyp = _exact(sock, 4)
        if atyp != 3:
            self.server.seen.append(("address-type", atyp))
            sock.sendall(b"\x05\x08\x00\x01" + b"\x00" * 6)
            return
        host = _exact(sock, _exact(sock, 1)[0]).decode("utf-8")
        port = int.from_bytes(_exact(sock, 2), "big")
        self.server.seen.append(("connect", f"{host}:{port}"))
        try:
            dest = socket.create_connection((host, port), 5)
        except OSError:
            sock.sendall(b"\x05\x05\x00\x01" + b"\x00" * 6)
            return
        sock.sendall(b"\x05\x00\x00\x01" + b"\x00" * 6)
        _pipe_both(sock, dest)
        dest.close()


class _Origin(socketserver.BaseRequestHandler):
    """A plain web server. It echoes the request target it was actually given,
    which is how the SOCKS tests can tell an origin-form rewrite happened."""

    def handle(self):
        head, rest = _head(self.request)
        if not head:
            return
        lines = head.split(b"\r\n")
        target = lines[0].split()[1].decode("latin-1")
        length = 0
        for line in lines[1:]:
            if line.lower().startswith(b"content-length:"):
                length = int(line.split(b":", 1)[1])
        body = rest
        while len(body) < length:
            chunk = self.request.recv(65536)
            if not chunk:
                break
            body += chunk
        _respond(self.request, f"{MARKER} {target} body={len(body)}".encode())


# --------------------------------------------------------------------------
# Fixtures and helpers
# --------------------------------------------------------------------------
@pytest.fixture
def serve():
    running = []

    def _start(handler):
        srv = _Stub(("127.0.0.1", 0), handler)
        srv.seen = []
        # 0.1s poll, not the stdlib's 0.5: shutdown() waits for it, once per stub.
        threading.Thread(target=srv.serve_forever, args=(0.1,), daemon=True).start()
        running.append(srv)
        return srv

    yield _start
    for srv in running:
        srv.shutdown()
        srv.server_close()


def _port(srv):
    return srv.server_address[1]


def _proxy(srv, scheme="http", password=PASSWORD, user=USER):
    return Proxy(server=f"{scheme}://127.0.0.1:{_port(srv)}",
                 username=user, password=password)


def _request(relay, url, method="GET", body=None):
    """Send one absolute-form request through the relay, as a browser would."""
    conn = http.client.HTTPConnection("127.0.0.1", relay.port, timeout=10)
    try:
        conn.request(method, url, body=body)
        resp = conn.getresponse()
        return resp.status, resp.read().decode("utf-8", "replace")
    finally:
        conn.close()


def _through_tunnel(relay, origin, path="/tunnelled"):
    """CONNECT to the origin through the relay, then speak HTTP inside it."""
    conn = http.client.HTTPConnection("127.0.0.1", relay.port, timeout=10)
    conn.set_tunnel("127.0.0.1", _port(origin))
    try:
        conn.request("GET", path)
        resp = conn.getresponse()
        return resp.status, resp.read().decode("utf-8", "replace")
    finally:
        conn.close()


# --------------------------------------------------------------------------
# The stub is a real gate
# --------------------------------------------------------------------------
def test_the_stub_upstream_refuses_an_unauthenticated_request(serve):
    """Without this, every test below could pass against an open proxy."""
    up = serve(_UpstreamProxy)
    conn = http.client.HTTPConnection("127.0.0.1", _port(up), timeout=10)
    try:
        conn.request("GET", "http://numera.test/")
        assert conn.getresponse().status == 407
    finally:
        conn.close()


# --------------------------------------------------------------------------
# HTTP upstream
# --------------------------------------------------------------------------
def test_relay_supplies_the_upstream_credentials(serve):
    up = serve(_UpstreamProxy)
    with ProxyRelay(_proxy(up)) as relay:
        status, body = _request(relay, "http://numera.test/page?q=1")
    assert status == 200, body
    assert MARKER in body and "http://numera.test/page?q=1" in body
    assert up.seen[-1][1] == f"{USER}:{PASSWORD}"


def test_a_wrong_password_still_fails(serve):
    up = serve(_UpstreamProxy)
    with ProxyRelay(_proxy(up, password="wrong")) as relay:
        status, _ = _request(relay, "http://numera.test/")
    assert status == 407
    assert up.seen[-1][1] == f"{USER}:wrong"


def test_the_browsers_own_proxy_authorization_is_replaced(serve):
    """A stale header from the client must not shadow the real credentials."""
    up = serve(_UpstreamProxy)
    with ProxyRelay(_proxy(up)) as relay:
        conn = http.client.HTTPConnection("127.0.0.1", relay.port, timeout=10)
        try:
            conn.request("GET", "http://numera.test/",
                         headers={"Proxy-Authorization": "Basic bm9wZTpub3Bl"})
            status = conn.getresponse().status
        finally:
            conn.close()
    assert status == 200
    assert up.seen[-1][1] == f"{USER}:{PASSWORD}"


def test_connect_tunnel_reaches_the_origin(serve):
    origin = serve(_Origin)
    up = serve(_UpstreamProxy)
    with ProxyRelay(_proxy(up)) as relay:
        status, body = _through_tunnel(relay, origin)
    assert status == 200, body
    assert MARKER in body and "/tunnelled" in body
    connects = [line for line, _ in up.seen if line.startswith("CONNECT ")]
    assert connects, f"the upstream never saw a CONNECT: {up.seen}"
    assert up.seen[0][1] == f"{USER}:{PASSWORD}"


# --------------------------------------------------------------------------
# SOCKS5 upstream — the case Chromium cannot do at all
# --------------------------------------------------------------------------
def test_socks5_upstream_carries_a_connect_tunnel(serve):
    origin = serve(_Origin)
    up = serve(_UpstreamSocks)
    with ProxyRelay(_proxy(up, "socks5")) as relay:
        status, body = _through_tunnel(relay, origin, "/socks-tunnel")
    assert status == 200, body
    assert MARKER in body and "/socks-tunnel" in body
    assert ("auth", f"{USER}:{PASSWORD}") in up.seen
    assert ("connect", f"127.0.0.1:{_port(origin)}") in up.seen


def test_socks5_upstream_rewrites_an_absolute_request(serve):
    """A SOCKS proxy connects us to the site itself, which would reject the
    absolute form a browser sends to a proxy."""
    origin = serve(_Origin)
    up = serve(_UpstreamSocks)
    with ProxyRelay(_proxy(up, "socks5")) as relay:
        status, body = _request(relay, f"http://127.0.0.1:{_port(origin)}/plain?x=2")
    assert status == 200, body
    assert body == f"{MARKER} /plain?x=2 body=0"


def test_socks5_upstream_forwards_a_request_body(serve):
    origin = serve(_Origin)
    up = serve(_UpstreamSocks)
    payload = b"n" * 5000
    with ProxyRelay(_proxy(up, "socks5")) as relay:
        status, body = _request(relay, f"http://127.0.0.1:{_port(origin)}/post",
                                method="POST", body=payload)
    assert status == 200, body
    assert body.endswith(f"body={len(payload)}")


def test_socks5_wrong_password_is_reported_not_swallowed(serve):
    up = serve(_UpstreamSocks)
    with ProxyRelay(_proxy(up, "socks5", password="wrong")) as relay:
        status, body = _request(relay, "http://numera.test/")
    assert status == 502
    assert "rejected the credentials" in body


def test_socks4_with_credentials_is_refused_up_front():
    with pytest.raises(ProxyRelayError) as excinfo:
        ProxyRelay(Proxy(server="socks4://127.0.0.1:1080", username="u", password="p"))
    assert "SOCKS4" in str(excinfo.value)


# --------------------------------------------------------------------------
# Failure handling
# --------------------------------------------------------------------------
def test_a_dead_upstream_answers_502_rather_than_hanging():
    closed = socket.socket()
    closed.bind(("127.0.0.1", 0))
    dead_port = closed.getsockname()[1]
    closed.close()

    proxy = Proxy(server=f"http://127.0.0.1:{dead_port}",
                  username=USER, password=PASSWORD)
    started = time.monotonic()
    with ProxyRelay(proxy) as relay:
        status, body = _request(relay, "http://numera.test/")
    assert status == 502
    assert "upstream proxy" in body
    assert time.monotonic() - started < 10, "a refused upstream should fail fast"


def test_a_malformed_request_gets_400_not_a_crash(serve):
    up = serve(_UpstreamProxy)
    with ProxyRelay(_proxy(up)) as relay:
        sock = socket.create_connection(("127.0.0.1", relay.port), 10)
        try:
            sock.sendall(b"NOT-A-PROXY-REQUEST\r\n\r\n")
            assert b"400" in sock.recv(4096)
        finally:
            sock.close()


# --------------------------------------------------------------------------
# Lifecycle — a leaked port or thread is the defect, not a detail
# --------------------------------------------------------------------------
def _relay_threads():
    return [t.name for t in threading.enumerate() if t.name.startswith("numera-relay")]


def test_close_releases_the_port_and_every_thread(serve):
    up = serve(_UpstreamProxy)
    relay = ProxyRelay(_proxy(up)).start()
    port = relay.port
    assert _request(relay, "http://numera.test/")[0] == 200
    relay.close()

    assert not _relay_threads(), f"threads survived close(): {_relay_threads()}"
    # Connect, don't rebind. Binding answers "has the kernel finished with the
    # port", which is a different question: Linux holds it in TIME_WAIT after a
    # clean close, so the bind failed on a relay that had shut down perfectly.
    # SO_REUSEADDR is not the fix either - on Windows it permits binding a port
    # that something *is* still listening on, so the test would pass either way.
    # A refused connection means nothing is listening, on both platforms.
    probe = socket.socket()
    probe.settimeout(5)
    try:
        with pytest.raises(OSError):
            probe.connect(("127.0.0.1", port))
    finally:
        probe.close()


def test_the_tunnel_thread_ends_when_its_sockets_are_pulled(tmp_path):
    """`_tunnel` directly, with both sockets closed under it.

    This is what `close()` does, and on Linux it left the thread wedged: closing
    a file descriptor removes it from the epoll interest list, so the selector
    emptied while the loop still believed two directions were open, and an
    untimed `select()` on an empty set never returns. `server_close()` then
    joined a thread that could not finish and the whole run hung - visible only
    on CI, because Windows' `SelectSelector` raises on a closed socket instead.

    Driving `_tunnel` directly rather than through a real relay is deliberate:
    the platform difference is in the selector, and this reproduces the wedge on
    both platforms rather than only where epoll is used.
    """
    a, b = socket.socketpair()
    c, d = socket.socketpair()
    done = threading.Event()

    def pump():
        try:
            relay_mod._tunnel(a, c)
        finally:
            done.set()

    t = threading.Thread(target=pump, name="tunnel-under-test", daemon=True)
    t.start()

    # Let it get as far as select(), then pull both sides exactly as close() does.
    for sock in (a, c):
        try:
            sock.shutdown(socket.SHUT_RDWR)
        except OSError:
            pass
        sock.close()

    assert done.wait(10), \
        "the tunnel thread outlived its sockets - close() would join it forever"
    t.join(5)
    assert not t.is_alive()
    for sock in (b, d):
        sock.close()


def test_close_does_not_hang_on_an_open_tunnel(serve):
    """An idle tunnel never ends by itself, so close() has to break it."""
    origin = serve(_Origin)
    up = serve(_UpstreamProxy)
    relay = ProxyRelay(_proxy(up)).start()
    tunnel = http.client.HTTPConnection("127.0.0.1", relay.port, timeout=10)
    tunnel.set_tunnel("127.0.0.1", _port(origin))
    tunnel.connect()                       # CONNECT established, then left idle

    started = time.monotonic()
    relay.close()
    elapsed = time.monotonic() - started
    tunnel.close()

    assert elapsed < 5, f"close() waited {elapsed:.1f}s on an idle tunnel"
    assert not _relay_threads(), f"threads survived close(): {_relay_threads()}"


def test_start_and_close_are_idempotent(serve):
    up = serve(_UpstreamProxy)
    relay = ProxyRelay(_proxy(up))
    assert relay.start() is relay.start()
    port = relay.port
    relay.close()
    relay.close()
    assert port and not _relay_threads()


def test_two_relays_take_different_ephemeral_ports(serve):
    up = serve(_UpstreamProxy)
    with ProxyRelay(_proxy(up)) as first, ProxyRelay(_proxy(up)) as second:
        assert first.port != second.port
        assert first.url == f"http://127.0.0.1:{first.port}"


def test_the_relay_is_not_reachable_off_the_machine(serve):
    """It is an open proxy holding real credentials — the bind is the only
    thing keeping it private."""
    up = serve(_UpstreamProxy)
    with ProxyRelay(_proxy(up)) as relay:
        assert relay._server.server_address[0] == "127.0.0.1"
        try:
            lan = socket.gethostbyname(socket.gethostname())
        except OSError:
            lan = "127.0.0.1"
        if lan.startswith("127."):
            pytest.skip("no non-loopback address here to aim at")
        with pytest.raises(OSError):
            socket.create_connection((lan, relay.port), timeout=3).close()


def test_port_is_unavailable_before_start(serve):
    up = serve(_UpstreamProxy)
    relay = ProxyRelay(_proxy(up))
    with pytest.raises(ProxyRelayError):
        _ = relay.port


def test_a_proxy_without_a_port_is_refused():
    with pytest.raises(ProxyRelayError):
        ProxyRelay(Proxy(server="http://127.0.0.1", username="u", password="p"))


# --------------------------------------------------------------------------
# The drivers.py hookup decision
# --------------------------------------------------------------------------
def test_only_a_credentialed_proxy_gets_a_relay():
    """The relay is for what the browser cannot do; an open proxy is not that,
    and an extra hop on every launch would be a new failure surface."""
    from numera import generate
    from numera.drivers import _browser_proxy
    from numera.models import Profile

    def _cfg(proxy):
        return _browser_proxy(Profile(name="p", fingerprint=generate(seed=5),
                                      proxy=proxy))

    assert _cfg(None) == (None, None)

    plain = Proxy(server="http://127.0.0.1:1")
    assert _cfg(plain) == ({"server": "http://127.0.0.1:1"}, None)

    config, relay = _cfg(Proxy(server="http://127.0.0.1:1", username="u", password="p"))
    try:
        assert relay is not None
        assert config == {"server": relay.url}
    finally:
        if relay:
            relay.close()
    assert not _relay_threads()
