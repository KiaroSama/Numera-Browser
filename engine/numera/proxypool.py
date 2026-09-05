"""
Proxy pool: parse the proxy lists people actually paste, and health-check them.

Proxy vendors hand you a text file, and every one uses a different order:
``host:port``, ``host:port:user:pass``, ``user:pass@host:port``,
``scheme://user:pass@host:port`` — sometimes with a trailing country code. This
module reads all of those tolerantly into one shape, and does a lightweight
liveness check (a real request through the proxy) without needing a browser, so
a whole pool can be tested quickly.

The shape everywhere is a plain dict::

    {"type": "HTTP"|"HTTPS"|"SOCKS5"|"SOCKS4", "host", "port",
     "user": str|None, "pass": str|None, "country": str|None}
"""

from __future__ import annotations

import json
import re
import time
import http.client
import urllib.error
import urllib.request
from typing import Optional

_SCHEME_TYPE = {"http": "HTTP", "https": "HTTPS", "socks5": "SOCKS5",
                "socks5h": "SOCKS5", "socks4": "SOCKS4", "socks": "SOCKS5"}
_CC = re.compile(r"^[A-Za-z]{2}$")
# A country code with anything after it -- "IT 1", "IT-2", "US_east". The code
# aligns the fingerprint; the rest is what tells four proxies in the same
# country apart, so it is kept as the label instead of being thrown away.
_CC_LABEL = re.compile(r"^([A-Za-z]{2})[\s._-]+(\S.*)$")

# A tiny IP-echo used for the liveness check; tried in order.
_ECHO = ["https://ipapi.co/json/", "https://ipwho.is/", "https://api.ipify.org/?format=json"]


def parse_line(line: str) -> Optional[dict]:
    """Parse one proxy line into the pool shape, or ``None`` if it isn't one."""
    s = (line or "").strip()
    if not s or s.startswith("#"):
        return None

    scheme = "http"
    if "://" in s:
        scheme, s = s.split("://", 1)
        scheme = scheme.lower()

    user = pw = None
    if "@" in s:                              # user:pass@host:port
        creds, s = s.rsplit("@", 1)
        if ":" in creds:
            user, pw = creds.split(":", 1)
        else:
            user = creds or None

    # An IPv6 literal is full of colons, so it has to come off the front before
    # anything is split on one. `[2001:db8::1]:1080` is the only unambiguous way
    # to write one with a port, and it is what every tool emits.
    v6 = None
    if s.startswith("["):
        end = s.find("]")
        if end < 0:
            return None
        v6, s = s[1:end], s[end + 1:]
        if s.startswith(":"):
            s = s[1:]
        parts = [v6] + s.split(":")
    else:
        parts = s.split(":")
    if len(parts) < 2 or not parts[1].isdigit():
        return None
    host, port = parts[0].strip(), parts[1].strip()
    if not host:
        return None

    country = label = None

    def _country_and_label(tok: str):
        """A bare 'IT' is just a country; 'IT 1' is a country plus a label."""
        tok = (tok or "").strip()
        if _CC.match(tok):
            return tok.upper(), None
        m = _CC_LABEL.match(tok)
        if m:
            return m.group(1).upper(), tok
        return None, None

    # host:port:user:pass[:CC]  (creds after the port, colon-separated)
    if user is None and len(parts) >= 4:
        user, pw = parts[2] or None, parts[3] or None
        if len(parts) >= 5:
            country, label = _country_and_label(parts[4])
    # host:port:CC  or  user:pass@host:port:CC
    elif len(parts) >= 3:
        country, label = _country_and_label(parts[-1])

    return {
        # Keep what was actually written. Defaulting an unrecognised scheme to
        # HTTP meant `normalize()` downstream saw a valid-looking entry and
        # could never reject the input - the user got an HTTP proxy they never
        # asked for. A line with no scheme at all is still HTTP by convention,
        # which `_SCHEME_TYPE` covers via the "http" default set in the parser.
        "type": _SCHEME_TYPE.get(scheme, scheme.upper()),
        "host": host, "port": port,
        "user": user or None, "pass": pw or None,
        "country": country, "label": label,
    }


_TYPE_SCHEME = {"HTTP": "http", "HTTPS": "https", "SOCKS5": "socks5", "SOCKS4": "socks4"}


