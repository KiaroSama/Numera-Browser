"""What the test process may see and may reach.

Two guards, both about the same thing: this suite drives a *browser identity
manager*. It opens real browsers, starts real servers, and handles proxy
credentials - so a test that reaches past the machine does not merely fail
oddly, it can send a profile's identity somewhere real or operate on the user's
own profiles. A sibling project paid for exactly that: a test meaning to check
a CLI usage error reached the live API and mutated production data.

**The environment is scrubbed.** `NUMERA_DATA_DIR` pointing at the user's real
data root would let a test create, rename and delete their actual profiles;
`NUMERA_API_TOKEN` would make an auth test pass for a reason it never asserted;
`HTTP_PROXY` and friends would route the engine's own requests through whatever
the developer happens to be using. Every test that needs one of these sets it
itself, so removing them costs nothing and closes all of it.

`PLAYWRIGHT_BROWSERS_PATH` stays: the live suites need to find the browsers.

**Outbound sockets are refused unless they are loopback.** The relay tests, the
API tests and every local server bind and connect to 127.0.0.1, which is what
this suite is for; anything further is a mistake. A hostname is refused without
resolving it, because the lookup is itself a network call.

**What this cannot cover, stated plainly:** the browser runs in its own
process, so nothing here constrains where *it* connects. That is the launch
arguments' job, not this file's, and a test that navigates a real browser to a
real site will still do so.
"""

import os
import socket

import pytest

#: Removed before any test module is imported. Anything Numera reads from the
#: environment can change what a test operates on or reaches; a test that wants
#: one sets it itself.
SCRUBBED = (
    "NUMERA_DATA_DIR", "NUMERA_API_TOKEN", "NUMERA_PASSWORD", "NUMERA_JOB_FILE",
    "NUMERA_PARENT_PID", "NUMERA_ALLOW_INSECURE_BIND", "NUMERA_ALLOWED_ORIGINS",
    "NUMERA_LOCK_TIMEOUT",
    "HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "NO_PROXY",
    "http_proxy", "https_proxy", "all_proxy", "no_proxy",
)

for _name in SCRUBBED:
    os.environ.pop(_name, None)


class NetworkAccessDenied(RuntimeError):
    """A test tried to reach something that is not on this machine."""


def _this_machine() -> frozenset:
    """Every address that is still this machine.

    Loopback, plus the host's own interface addresses. The second half is not
    generosity: `test_the_relay_is_not_reachable_off_the_machine` dials this
    machine's LAN address *on purpose*, to prove the relay refused to bind
    there. Denying it did not make the suite safer - it replaced a real
    assertion about the relay with an exception from this file, which is the
    exact way a guard turns a regression test into a decoration.

    `0.0.0.0` is here because a server bound to it is reached at 127.0.0.1 and
    some clients hand the bind address straight back. The lookup is local and
    happens once, before the guard below is installed.
    """
    here = {"127.0.0.1", "::1", "localhost", "0.0.0.0", "::"}
    try:
        here.update(socket.gethostbyname_ex(socket.gethostname())[2])
    except OSError:                                          # pragma: no cover
        pass                        # no name resolution here; loopback only
    return frozenset(here)


#: Everything a test is allowed to connect to.
LOOPBACK = _this_machine()

_real_connect = socket.socket.connect
_real_connect_ex = socket.socket.connect_ex


def _permitted(address) -> bool:
    """Is this an address on this machine?

    A tuple whose host is a loopback literal passes. A hostname does not, even
    one that would resolve to loopback: resolving it is itself a network call,
    and a test with a hostname in it is a test that meant to go outside.
    Anything that is not an IP socket - a Unix socket, a pipe - is local by
    construction and passes.
    """
    if not isinstance(address, tuple) or not address:
        return True
    host = address[0]
    if isinstance(host, bytes):
        host = host.decode("utf-8", "replace")
    if not isinstance(host, str):
        return False
    return host.strip("[]") in LOOPBACK


def _refuse(address):
    return NetworkAccessDenied(
        f"this test tried to connect to {address!r}. The suite is confined to "
        f"this machine: it handles proxy credentials and browser identities, "
        f"so reaching a real host from a test is a defect even when it works. "
        f"Point it at a local server, or fake the client.")


def _guarded_connect(self, address):
    if not _permitted(address):
        raise _refuse(address)
    return _real_connect(self, address)


def _guarded_connect_ex(self, address):
    if not _permitted(address):
        raise _refuse(address)
    return _real_connect_ex(self, address)


socket.socket.connect = _guarded_connect
socket.socket.connect_ex = _guarded_connect_ex


@pytest.fixture
def allow_outbound(monkeypatch):
    """Lift the network guard for one test that genuinely needs the internet.

    Nothing uses it today. It exists so that adding such a test is a visible,
    named decision in the test's own signature rather than a quiet edit to the
    guard above.
    """
    monkeypatch.setattr(socket.socket, "connect", _real_connect)
    monkeypatch.setattr(socket.socket, "connect_ex", _real_connect_ex)
    return None
