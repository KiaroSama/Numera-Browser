"""Proxy-pool parsing and the engine<->dashboard profile round-trip for import.

The health check needs the network, so what's pinned here is the parser (which
has to read every layout a vendor might hand you) and the reverse mapping that
turns an imported engine profile back into the dashboard shape.
"""

import pytest

from numera.proxypool import parse_line, parse_many, to_line
from numera.server import ProxyPool, to_dashboard_profile, to_engine_profile
from numera import generate
from numera.models import Profile, Proxy


def test_parse_host_port():
    assert parse_line("1.2.3.4:8080") == {
        "type": "HTTP", "host": "1.2.3.4", "port": "8080",
        "user": None, "pass": None, "country": None, "label": None}


def test_parse_keeps_the_label_beside_the_country_code():
    """'IT 1'..'IT 4' are four distinct proxies, not four copies of 'IT'.

    The country code aligns the fingerprint; the suffix is the only thing that
    tells same-country entries apart in the picker, so it must survive import."""
    one = parse_line("gw.example:3128:user:pass:IT 1")
    two = parse_line("gw.example:3128:user:pass:IT 2")
    assert (one["country"], one["label"]) == ("IT", "IT 1")
    assert (two["country"], two["label"]) == ("IT", "IT 2")
    assert one["label"] != two["label"]
    # A bare code is still just a country, with no label to show.
    assert parse_line("gw.example:3128:IT")["label"] is None


def test_parse_host_port_user_pass():
    p = parse_line("1.2.3.4:8080:alice:secret")
    assert (p["host"], p["port"], p["user"], p["pass"]) == ("1.2.3.4", "8080", "alice", "secret")


def test_parse_user_pass_at_host():
    p = parse_line("alice:secret@1.2.3.4:8080")
    assert (p["user"], p["pass"], p["host"], p["port"]) == ("alice", "secret", "1.2.3.4", "8080")


def test_parse_scheme_and_socks():
    assert parse_line("socks5://host.io:1080")["type"] == "SOCKS5"
    assert parse_line("https://u:p@host.io:3128")["type"] == "HTTPS"


def test_parse_trailing_country_code():
    assert parse_line("1.2.3.4:8080:u:p:US")["country"] == "US"
    assert parse_line("1.2.3.4:8080:DE")["country"] == "DE"


def test_parse_rejects_junk():
    assert parse_line("not a proxy") is None
    assert parse_line("# comment") is None
    assert parse_line("") is None
    assert parse_line("host-without-port") is None


def test_parse_many_skips_bad_lines():
    text = "1.2.3.4:8080\n# a comment\n\ngarbage\nuser:pass@5.6.7.8:3128\n"
    out = parse_many(text)
    assert len(out) == 2
    assert {p["host"] for p in out} == {"1.2.3.4", "5.6.7.8"}


def test_dashboard_engine_round_trip_preserves_identity():
    prof = Profile(name="rt", engine="cloak",
                   fingerprint=generate(seed=5, os="macos", locale="de-DE"),
                   proxy=Proxy(server="socks5://9.9.9.9:1080", username="x",
                               password="y", country="DE"), tags=["t"])
    d = to_dashboard_profile(prof)
    assert d["os"] == "macOS" and d["engine"] == "cloak"
    assert d["proxy"]["type"] == "SOCKS5" and d["proxy"]["host"] == "9.9.9.9"
    assert d["proxy"]["country"] == "DE"

    back = to_engine_profile(d)
    assert back.fingerprint.os == "macos"
    assert back.fingerprint.webgl_renderer == prof.fingerprint.webgl_renderer
    assert back.proxy.server.startswith("socks5://9.9.9.9")


def test_import_shaped_profile_is_coherent():
    # A profile produced by the importer must survive to_dashboard_profile ->
    # to_engine_profile and still validate.
    from numera.importers import profile_from_export
    from numera.fingerprint import validate
    prof, _ = profile_from_export({"name": "gl", "os": "mac",
                                   "navigator": {"userAgent": "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) Chrome/128.0.0.0 Safari/537.36"}})
    d = to_dashboard_profile(prof)
    assert validate(to_engine_profile(d).fingerprint) == []


def test_sticky_session_proxies_are_not_collapsed(tmp_path):
    # Sticky-session vendors serve one host:port for the whole pool and pick the
    # exit node from the credentials - some vary the username, some only the
    # password. Every line is a distinct proxy, so none may be deduped away.
    pool = ProxyPool(tmp_path)
    lines = ",".join(
        f"socks5://gate.example.com:32325:acct-1:token-{cc}-{n}"
        for cc in ("IT", "BE", "UK", "TR") for n in range(1, 5)
    )
    added = pool.add_many(parse_many(lines))
    assert len(added) == 16
    assert len(pool.list()) == 16

    # Re-importing the same list is still a no-op.
    assert pool.add_many(parse_many(lines)) == []
    assert len(pool.list()) == 16


# --------------------------------------------------------------------------
# Export
# --------------------------------------------------------------------------
@pytest.mark.parametrize("line", [
    "1.2.3.4:8080",
    "http://1.2.3.4:8080",
    "socks5://1.2.3.4:1080",
    "user:pass@1.2.3.4:8080",
    "socks5://user:pass@1.2.3.4:1080",
    "1.2.3.4:8080:user:pass",
    "1.2.3.4:8080:US",
    "socks5://user:pass@1.2.3.4:1080:IT 1",
    "1.2.3.4:8080:user:pass:DE",
])
def test_export_re_imports_to_the_same_proxy(line):
    """`to_line` is the parser's inverse, and export is only useful if it is.

    A pool leaves this machine through export - into a teammate's Numera, a
    vendor's dashboard, a backup. If a round trip loses the credentials or the
    country, the file looks fine and the restored pool is wrong.
    """
    first = parse_line(line)
    assert first, f"the fixture itself does not parse: {line}"
    again = parse_line(to_line(first))
    assert again, f"exported line did not parse back: {to_line(first)}"
    for key in ("type", "host", "port", "user", "pass", "country"):
        assert again[key] == first[key], (
            f"{key} changed on the round trip: {first[key]!r} -> {again[key]!r} "
            f"via {to_line(first)!r}")


def test_export_keeps_a_password_containing_a_colon():
    """The colon-separated form is ambiguous for such a password; the URL form is not."""
    p = {"type": "SOCKS5", "host": "1.2.3.4", "port": "1080",
         "user": "u", "pass": "a:b:c", "country": None, "label": None}
    back = parse_line(to_line(p))
    assert back["pass"] == "a:b:c", to_line(p)


def test_export_does_not_repeat_the_country_when_a_label_carries_it():
    """`label` already begins with the code - emitting both would grow each round trip."""
    p = parse_line("socks5://1.2.3.4:1080:IT 1")
    line = to_line(p)
    assert line.count("IT") == 1, line
    assert parse_line(line)["label"] == "IT 1"