def to_line(proxy: dict) -> str:
    """Render one pooled proxy back to a line `parse_line` can read again.

    Export exists so a pool can leave this machine - into a teammate's Numera, a
    vendor's dashboard, a backup. That is only true if the output parses back to
    the same proxy, so this is written as the exact inverse of the parser and a
    round-trip test holds the two together.

    The scheme://user:pass@host:port form is chosen over the colon-separated one
    because it is unambiguous when a password itself contains a colon.
    """
    scheme = _TYPE_SCHEME.get(str(proxy.get("type", "HTTP")).upper(), "http")
    auth = ""
    if proxy.get("user"):
        auth = f"{proxy['user']}:{proxy.get('pass') or ''}@"
    # Re-bracket an IPv6 host, or the port reads as another group of the address
    # and the export no longer parses back - which is the one thing this function
    # exists to guarantee.
    host = str(proxy["host"])
    if ":" in host and not host.startswith("["):
        host = f"[{host}]"
    line = f"{scheme}://{auth}{host}:{proxy['port']}"
    # The label already starts with the country code when there is one (that is
    # how the parser built it), so appending both would repeat it.
    tail = proxy.get("label") or proxy.get("country")
    return f"{line}:{tail}" if tail else line


def validate_rotate_url(url):
    """A syntactically usable change-IP URL, or a ValueError naming the problem.

    Shape only - scheme, host, port. Whether the address is public and where it
    actually resolves is decided at *use* time by `_public_destination()`,
    because DNS answers change and a write must not depend on the network.

    The port matters more than it looks: `rotate()` reads `urlsplit(url).port`,
    and `urlsplit` raises `ValueError` for an out-of-range port when the
    attribute is read - outside `rotate()`'s network `try`, so a stored bad
    endpoint surfaced as a 500 rather than a refusal.
    """
    from urllib.parse import urlsplit

    if url in (None, ""):
        return None
    text = str(url).strip()
    if not text:
        return None
    parts = urlsplit(text)
    if parts.scheme.lower() not in ("http", "https"):
        raise ValueError(f"the change-IP URL must be http(s), got {parts.scheme or 'nothing'!r}")
    if not parts.hostname:
        raise ValueError("the change-IP URL has no host")
    try:
        parts.port                                  # raises on an invalid port
    except ValueError:
        raise ValueError(f"{text!r} has an invalid port") from None
    return text


def normalize(entry) -> dict:
    """One proxy in the canonical shape, or a ValueError saying what is wrong.

    Text input went through `parse_line`, which validates as it parses.
    Structured input from `POST /api/proxies` went straight to the pool, whose
    identity code indexes `host` and `port` - so `{"nope": 1}` raised a KeyError
    as a 500, a non-numeric port was stored and only failed at launch, and an
    unsupported type was accepted and silently treated as HTTP. Both paths come
    through here now, so a malformed proxy is a 4xx naming the field rather than
    a stack trace or a broken record.
    """
    if not isinstance(entry, dict):
        raise ValueError(f"a proxy must be an object, got {type(entry).__name__}")

    kind = str(entry.get("type") or "HTTP").upper()
    if kind not in _TYPE_SCHEME:
        raise ValueError(f"unsupported proxy type {kind!r}; "
                         f"expected one of {', '.join(sorted(_TYPE_SCHEME))}")

    host = str(entry.get("host") or "").strip().strip("[]")
    if not host:
        raise ValueError("a proxy needs a host")
    if any(c.isspace() for c in host):
        raise ValueError(f"{host!r} is not a valid host")

    raw_port = entry.get("port")
    try:
        port = int(str(raw_port).strip())
    except (TypeError, ValueError):
        raise ValueError(f"{raw_port!r} is not a valid port") from None
    if not 1 <= port <= 65535:
        raise ValueError(f"port {port} is outside 1-65535")

    def text(key):
        value = entry.get(key)
        return str(value) if value not in (None, "") else None

    country = (text("country") or "").strip().upper() or None
    if country and not _CC.match(country):
        raise ValueError(f"{country!r} is not a two-letter country code")

    return {"type": kind, "host": host, "port": str(port),
            "user": text("user"), "pass": text("pass"),
            "country": country, "label": text("label"),
            # Part of the canonical shape, not an extra. `normalize()` returned
            # only the core fields, so `POST /api/proxies` silently dropped a
            # rotation endpoint the caller supplied - while `PUT` kept it,
            # because edit merges onto the stored record instead.
            "rotateUrl": validate_rotate_url(text("rotateUrl"))}


