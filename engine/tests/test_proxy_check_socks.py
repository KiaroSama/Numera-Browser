"""A SOCKS proxy in the pool must actually be health-checked.

It used to come back as `{"ok": None, "error": "SOCKS - test it via a profile's
proxy test"}`. For a pool that is mostly SOCKS - which is the normal case, since
that is what resellers sell - the Test button then appeared to do nothing at all.

Nothing was missing: `relay.py` already speaks SOCKS5, because that is how a
credentialed SOCKS proxy reaches the browser. The check just did not use it.

These tests stand up a real SOCKS5 server on loopback rather than mocking one,
so a regression in the relay's handshake fails here too.
"""
import socket
import struct
import threading

import pytest

from numera import proxypool

USER, PASSWORD = "u", "p"


class Socks5:
    """A minimal SOCKS5 server that CONNECTs wherever it is asked.

    Records whether credentials were presented and how many tunnels it opened,
    so a test can tell "the check passed" from "the check went through us".
    """

    def __init__(self, require_auth=False, accept_password=True, refuse_connect=False):
        self.require_auth = require_auth
        self.accept_password = accept_password
        self.refuse_connect = refuse_connect
        self.authenticated = False
        self.connects = 0
        self._srv = socket.socket()
        self._srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self._srv.bind(("127.0.0.1", 0))
        self._srv.listen(8)
        self.port = self._srv.getsockname()[1]
        self._stop = False
        threading.Thread(target=self._loop, daemon=True, name="test-socks5").start()

    def close(self):
        self._stop = True
        self._srv.close()

    def _loop(self):
        while not self._stop:
            try:
                conn, _ = self._srv.accept()
            except OSError:
                return
            threading.Thread(target=self._handle, args=(conn,), daemon=True).start()

    def _handle(self, c):
        try:
            greeting = c.recv(2)
            if len(greeting) < 2:
                return
            c.recv(greeting[1])                       # the offered methods
            if self.require_auth:
                c.sendall(b"\x05\x02")
                c.recv(1)
                user = c.recv(c.recv(1)[0]).decode()
                pw = c.recv(c.recv(1)[0]).decode()
                ok = self.accept_password and (user, pw) == (USER, PASSWORD)
                self.authenticated = ok
                c.sendall(bytes([1, 0 if ok else 1]))
                if not ok:
                    return
            else:
                c.sendall(b"\x05\x00")

            head = c.recv(4)
            atyp = head[3]
            if atyp == 1:
                host = socket.inet_ntoa(c.recv(4))
            elif atyp == 3:
                host = c.recv(c.recv(1)[0]).decode()
            else:
                c.sendall(b"\x05\x08\x00\x01" + b"\0" * 6)
                return
            port = struct.unpack(">H", c.recv(2))[0]

            if self.refuse_connect:
                c.sendall(b"\x05\x05\x00\x01" + b"\0" * 6)   # connection refused
                return
            up = socket.create_connection((host, port), timeout=20)
            self.connects += 1
            c.sendall(b"\x05\x00\x00\x01" + b"\0" * 6)
            threading.Thread(target=self._pipe, args=(c, up), daemon=True).start()
            self._pipe(up, c)
        except Exception:                             # noqa: BLE001 - a test server
            pass
        finally:
            c.close()

    @staticmethod
    def _pipe(a, b):
        try:
            while True:
                data = a.recv(65536)
                if not data:
                    break
                b.sendall(data)
        except OSError:
            pass
        finally:
            for s in (a, b):
                try:
                    s.shutdown(socket.SHUT_RDWR)
                except OSError:
                    pass


@pytest.fixture
def socks():
    made = []

    def build(**kw):
        s = Socks5(**kw)
        made.append(s)
        return s

    yield build
    for s in made:
        s.close()


def test_a_socks_proxy_is_no_longer_reported_as_untestable(socks):
    """The regression that made the Test button look broken."""
    s = socks()
    res = proxypool.check({"type": "SOCKS5", "host": "127.0.0.1", "port": s.port}, timeout=25)
    assert res.get("ok") is not None, (
        "a SOCKS proxy came back with no verdict - this is the old "
        "'test it via a profile's proxy test' behaviour")


def test_a_wrong_socks_password_fails_and_says_so(socks):
    """And is distinguishable from an unreachable proxy.

    Both used to surface as urllib's "Tunnel connection failed: 502 Bad Gateway",
    because a CONNECT error body never reaches the caller. The relay records why
    it failed and the check prefers that account.
    """
    s = socks(require_auth=True, accept_password=False)
    res = proxypool.check({"type": "SOCKS5", "host": "127.0.0.1", "port": s.port,
                           "user": USER, "pass": "wrong"}, timeout=15)
    assert res["ok"] is False
    assert "credential" in res["error"].lower(), res["error"]
    assert "502" not in res["error"], "the opaque tunnel error leaked through"


def test_a_socks_upstream_that_refuses_the_target_fails(socks):
    s = socks(refuse_connect=True)
    res = proxypool.check({"type": "SOCKS5", "host": "127.0.0.1", "port": s.port}, timeout=15)
    assert res["ok"] is False
    assert res["error"], "a failure with no explanation is not actionable"


def test_a_dead_socks_port_fails_rather_than_hanging():
    sock = socket.socket()
    sock.bind(("127.0.0.1", 0))
    port = sock.getsockname()[1]
    sock.close()                                       # nothing listening now
    res = proxypool.check({"type": "SOCKS5", "host": "127.0.0.1", "port": port}, timeout=10)
    assert res["ok"] is False
    assert "502" not in res["error"], res["error"]


def test_socks4_is_declined_with_a_reason_not_a_shrug():
    """SOCKS4 genuinely cannot be checked; the message has to say why."""
    res = proxypool.check({"type": "SOCKS4", "host": "127.0.0.1", "port": 1}, timeout=5)
    assert res["ok"] is None
    assert "SOCKS4" in res["error"]


@pytest.mark.parametrize("require_auth", [False, True])
def test_the_check_routes_through_the_socks_server(socks, require_auth):
    """Proves the tunnel is real, independently of whether the internet answers.

    A green result from an echo service would not by itself show the request went
    through the proxy - it could have gone direct. The server counts its tunnels.
    """
    kw = {"require_auth": True} if require_auth else {}
    creds = {"user": USER, "pass": PASSWORD} if require_auth else {}
    s = socks(**kw)
    proxypool.check({"type": "SOCKS5", "host": "127.0.0.1", "port": s.port, **creds}, timeout=25)
    assert s.connects >= 1, "the check never opened a tunnel through the proxy"
    if require_auth:
        assert s.authenticated, "credentials were never presented to the proxy"
