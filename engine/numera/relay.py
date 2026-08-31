"""
Local proxy-authentication relay.

Two of the engines Numera ships cannot authenticate an upstream proxy at all:
fingerprint-chromium has no password support on ``--proxy-server``, and
Firefox/camoufox cannot do authenticated SOCKS. Chromium refuses SOCKS
authentication outright — its own error, ``Browser does not support socks5
proxy authentication``, is what killed the real proxy list in BUGS.md B4.

One component fixes all three: an *unauthenticated* HTTP proxy on loopback that
forwards to the real upstream and supplies the credentials itself. The browser
is then pointed at ``http://127.0.0.1:<port>`` with nothing to authenticate,
which every engine can do.

    with ProxyRelay(profile.proxy) as relay:
        launch(browser, proxy={"server": relay.url})

Security: an open proxy is exactly what this is, so it binds ``127.0.0.1`` only
— nothing off the machine can reach it — and takes an ephemeral port, because
several profiles launch at once and must not collide.

Hostnames are handed to the upstream unresolved (absolute-form URLs for an HTTP
proxy, the SOCKS5 domain address type otherwise), so the browser's DNS keeps
going out through the proxy rather than leaking around it.
"""

from __future__ import annotations

import base64
import selectors
import socket
import socketserver
import threading
from typing import Optional
from urllib.parse import urlsplit

from .models import Proxy

__all__ = ["ProxyRelay", "ProxyRelayError"]

_CONNECT_TIMEOUT = 15      # seconds — a dead upstream has to fail, not hang
_CHUNK = 65536
_HEAD_LIMIT = 64 * 1024    # a request head bigger than this is not a browser

_HTTP_SCHEMES = {"http", "https"}
_SOCKS5_SCHEMES = {"socks", "socks5", "socks5h"}
_SOCKS4_SCHEMES = {"socks4", "socks4a"}
_SOCKS_SCHEMES = _SOCKS5_SCHEMES | _SOCKS4_SCHEMES

# Hop-by-hop headers: the relay terminates that hop, so the browser's versions
# are dropped rather than forwarded.
_DROP_HEADERS = (b"connection:", b"proxy-connection:",
                 b"proxy-authorization:", b"keep-alive:")

_REASON = {400: "Bad Request", 502: "Bad Gateway"}

# RFC 1928 §6 reply codes, so a refusal says what happened instead of "5".
_SOCKS_REPLY = {
    1: "general SOCKS server failure", 2: "connection not allowed by ruleset",
    3: "network unreachable", 4: "host unreachable", 5: "connection refused",
    6: "TTL expired", 7: "command not supported", 8: "address type not supported",
}


class ProxyRelayError(RuntimeError):
    """The upstream proxy could not be reached, or refused us."""


# --------------------------------------------------------------------------
# HTTP head handling
# --------------------------------------------------------------------------
def _read_head(sock: socket.socket) -> tuple[bytes, bytes]:
    """Read one request head. Returns it without the blank line, plus whatever
    body bytes arrived in the same read."""
    buf = b""
    while b"\r\n\r\n" not in buf:
        if len(buf) > _HEAD_LIMIT:
            raise ValueError("request head too large")
        chunk = sock.recv(_CHUNK)
        if not chunk:
            raise ValueError("client closed before sending a request")
        buf += chunk
    head, _, rest = buf.partition(b"\r\n\r\n")
    return head, rest


def _rebuild_head(head: bytes, request_line: bytes, auth: Optional[bytes],
                  close_after: bool) -> bytes:
    """Reassemble a head with our own request line, credentials and connection
    handling.

    ``close_after`` is set for everything except CONNECT, so the relay serves
    exactly one plain-HTTP request per browser connection. That is what keeps
    every request authenticated — credentials are per-request, and a second
    request arriving on a reused socket would carry none — and on a SOCKS
    upstream it also stops a request for a *different* host arriving on a
    socket already wired to the first one.
    """
    lines = [request_line]
    for line in head.split(b"\r\n")[1:]:
        if line and not line.lower().startswith(_DROP_HEADERS):
            lines.append(line)
    if auth:
        lines.append(b"Proxy-Authorization: Basic " + auth)
    if close_after:
        lines.append(b"Connection: close")
    return b"\r\n".join(lines) + b"\r\n\r\n"