def normalize_many(entries) -> list[dict]:
    """Normalize a list, reporting every bad entry rather than only the first."""
    if not isinstance(entries, list):
        raise ValueError(f"'proxies' must be a list, got {type(entries).__name__}")
    out, problems = [], []
    for i, entry in enumerate(entries):
        try:
            out.append(normalize(entry))
        except ValueError as exc:
            problems.append(f"proxies[{i}]: {exc}")
    if problems:
        raise ValueError("; ".join(problems[:8]))
    return out


def parse_many(text: str) -> list[dict]:
    """Parse a whole pasted list / file, skipping blank and unparseable lines."""
    out = []
    for line in (text or "").replace(",", "\n").splitlines():
        p = parse_line(line)
        if p:
            out.append(p)
    return out


# Kept for callers that still import it, but no longer a warning worth showing:
# a credentialed SOCKS proxy now works, because the launcher routes it through a
# local relay that terminates the authentication (see relay.py). Chromium's own
# refusal ("Browser does not support socks5 proxy authentication") is real, and
# the relay is what steps around it.
SOCKS_AUTH_NOTE = (
    "Chromium cannot authenticate a SOCKS proxy itself. Numera launches this "
    "profile through a local relay that supplies the credentials, so the proxy "
    "works - no change needed."
)


def proxy_warnings(profile) -> list:
    """Problems with a profile's proxy that the browser cannot work around.

    Credentialed SOCKS used to be listed here. It is not a problem any more -
    the launcher puts a local relay in front of it - so warning about it would
    now be the opposite mistake: telling the user something is broken when it
    works."""
    return []


def _proxy_url(proxy: dict) -> str:
    auth = ""
    if proxy.get("user"):
        auth = f"{proxy['user']}:{proxy.get('pass') or ''}@"
    host = str(proxy["host"])
    if ":" in host and not host.startswith("["):
        host = f"[{host}]"
    return f"http://{auth}{host}:{proxy['port']}"


def _ask_echo(opener, timeout: float, started: float) -> Optional[dict]:
    """Ask each IP echo service in turn until one answers with an address."""
    last = None
    for url in _ECHO:
        try:
            with opener.open(url, timeout=timeout) as r:
                text = r.read().decode("utf-8", "replace").strip()
            try:
                data = json.loads(text)
            except json.JSONDecodeError:
                data = {"ip": text}
            ip = data.get("ip") or data.get("query") or data.get("IPv4")
            if not ip:
                continue
            return {
                "ok": True, "ip": ip,
                "country": data.get("country_code") or data.get("country") or data.get("countryCode"),
                "city": data.get("city"),
                "latencyMs": int((time.time() - started) * 1000),
            }
        except Exception as e:  # noqa: BLE001 - report, try the next echo
            last = str(e).splitlines()[0][:140]
    return {"ok": False, "error": last or "no IP service answered"}


