"""Real-browser proof that a credentialed proxy is carried by the relay.

The unit tests show the relay speaks the protocols. They cannot show that a
browser launched through ``drivers.py`` actually ends up using it — and in this
codebase that gap has bitten twice, with a WebRTC flag and with
``--load-extension``: both were passed on every launch and both were ignored.
So this launches a real profile and asks the *page* what it received.

The target is a reserved ``.test`` name that cannot resolve, and the stub
upstream never leaves the machine. A page that renders at all is therefore proof
the request went through the proxy rather than direct, and the stub's record of
the credentials proves the relay supplied what the browser could not.

The SOCKS case is the one that matters most: Chromium refuses SOCKS
authentication outright (``Browser does not support socks5 proxy
authentication``, BUGS.md B4), so a rendered page there is the fix itself.
"""

import base64
import socketserver
import threading

import pytest

from numera import generate
from numera.models import Profile, Proxy
from numera.store import ProfileStore

pytest.importorskip("playwright")

USER = "live-relay-user"
PASSWORD = "live pass word"
MARKER = "NUMERA-RELAY-CARRIED-THIS"
PROBE_URL = "http://numera-relay-probe.test/"
PAGE = (f"<!doctype html><html><body><h1>{MARKER}</h1></body></html>").encode()


class _Stub(socketserver.ThreadingTCPServer):
    daemon_threads = True


def _head(sock):
    buf = b""
    while b"\r\n\r\n" not in buf:
        chunk = sock.recv(4096)
        if not chunk:
            return b""
        buf += chunk
    return buf.split(b"\r\n\r\n", 1)[0]


def _serve_page(sock):
    sock.sendall(b"HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n"
                 + f"Content-Length: {len(PAGE)}\r\n".encode()
                 + b"Connection: close\r\n\r\n" + PAGE)


class _UpstreamProxy(socketserver.BaseRequestHandler):
    """HTTP proxy: serves the probe page, but only once authenticated."""

    def handle(self):
        head = _head(self.request)
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
            self.request.sendall(b"HTTP/1.1 407 Proxy Authentication Required\r\n"
                                 b'Proxy-Authenticate: Basic realm="stub"\r\n'
                                 b"Content-Length: 0\r\nConnection: close\r\n\r\n")
        elif request_line.startswith("CONNECT "):
            # Nothing here speaks TLS. Refusing quickly lets Chrome's automatic
            # https-first attempt fall back to http instead of stalling.
            self.request.sendall(b"HTTP/1.1 502 Bad Gateway\r\n"
                                 b"Content-Length: 0\r\nConnection: close\r\n\r\n")
        else:
            _serve_page(self.request)


class _UpstreamSocks(socketserver.BaseRequestHandler):
    """SOCKS5 proxy demanding username/password. Every accepted CONNECT is
    answered by this process, so the probe host never has to resolve."""

    def _exact(self, n):
        buf = b""
        while len(buf) < n:
            chunk = self.request.recv(n - len(buf))
            if not chunk:
                raise ConnectionError("stub: short read")
            buf += chunk
        return buf

    def handle(self):
        sock = self.request
        _version, nmethods = self._exact(2)
        if 2 not in self._exact(nmethods):
            sock.sendall(b"\x05\xff")
            return
        sock.sendall(b"\x05\x02")
        self._exact(1)
        user = self._exact(self._exact(1)[0]).decode("utf-8")
        password = self._exact(self._exact(1)[0]).decode("utf-8")
        ok = (user, password) == (USER, PASSWORD)
        self.server.seen.append(("auth", f"{user}:{password}"))
        sock.sendall(b"\x01\x00" if ok else b"\x01\x01")
        if not ok:
            return

        _version, _cmd, _rsv, atyp = self._exact(4)
        if atyp != 3:
            sock.sendall(b"\x05\x08\x00\x01" + b"\x00" * 6)
            return
        host = self._exact(self._exact(1)[0]).decode("utf-8")
        port = int.from_bytes(self._exact(2), "big")
        self.server.seen.append(("connect", f"{host}:{port}"))
        if port != 80:
            sock.sendall(b"\x05\x05\x00\x01" + b"\x00" * 6)   # only http here
            return
        sock.sendall(b"\x05\x00\x00\x01" + b"\x00" * 6)
        if _head(sock):
            _serve_page(sock)