def _target_host_port(target: bytes, connect: bool) -> tuple[str, int]:
    """Where the browser wants to go: ``host:port`` after CONNECT, or the host
    of the absolute URL every other proxied request carries."""
    text = target.decode("latin-1")
    if connect:
        host, _, port = text.rpartition(":")
        if not host or not port.isdigit():
            raise ValueError(f"CONNECT target is not host:port: {text!r}")
        return host.strip("[]"), int(port)
    parts = urlsplit(text)
    if not parts.hostname:
        raise ValueError(f"expected an absolute URL from the browser, got {text!r}")
    return parts.hostname, parts.port or (443 if parts.scheme == "https" else 80)


def _origin_request_line(request_line: bytes) -> bytes:
    """``GET http://host/a?b HTTP/1.1`` -> ``GET /a?b HTTP/1.1``.

    Only the SOCKS path needs this: there the relay is talking to the site
    itself, and an origin server rejects the absolute form a proxy expects.
    """
    method, target, version = request_line.split(None, 2)
    parts = urlsplit(target.decode("latin-1"))
    path = parts.path or "/"
    if parts.query:
        path += "?" + parts.query
    return b" ".join([method, path.encode("latin-1"), version])


def _reply(sock: socket.socket, status: int, message: str) -> None:
    """Answer the browser directly — the relay itself failed, not the site."""
    body = message.encode("utf-8", "replace")
    try:
        sock.sendall(
            f"HTTP/1.1 {status} {_REASON[status]}\r\n"
            "Content-Type: text/plain; charset=utf-8\r\n"
            f"Content-Length: {len(body)}\r\n"
            "Connection: close\r\n\r\n".encode() + body)
    except OSError:
        pass


# --------------------------------------------------------------------------
# SOCKS5 client (RFC 1928 + RFC 1929)
# --------------------------------------------------------------------------
def _recv_exact(sock: socket.socket, n: int) -> bytes:
    buf = b""
    while len(buf) < n:
        chunk = sock.recv(n - len(buf))
        if not chunk:
            raise ProxyRelayError("SOCKS5 upstream closed the connection mid-handshake")
        buf += chunk
    return buf


def _socks5_connect(sock: socket.socket, host: str, port: int,
                    user: Optional[str], password: Optional[str]) -> None:
    """Open a tunnel to ``host:port`` through an already-connected SOCKS5 proxy.

    The destination goes over as a domain name (address type 3) rather than an
    address, so the upstream resolves it — resolving here would put a DNS
    lookup for every site the browser visits on the local resolver.
    """
    sock.sendall(b"\x05\x02\x00\x02" if user else b"\x05\x01\x00")
    version, method = _recv_exact(sock, 2)
    if version != 5:
        raise ProxyRelayError(
            f"upstream did not answer as SOCKS5 (version byte {version})")
    if method == 0xFF:
        raise ProxyRelayError(
            "SOCKS5 upstream accepted none of our authentication methods"
            + ("" if user else " — it likely needs a username and password"))
    if method == 0x02:
        u = (user or "").encode("utf-8")
        p = (password or "").encode("utf-8")
        if len(u) > 255 or len(p) > 255:
            raise ProxyRelayError("SOCKS5 username/password may not exceed 255 bytes")
        sock.sendall(b"\x01" + bytes([len(u)]) + u + bytes([len(p)]) + p)
        _, status = _recv_exact(sock, 2)
        if status != 0:
            raise ProxyRelayError("SOCKS5 upstream rejected the credentials")
    elif method != 0x00:
        raise ProxyRelayError(f"SOCKS5 upstream asked for authentication method "
                              f"{method}, which the relay does not speak")

    target = host.encode("utf-8")
    if not target or len(target) > 255:
        raise ProxyRelayError(f"hostname does not fit a SOCKS5 request: {host!r}")
    sock.sendall(b"\x05\x01\x00\x03" + bytes([len(target)]) + target
                 + port.to_bytes(2, "big"))
    _, reply, _, atyp = _recv_exact(sock, 4)
    if reply != 0:
        raise ProxyRelayError(
            f"SOCKS5 upstream refused CONNECT to {host}:{port} "
            f"({_SOCKS_REPLY.get(reply, reply)})")
    # Drain the bound address, or the tunnel's first bytes would be this reply's.
    length = {1: 4, 4: 16}.get(atyp)
    if length is None:
        if atyp != 3:
            raise ProxyRelayError(f"SOCKS5 upstream sent unknown address type {atyp}")
        length = _recv_exact(sock, 1)[0]
    _recv_exact(sock, length + 2)