def check(proxy: dict, timeout: float = 12) -> dict:
    """Route a real request through the proxy and report the exit IP/country.

    urllib has no SOCKS transport, so a SOCKS proxy used to come back as
    "test it via a profile's proxy test" - which for a pool that is mostly SOCKS
    means the Test button appears to do nothing. It is not a missing capability:
    `relay.py` already speaks SOCKS5 (that is how credentialed SOCKS works in the
    browser at all), so the check borrows it. The relay is an HTTP proxy on
    loopback, which urllib does understand.
    """
    started = time.time()
    is_socks = str(proxy.get("type", "HTTP")).upper().startswith("SOCKS")

    if is_socks:
        from .models import Proxy
        from .relay import ProxyRelay, ProxyRelayError

        # SOCKS4 used to come back `ok=None` with "the protocol has no
        # authentication to test", which was the wrong reason for the wrong
        # thing: a health check measures whether the proxy connects and what IP
        # it exits from, and neither needs authentication. The real gap was that
        # the relay only spoke SOCKS5. It speaks SOCKS4a now, so the Test button
        # on a SOCKS4 row does what it says.
        scheme = "socks4" if str(proxy.get("type", "")).upper() == "SOCKS4" else "socks5"
        upstream = Proxy(server=f"{scheme}://{format_host(proxy['host'])}:{proxy['port']}",
                         username=proxy.get("user") or None,
                         password=proxy.get("pass") or None)
        try:
            with ProxyRelay(upstream).start() as relay:
                handler = urllib.request.ProxyHandler({"http": relay.url, "https": relay.url})
                opener = urllib.request.build_opener(handler)
                opener.addheaders = [("User-Agent", "numera-proxy-check/1.0")]
                res = _ask_echo(opener, timeout, started)
                # "Tunnel connection failed: 502 Bad Gateway" is what urllib
                # reports for every upstream problem alike. The relay knows
                # which one it was; prefer its account.
                if not res.get("ok") and relay.last_error:
                    res["error"] = relay.last_error
                return res
        except ProxyRelayError as e:
            return {"ok": False, "error": str(e).splitlines()[0][:180]}
        except Exception as e:  # noqa: BLE001 - a dead upstream is a failed check
            return {"ok": False, "error": str(e).splitlines()[0][:180]}

    handler = urllib.request.ProxyHandler({"http": _proxy_url(proxy), "https": _proxy_url(proxy)})
    opener = urllib.request.build_opener(handler)
    opener.addheaders = [("User-Agent", "numera-proxy-check/1.0")]
    return _ask_echo(opener, timeout, started)


def exit_country(proxy, timeout: float = 10) -> Optional[str]:
    """ISO country of a ``Proxy`` model's exit IP, or None if it can't be read.

    The pool stores proxies as flat dicts and `check` speaks that shape; a
    launched profile carries a `Proxy` model. Rather than a second echo client,
    this adapts one to the other.
    """
    from urllib.parse import urlsplit

    server = proxy.server if "://" in proxy.server else "http://" + proxy.server
    u = urlsplit(server)
    res = check({"type": _SCHEME_TYPE.get((u.scheme or "http").lower(), "HTTP"),
                 "host": u.hostname or "", "port": u.port or 0,
                 "user": proxy.username or "", "pass": proxy.password or ""},
                timeout=timeout)
    country = (res or {}).get("country")
    return country.upper()[:2] if isinstance(country, str) and country else None


# A rotating proxy's change-IP endpoint is rate-limited by every provider that
# offers one, and hitting it too soon returns the *same* address - which reads
# as a broken button. Refusing early is honest; pretending is not.
ROTATE_COOLDOWN = 20.0


def _resolve(host: str, port: int) -> list:
    """Every address `host` currently resolves to, as `ipaddress` objects."""
    import ipaddress
    import socket

    out = []
    for info in socket.getaddrinfo(host, port, proto=socket.IPPROTO_TCP):
        try:
            out.append(ipaddress.ip_address(info[4][0]))
        except ValueError:                                      # pragma: no cover
            continue
    return out


def _not_global(ip) -> bool:
    """True when this address is not routable on the public internet.

    An allow rule, not a deny list. The previous check enumerated private,
    loopback, link-local, multicast and reserved - and missed
    `100.64.0.0/10` (carrier-grade NAT), which Python reports as none of those.
    `is_global` is the property that actually answers the question, so anything
    it rejects is rejected here, whatever range it belongs to.
    """
    return not ip.is_global


def _public_destination(url: str) -> "tuple":
    """`(problem, pinned_ip)` for a rotate URL - problem is None when allowed.

    The rotate endpoint is the one place the engine fetches a caller-supplied
    address, deliberately *not* through the proxy, so it runs from the server's
    own network position. Returns the address it validated so the connection can
    be pinned to it: resolving here and letting `urlopen` resolve again leaves a
    window where a hostname answers with a public address for the check and a
    private one for the connection.
    """
    from urllib.parse import urlsplit

    # Fails closed on anything `urlsplit` cannot make sense of. Write-time
    # validation only covers values written since it was added: an endpoint
    # from an older build, a hand-edited proxies.json or a restored backup can
    # still hold `http://example.test:99999/rotate`, and reading `.port` raises
    # ValueError *here* - outside `rotate()`'s network `try`, so it left the
    # request as a 500 with a traceback instead of a message the user can act
    # on. `.hostname` can raise the same way on a malformed authority.
    try:
        parts = urlsplit(url)
        scheme = parts.scheme.lower()
        host = parts.hostname
        port = parts.port or (443 if scheme == "https" else 80)
    except ValueError as e:
        return (f"the change-IP URL is malformed ({e})", None)
    if scheme not in ("http", "https"):
        return (f"the change-IP URL must be http(s), got {parts.scheme!r}", None)
    if not host:
        return ("the change-IP URL has no host", None)
    try:
        addresses = _resolve(host, port)
    except OSError as e:
        return (f"{host} does not resolve ({e})", None)
    if not addresses:
        return (f"{host} does not resolve", None)
    for ip in addresses:
        if _not_global(ip):
            return (f"{host} resolves to {ip}, which is not a public address - "
                    f"a change-IP URL points at your provider, not at this "
                    f"machine or your network", None)
    return (None, addresses[0])