@pytest.fixture
def upstream():
    running = []

    def _start(handler):
        srv = _Stub(("127.0.0.1", 0), handler)
        srv.seen = []
        threading.Thread(target=srv.serve_forever, args=(0.1,), daemon=True).start()
        running.append(srv)
        return srv

    yield _start
    for srv in running:
        srv.shutdown()
        srv.server_close()


def _visit(tmp_path, name, proxy):
    """Launch a profile whose proxy needs a password, and read the page back."""
    from numera.drivers import get

    store = ProfileStore(tmp_path / name)
    prof = Profile(name=name, fingerprint=generate(seed=23), proxy=proxy)
    store.save(prof)
    try:
        pw, ctx, page = get("playwright")(prof, store, headless=True, keep_open=True)
    except Exception as e:
        pytest.skip(f"chromium unavailable: {str(e).splitlines()[0][:80]}")
    try:
        page.goto(PROBE_URL, wait_until="domcontentloaded", timeout=45000)
        return page.content()
    finally:
        ctx.close()
        pw.stop()


def _relay_threads():
    return [t.name for t in threading.enumerate() if t.name.startswith("numera-relay")]


def test_http_proxy_credentials_reach_the_upstream(tmp_path, upstream):
    up = upstream(_UpstreamProxy)
    proxy = Proxy(server=f"http://127.0.0.1:{up.server_address[1]}",
                  username=USER, password=PASSWORD)
    content = _visit(tmp_path, "relay-http", proxy)

    assert MARKER in content, (
        "the page did not come from the stub upstream — the probe host cannot "
        "resolve, so this means the browser never reached the relay")

    probe = [creds for line, creds in up.seen if PROBE_URL in line]
    assert probe, f"the upstream never saw the probe request: {up.seen}"
    # The discriminator. Chromium can authenticate an HTTP proxy by itself, but
    # only by being refused first: it sends the request bare, takes the 407, and
    # retries. The relay authenticates from the first byte, so an unauthenticated
    # probe request here means the browser went round the relay, not through it.
    assert None not in probe, (
        f"the upstream saw an unauthenticated request — that is Chromium's own "
        f"407 retry, so the relay was not in the path: {up.seen}")
    assert probe[-1] == f"{USER}:{PASSWORD}"


def test_socks5_with_credentials_works_in_a_real_browser(tmp_path, upstream):
    """Chromium cannot do this at all without the relay (BUGS.md B4)."""
    up = upstream(_UpstreamSocks)
    proxy = Proxy(server=f"socks5://127.0.0.1:{up.server_address[1]}",
                  username=USER, password=PASSWORD)
    content = _visit(tmp_path, "relay-socks", proxy)

    assert MARKER in content, f"page never arrived through SOCKS5: {up.seen}"
    assert ("auth", f"{USER}:{PASSWORD}") in up.seen
    assert ("connect", "numera-relay-probe.test:80") in up.seen, (
        f"the hostname was not handed to the proxy unresolved: {up.seen}")


def test_closing_the_session_frees_the_relay(tmp_path, upstream):
    """The relay's lifetime is tied to the browser context in drivers.py. If
    that binding is wrong the port and its threads outlive the session."""
    up = upstream(_UpstreamProxy)
    proxy = Proxy(server=f"http://127.0.0.1:{up.server_address[1]}",
                  username=USER, password=PASSWORD)
    _visit(tmp_path, "relay-teardown", proxy)
    assert not _relay_threads(), f"relay survived the session: {_relay_threads()}"