_SOCKS4_REPLY = {
    91: "request rejected or failed",
    92: "rejected: the proxy could not reach an identd on this machine",
    93: "rejected: the proxy's identd disagreed with the user id we sent",
}


def _socks4_connect(sock: socket.socket, host: str, port: int,
                    user: Optional[str]) -> None:
    """Open a tunnel through an already-connected SOCKS4 proxy.

    The SOCKS4a form, so the destination goes over as a name and the *upstream*
    resolves it - the same reason the SOCKS5 path uses address type 3. The
    marker for it is a bogus address 0.0.0.x.

    SOCKS4 has a user-id field but no password, which is why `ProxyRelay`
    refuses a SOCKS4 proxy that was given one: there is nowhere to put it. That
    is a real limitation of the protocol. Being unable to *health-check* one was
    not - the check only needs a connection and an exit IP, and this is it.
    """
    ident = (user or "").encode("utf-8")
    if len(ident) > 255:
        raise ProxyRelayError("SOCKS4 user id may not exceed 255 bytes")
    target = host.encode("idna" if not host.isascii() else "utf-8")
    if not target or len(target) > 255:
        raise ProxyRelayError(f"hostname does not fit a SOCKS4a request: {host!r}")
    sock.sendall(b"\x04\x01" + port.to_bytes(2, "big") + b"\x00\x00\x00\x01"
                 + ident + b"\x00" + target + b"\x00")
    reply = _recv_exact(sock, 8)
    if reply[0] != 0:
        raise ProxyRelayError(
            f"upstream did not answer as SOCKS4 (version byte {reply[0]})")
    if reply[1] != 90:
        raise ProxyRelayError(
            f"SOCKS4 upstream refused CONNECT to {host}:{port} "
            f"({_SOCKS4_REPLY.get(reply[1], reply[1])})")


# --------------------------------------------------------------------------
# Byte pump
# --------------------------------------------------------------------------
def _tunnel(client: socket.socket, upstream: socket.socket) -> None:
    """Copy both directions until each one ends.

    One thread, not two: a reader thread parked on the quiet side of an idle
    tunnel is exactly the thread that would still be alive after close().
    """
    sel = selectors.DefaultSelector()
    sel.register(client, selectors.EVENT_READ, upstream)
    sel.register(upstream, selectors.EVENT_READ, client)
    try:
        open_directions = 2
        while open_directions:
            for key, _mask in sel.select():
                data = key.fileobj.recv(_CHUNK)
                if not data:
                    sel.unregister(key.fileobj)
                    open_directions -= 1
                    try:
                        key.data.shutdown(socket.SHUT_WR)  # pass the EOF along
                    except OSError:
                        pass
                    continue
                key.data.sendall(data)
    except OSError:
        pass          # either side went away, or close() pulled the sockets
    finally:
        sel.close()