class _PinnedHTTPConnection(http.client.HTTPConnection):
    """Connect to a pre-validated address while keeping the real hostname.

    The Host header and TLS verification still use the name; only the socket
    target is pinned, so a second lookup cannot hand back a different address
    than the one that passed validation.
    """

    pinned_ip = None

    def connect(self):
        import socket

        self.sock = socket.create_connection((str(self.pinned_ip), self.port), self.timeout)
        if getattr(self, "_tunnel_host", None):                 # pragma: no cover
            self._tunnel()


class _PinnedHTTPSConnection(http.client.HTTPSConnection):
    pinned_ip = None

    def connect(self):
        import socket

        sock = socket.create_connection((str(self.pinned_ip), self.port), self.timeout)
        # `server_hostname` keeps SNI and certificate verification on the name,
        # not on the pinned literal - pinning must not weaken TLS.
        self.sock = self._context.wrap_socket(sock, server_hostname=self.host)


def _pinned_opener(ip):
    """An opener whose sockets go to `ip`, revalidating on every redirect."""

    # Each request carries its own pin. These classes used to close over the
    # opener's original `ip`, so the redirect handler below validated hop two's
    # address, stored it on the new request - and the connection still went to
    # hop one's IP. A cross-host redirect was checked against one host and
    # connected to another, which is the whole bypass the pinning exists to stop.
    def _pin_for(req):
        return getattr(req, "pinned_ip", ip)

    class _H(urllib.request.HTTPHandler):
        def http_open(self, req):
            return self.do_open(
                type("C", (_PinnedHTTPConnection,), {"pinned_ip": _pin_for(req)}), req)

    class _HS(urllib.request.HTTPSHandler):
        def https_open(self, req):
            return self.do_open(
                type("C", (_PinnedHTTPSConnection,), {"pinned_ip": _pin_for(req)}), req)

    class _Redirects(urllib.request.HTTPRedirectHandler):
        """Every hop is checked and pinned again.

        `urlopen` follows redirects by default, so validating only the first URL
        would be bypassable with a single 3xx to an internal address.
        """

        def redirect_request(self, req, fp, code, msg, headers, newurl):
            problem, next_ip = _public_destination(newurl)
            if problem:
                raise urllib.error.URLError(f"refused to follow a redirect: {problem}")
            new = super().redirect_request(req, fp, code, msg, headers, newurl)
            if new is not None:
                new.pinned_ip = next_ip
            return new

    # An empty ProxyHandler, explicitly. `build_opener` installs one built from
    # the environment by default, so `HTTP_PROXY`/`HTTPS_PROXY` silently routed
    # this deliberately-direct control-plane request through somebody else's
    # proxy - and the pinned socket then connected to the proxy, not to the
    # address that was validated.
    return urllib.request.build_opener(urllib.request.ProxyHandler({}), _H, _HS, _Redirects)


