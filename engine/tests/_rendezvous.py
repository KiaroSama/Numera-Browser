"""A cross-process barrier for concurrency tests: blocking sockets, no sleeps.

Children check in by name and block in `recv()`. The parent releases them one
at a time, so a test states the exact interleaving it needs and gets an
assertion - not a flake - when the code under test does not honour it.
"""
import select
import socket
import subprocess
import sys
import textwrap
from pathlib import Path

ENGINE = str(Path(__file__).resolve().parents[1])
# Long enough that a loaded machine never trips it, short enough that a real
# deadlock fails the suite instead of hanging it.
RENDEZVOUS_TIMEOUT = 90


class Rendezvous:
    """A cross-process barrier built on blocking sockets, not timing.

    Children check in by name and then block in `recv()`. The parent releases
    them one at a time, so a test can state the exact order it needs and get an
    error - not a flake - when the code under test does not honour it.
    """

    def __init__(self):
        self._srv = socket.socket()
        self._srv.bind(("127.0.0.1", 0))
        self._srv.listen(8)
        self._srv.settimeout(RENDEZVOUS_TIMEOUT)
        self.port = self._srv.getsockname()[1]
        self._peers: dict = {}

    def _peer(self, name: str):
        while name not in self._peers:
            conn, _ = self._srv.accept()           # blocks; no polling, no sleep
            conn.settimeout(RENDEZVOUS_TIMEOUT)
            stream = conn.makefile("rw", encoding="utf-8")
            self._peers[stream.readline().strip()] = (conn, stream)
        return self._peers[name]

    def expect(self, name: str, prefix: str) -> str:
        """Block until `name` reports a message starting with `prefix`."""
        _, stream = self._peer(name)
        got = stream.readline().strip()
        assert got.startswith(prefix), f"{name} said {got!r}, expected {prefix!r}"
        return got

    def silent(self, name: str, seconds: float) -> bool:
        """True if `name` says nothing at all within `seconds`.

        For stating that a child is *blocked*, which is the only way to show a
        lock is doing its job: an assertion about something not happening needs
        a bound, and this is the bound.

        `select` on the socket rather than a short read - a timed-out read
        leaves the buffered file object permanently unusable ("cannot read from
        timed out object"), so the very next `expect` on that child would fail
        for a reason that has nothing to do with the code under test.
        """
        conn, _ = self._peer(name)
        return not select.select([conn], [], [], seconds)[0]

    def release(self, name: str, payload: str = "go") -> None:
        _, stream = self._peer(name)
        stream.write(payload + "\n")
        stream.flush()

    def close(self) -> None:
        for conn, stream in self._peers.values():
            stream.close()
            conn.close()
        self._srv.close()


# Prepended to every child script: the same `say`/`wait` pair on both sides.
PREAMBLE = """
import json, pathlib, socket, sys
sys.path.insert(0, {engine!r})
ROOT, WHO, PORT = sys.argv[1], sys.argv[2], int(sys.argv[3])
_sock = socket.create_connection(("127.0.0.1", PORT))
_sock.settimeout({timeout})
_io = _sock.makefile("rw", encoding="utf-8")

def say(message):
    _io.write(message + "\\n")
    _io.flush()

def wait():
    line = _io.readline()
    if not line:
        raise SystemExit("the parent closed the rendezvous")
    return line.strip()

say(WHO)
"""


def start(body: str, root, who: str, port: int):
    """Run `body` in a real child process wired to the barrier."""
    script = (PREAMBLE.format(engine=ENGINE, timeout=RENDEZVOUS_TIMEOUT)
              + textwrap.dedent(body))
    return subprocess.Popen(
        [sys.executable, "-c", script, str(root), who, str(port)],
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)


def finish(procs: dict) -> dict:
    out = {}
    for who, pr in procs.items():
        stdout, stderr = pr.communicate(timeout=RENDEZVOUS_TIMEOUT)
        out[who] = {"code": pr.returncode, "out": stdout.strip(), "err": stderr.strip()}
    return out


def run_all(bodies: dict, root, drive) -> dict:
    """Start every child, run `drive(bar)`, then collect the results.

    `drive` receives the barrier and states the interleaving. Children are
    killed rather than waited on if the drive step failed, so a deadlock in the
    code under test surfaces as a failing test and not as a hung suite.
    """
    bar = Rendezvous()
    procs = {}
    try:
        for who, body in bodies.items():
            procs[who] = start(body, root, who, bar.port)
        drive(bar)
        results = finish(procs)
    finally:
        bar.close()
        for pr in procs.values():
            if pr.poll() is None:
                pr.kill()
    assert all(r["code"] == 0 for r in results.values()), results
    return results