# --------------------------------------------------------------------------
# Server
# --------------------------------------------------------------------------
class _Handler(socketserver.BaseRequestHandler):
    """One browser connection: read its proxy request, open the upstream, pipe."""

    def handle(self):
        relay: ProxyRelay = self.server.relay
        client = self.request
        upstream = None
        # Named so a thread dump — or a test — can tell a leaked relay thread
        # from every other thread in a process that also runs a web server.
        threading.current_thread().name = (
            f"numera-relay-conn-{self.server.server_address[1]}")
        relay._track(client)
        try:
            try:
                head, rest = _read_head(client)
            except (OSError, ValueError):
                return          # a speculative connection, or garbage
            try:
                request_line = head.split(b"\r\n", 1)[0]
                method = request_line.split(None, 1)[0]
                host, port = _target_host_port(request_line.split(None, 2)[1],
                                               method.upper() == b"CONNECT")
            except (IndexError, ValueError) as e:
                _reply(client, 400, f"the relay could not read that request: {e}")
                return
            connect = method.upper() == b"CONNECT"

            try:
                upstream = relay._open_upstream(host, port)
            except (OSError, ProxyRelayError) as e:
                # Remember why. A client speaking HTTP sees only "502 Bad
                # Gateway" - urllib does not surface a CONNECT response body -
                # so the pool health check would otherwise report a dead
                # upstream and a wrong password identically.
                relay.last_error = str(e).splitlines()[0][:180]
                _reply(client, 502, f"relay could not reach the upstream proxy: {e}")
                return
            relay._track(upstream)

            if not relay.is_socks:
                # A real HTTP proxy already understands both CONNECT and the
                # absolute form, so only the credentials have to be added.
                upstream.sendall(
                    _rebuild_head(head, request_line, relay._auth, not connect) + rest)
            elif connect:
                # The SOCKS handshake already built the tunnel; the browser is
                # still waiting for the answer to its CONNECT.
                client.sendall(b"HTTP/1.1 200 Connection established\r\n\r\n")
            else:
                upstream.sendall(
                    _rebuild_head(head, _origin_request_line(request_line),
                                  None, True) + rest)
            _tunnel(client, upstream)
        except OSError:
            pass                # a peer went away, or close() pulled the socket
        finally:
            relay._untrack(client)
            if upstream is not None:
                relay._untrack(upstream)
                upstream.close()


class _RelayServer(socketserver.ThreadingTCPServer):
    # Non-daemon threads so server_close() joins every handler — that join is
    # what makes "closed" mean no thread survives. ProxyRelay.close() shuts the
    # live sockets down first, so the join cannot wait on an idle browser tab.
    daemon_threads = False
    relay: "ProxyRelay"