def rotate(proxy: dict, timeout: float = 20) -> dict:
    """Ask a rotating proxy for a new exit IP, then prove it changed.

    Returns the usual check payload plus `rotated`, which is True only when the
    exit IP actually moved. Fetching the endpoint and reporting success without
    re-checking would be the dead-control pattern this project keeps hitting:
    providers return 200 for a request they then ignore.
    """
    url = (proxy.get("rotateUrl") or "").strip()
    if not url:
        return {"ok": None, "rotated": False,
                "error": "no change-IP URL is set for this proxy"}
    problem, pinned = _public_destination(url)
    if problem:
        return {"ok": None, "rotated": False, "error": problem}

    last = max(float(proxy.get("rotateTriedAt") or 0),
               float(proxy.get("rotatedAt") or 0))
    since = time.time() - last
    if since < ROTATE_COOLDOWN:
        return {"ok": None, "rotated": False,
                "error": f"rotated {int(since)}s ago; most providers rate-limit this "
                         f"and would hand back the same IP. Wait "
                         f"{int(ROTATE_COOLDOWN - since)}s."}

    before = (proxy.get("lastCheck") or {}).get("ip")
    try:
        # Deliberately direct, not through the proxy: the endpoint belongs to the
        # provider's control plane, and routing it through the very proxy being
        # rotated is how you lose the response mid-flight.
        req = urllib.request.Request(url, headers={"User-Agent": "numera-proxy-rotate/1.0"})
        opener = _pinned_opener(pinned)
        with opener.open(req, timeout=timeout) as r:
            body = r.read(500).decode("utf-8", "replace").strip()
    except Exception as e:  # noqa: BLE001 - the provider's endpoint is theirs to fail
        # Reached far enough to spend the provider's quota, even though it failed.
        return {"ok": False, "rotated": False, "attempted": True,
                "error": str(e).splitlines()[0][:180]}

    res = check(proxy, timeout=timeout)
    res["attempted"] = True
    after = res.get("ip")
    res["rotated"] = bool(after and before and after != before) or (
        bool(after) and not before)
    res["previousIp"] = before
    if not res["rotated"] and after and before == after:
        # The request succeeded and the address did not move. Say so plainly
        # rather than reporting the green check underneath it.
        res["error"] = (f"the endpoint answered but the exit IP is still {after} - "
                        f"the provider may rotate on a timer, or the URL may be wrong."
                        + (f" It replied: {body[:120]}" if body else ""))
    return res


def split_server(server: str) -> tuple:
    """`(scheme, host, port)` from an engine `Proxy.server`.

    `partition(":")` was used for this, which cuts an IPv6 literal at its first
    group. Everything that takes a proxy apart goes through here so the pool,
    the dashboard and the engine cannot disagree about what a proxy *is*.
    """
    text = str(server or "").strip()
    scheme = "http"
    if "://" in text:
        scheme, text = text.split("://", 1)
        scheme = scheme.lower()
    host, port = text, ""
    if text.startswith("["):                       # [2001:db8::1]:1080
        end = text.find("]")
        if end >= 0:
            host = text[1:end]
            rest = text[end + 1:]
            port = rest[1:] if rest.startswith(":") else ""
    elif text.count(":") == 1:                     # host:port
        host, _, port = text.partition(":")
    # More than one colon and no brackets: a bare IPv6 with no port.
    return scheme, host, port


def format_host(host: str) -> str:
    """Bracket an IPv6 literal so a port can follow it unambiguously."""
    text = str(host or "")
    return f"[{text}]" if ":" in text and not text.startswith("[") else text


def server_url(kind: str, host: str, port) -> str:
    """The engine-side `Proxy.server` for a pool entry's type/host/port.

    The scheme is preserved exactly. A missing mapping used to fall back to
    `http`, so selecting SOCKS4 silently produced an HTTP proxy - the profile
    said one protocol and the browser spoke another.
    """
    scheme = _TYPE_SCHEME.get(str(kind or "HTTP").upper())
    if scheme is None:
        raise ValueError(f"unsupported proxy type {kind!r}; "
                         f"expected one of {sorted(_TYPE_SCHEME)}")
    tail = f":{port}" if str(port or "").strip() else ""
    return f"{scheme}://{format_host(host)}{tail}"


def identity(entry: dict) -> tuple:
    """What makes two pool entries the same proxy.

    Credentials are part of it. Sticky-session vendors hand out one host:port
    for a whole pool and select the exit node from the username or password, so
    keying on host:port alone made every one of them look like the same proxy -
    and the dashboard's "used by N profiles" counted them as one.
    """
    return (str(entry.get("type", "HTTP")).upper(),
            str(entry.get("host", "")),
            str(entry.get("port", "")),
            entry.get("user") or "",
            entry.get("pass") or "")