class ProxyRelay:
    """An unauthenticated HTTP proxy on loopback that forwards to ``proxy``.

    Start it, point the browser at :attr:`url`, and close it when the session
    ends. Closing is idempotent and releases the port and every thread.
    """

    def __init__(self, proxy: Proxy):
        server = proxy.server or ""
        if "://" not in server:
            server = "http://" + server      # some lists arrive as bare host:port
        parts = urlsplit(server)
        scheme = (parts.scheme or "http").lower()
        if scheme not in _HTTP_SCHEMES and scheme not in _SOCKS_SCHEMES:
            raise ProxyRelayError(
                f"the relay cannot forward to a '{scheme}' proxy — it speaks "
                f"HTTP, SOCKS4 and SOCKS5.")
        if scheme in _SOCKS4_SCHEMES and proxy.password:
            # Not a gap in the relay: SOCKS4 has a user-id field and no password
            # field, so there is nowhere for this to go on the wire.
            raise ProxyRelayError(
                "SOCKS4 has no password in the protocol, so a SOCKS4 proxy with "
                "a password cannot be used anywhere — use SOCKS5 for this one.")
        try:
            host, port = parts.hostname, parts.port
        except ValueError as e:                                # non-numeric port
            raise ProxyRelayError(f"bad upstream proxy address {proxy.server!r}: {e}")
        if not host or not port:
            raise ProxyRelayError(
                f"upstream proxy address needs a host and a port: {proxy.server!r}")

        self.is_socks = scheme in _SOCKS_SCHEMES
        self._socks4 = scheme in _SOCKS4_SCHEMES
        # Why the last upstream connection failed, for callers whose transport
        # cannot see the relay's 502 body (see the handler).
        self.last_error: Optional[str] = None
        self._host, self._port = host, port
        self._user, self._password = proxy.username, proxy.password
        self._auth = None
        if not self.is_socks and self._user is not None:
            self._auth = base64.b64encode(
                f"{self._user}:{self._password or ''}".encode("utf-8"))

        self._server: Optional[_RelayServer] = None
        self._thread: Optional[threading.Thread] = None
        self._live: set[socket.socket] = set()
        self._lock = threading.Lock()

    # -- lifecycle ---------------------------------------------------------
    def start(self) -> "ProxyRelay":
        if self._server is None:
            # Port 0: the OS picks a free one, so parallel launches can't collide.
            self._server = _RelayServer(("127.0.0.1", 0), _Handler)
            self._server.relay = self
            self._thread = threading.Thread(
                target=self._server.serve_forever,
                # The accept loop only notices shutdown() between polls, and the
                # stdlib's 0.5s default is that long added to every browser close.
                args=(0.1,),
                name=f"numera-relay-{self.port}", daemon=True)
            self._thread.start()
        return self

    def close(self) -> None:
        server, self._server = self._server, None
        if server is None:
            return
        server.shutdown()                       # stop accepting
        # Unblock handlers parked in recv() on a live connection: an open tunnel
        # never ends on its own, and server_close() below waits for all of them.
        with self._lock:
            live, self._live = list(self._live), set()
        for sock in live:
            try:
                sock.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass
            try:
                sock.close()
            except OSError:
                pass
        server.server_close()                   # frees the port, joins handlers
        if self._thread is not None:
            self._thread.join(timeout=5)
            self._thread = None

    def __enter__(self) -> "ProxyRelay":
        return self.start()

    def __exit__(self, *_exc) -> None:
        self.close()

    # -- what the browser is pointed at ------------------------------------
    @property
    def port(self) -> int:
        if self._server is None:
            raise ProxyRelayError("the relay is not running")
        return self._server.server_address[1]

    @property
    def url(self) -> str:
        return f"http://127.0.0.1:{self.port}"

    # -- internals ---------------------------------------------------------
    def _track(self, sock: socket.socket) -> None:
        with self._lock:
            if self._server is None:
                # Accepted in the instant before close() drained the live set.
                # Left alone it would park in recv() and server_close()'s join
                # would wait for it forever.
                sock.close()
                return
            self._live.add(sock)

    def _untrack(self, sock: socket.socket) -> None:
        with self._lock:
            self._live.discard(sock)

    def _open_upstream(self, host: str, port: int) -> socket.socket:
        """Connect to the upstream proxy and, for SOCKS, negotiate the tunnel."""
        # ponytail: plain TCP to the upstream. An https:// *hop* to the proxy
        # itself would need a TLS wrap here; no vendor in use offers one, and
        # in this codebase "HTTPS" labels the traffic a proxy carries, not the
        # hop (see proxypool._SCHEME_TYPE).
        sock = socket.create_connection((self._host, self._port), _CONNECT_TIMEOUT)
        try:
            if self.is_socks:
                if self._socks4:
                    _socks4_connect(sock, host, port, self._user)
                else:
                    _socks5_connect(sock, host, port, self._user, self._password)
            sock.settimeout(None)   # handshake done; a tunnel may idle for hours
        except BaseException:
            sock.close()
            raise
        return sock
