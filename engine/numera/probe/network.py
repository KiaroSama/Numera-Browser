"""Where the traffic actually comes out, and what it gives away on the way.

Proxy exit, WebRTC leaks and the TLS/HTTP2 handshake. All three need internet:
they are questions only a real request can answer. `check_proxy` is the
orchestrator - it opens one browser context and runs the others inside it, so a
profile is exercised once rather than four times.
"""

from __future__ import annotations

import time
from typing import Optional

from ..devices import same_timezone as _same_timezone
from ..drivers import session
from ..models import Profile
from ..store import ProfileStore
from .dns import check_dns
from .shared import _check

# go, and a proxy test that fails because one host is down is worse than useless.
IP_ECHO = [
    "https://ipapi.co/json/",
    "https://ipwho.is/",
    "https://api.ipify.org/?format=json",
]

# Services that reflect the caller's TLS/HTTP2 fingerprint back as JSON. Read
# through the browser, so it's the browser's real handshake (and proxy) we see.
# Formats differ, so the parser is defensive; order is by how much they return.
TLS_ECHO = [
    "https://tls.peet.ws/api/all",          # ja3 + ja4 + http2, richest
    "https://check.ja3.zone/",              # {hash, fingerprint}
    "https://tools.scrapfly.io/api/fp/ja3",  # {ja3, ...}
]

# A blank page served from memory. Fingerprinting APIs want a real secure

# --------------------------------------------------------------------------
# Proxy + leaks
# --------------------------------------------------------------------------
_IP_JS = """
async (urls) => {
  for (const url of urls) {
    try {
      const res = await fetch(url, { cache: 'no-store' });
      if (!res.ok) continue;
      const text = (await res.text()).trim();
      let data;
      try { data = JSON.parse(text); } catch { data = { ip: text }; }
      const ip = data.ip || data.query || data.IPv4;
      if (!ip) continue;
      return {
        source: url,
        ip,
        country: data.country_code || data.country || data.countryCode || null,
        city: data.city || null,
        timezone: data.timezone?.id || data.timezone || null,
      };
    } catch (e) { /* try the next one */ }
  }
  return null;
}
"""

# Ask the browser for its ICE candidates. If a proxy is in play, any public
# address here is an address the proxy failed to hide.
_WEBRTC_JS = """
() => new Promise((resolve) => {
  const found = new Set();
  let pc;
  try { pc = new RTCPeerConnection({ iceServers: [{ urls: 'stun:stun.l.google.com:19302' }] }); }
  catch (e) { return resolve({ supported: false, ips: [] }); }
  const done = () => { try { pc.close(); } catch (e) {} resolve({ supported: true, ips: [...found] }); };
  pc.onicecandidate = (e) => {
    if (!e.candidate) return done();
    const m = /([0-9]{1,3}(\\.[0-9]{1,3}){3}|[a-f0-9]{1,4}(:[a-f0-9]{0,4}){2,7})/i
      .exec(e.candidate.candidate || '');
    if (m && !m[0].endsWith('.local')) found.add(m[0]);
  };
  try { pc.createDataChannel('probe'); pc.createOffer().then((o) => pc.setLocalDescription(o)); }
  catch (e) { return done(); }
  setTimeout(done, 3000);
})
"""


def _is_private(ip: str) -> bool:
    """Private/link-local addresses are normal to see; public ones are the leak."""
    if ":" in ip:                       # IPv6: fc00::/7 (unique local), fe80::/10
        low = ip.lower()
        return low.startswith(("fc", "fd", "fe8", "fe9", "fea", "feb", "::1"))
    parts = ip.split(".")
    if len(parts) != 4 or not all(p.isdigit() for p in parts):
        return True
    a, b = int(parts[0]), int(parts[1])
    return (a == 10 or a == 127 or (a == 172 and 16 <= b <= 31)
            or (a == 192 and b == 168) or (a == 169 and b == 254))


def check_proxy(profile: Profile, store: ProfileStore,
                engine: Optional[str] = None) -> dict:
    """Route a real request through the profile and report where it came out.

    Works with or without a proxy: without one it simply tells you your own exit
    IP, which is the baseline you compare a proxy against.
    """
    started = time.time()
    with session(profile, store, engine) as context:
        page = context.pages[0] if context.pages else context.new_page()
        try:
            info = page.evaluate(_IP_JS, IP_ECHO)
        except Exception as e:
            return {"ok": False, "error": str(e).splitlines()[0],
                    "proxy": profile.proxy.server if profile.proxy else None,
                    "checks": []}
        leaks = check_leaks(profile, store, context=context, exit_ip=(info or {}).get("ip"))
        dns = check_dns(profile, store, context=context, exit_ip=(info or {}).get("ip"))
        tls = check_tls(profile, store, context=context, engine=engine)

    latency = int((time.time() - started) * 1000)
    if not info:
        return {"ok": False, "error": "no IP service answered — is the proxy reachable?",
                "proxy": profile.proxy.server if profile.proxy else None,
                "latencyMs": latency,
                "checks": leaks["checks"] + dns["checks"] + tls["checks"]}

    fp = profile.fingerprint
    checks = [_check("Traffic reaches the internet", True,
                     f"exit IP {info['ip']}" + (f" · {info['city']}" if info.get("city") else ""))]

    want_country = (profile.proxy.country or "").upper() if profile.proxy else ""
    got_country = (info.get("country") or "").upper()
    if want_country:
        checks.append(_check(
            "Exit country matches the proxy setting",
            got_country == want_country,
            f"expected {want_country}, got {got_country or 'unknown'}"))
    elif got_country:
        checks.append(_check("Exit country", True, got_country))

    if info.get("timezone"):
        checks.append(_check(
            "Timezone matches the exit location",
            _same_timezone(info["timezone"], fp.timezone),
            f"profile says {fp.timezone}, the IP looks like {info['timezone']}"))

    checks += leaks["checks"]
    checks += dns["checks"]
    checks += tls["checks"]
    return {
        "ok": all(c["ok"] for c in checks),
        "proxy": profile.proxy.server if profile.proxy else None,
        "ip": info["ip"],
        "country": got_country or None,
        "city": info.get("city"),
        "timezone": info.get("timezone"),
        "latencyMs": latency,
        "dnsCountries": dns.get("countries"),
        "ja3Hash": tls.get("ja3Hash"),
        "ja4": tls.get("ja4"),
        "checks": checks,
    }


# The echo services don't send CORS headers, so a fetch() from page script is
# blocked. Navigating the page straight to the endpoint sidesteps that — it's a
# top-level request, and the handshake is still the browser's real one — then we
# read the JSON out of the rendered body.
_TLS_PARSE = """
() => {
  const raw = document.body ? document.body.innerText : '';
  let d;
  try { d = JSON.parse(raw); } catch (e) { return null; }
  const tls = d.tls || d;
  const ja3 = tls.ja3 || d.ja3 || d.fingerprint || null;
  const ja3Hash = tls.ja3_hash || d.ja3_hash || d.hash || d.ja3_digest || null;
  const ja4 = tls.ja4 || d.ja4 || (d.ja4_r ? String(d.ja4_r).split('_')[0] : null);

  // Cipher count from the ja3 string's 2nd field (GREASE is stripped there per
  // spec, so this is the "real" suite count).
  let cipherCount = 0;
  if (ja3 && ja3.includes(',')) {
    cipherCount = (ja3.split(',')[1] || '').split('-').filter(Boolean).length;
  } else if (tls.ciphers) {
    cipherCount = tls.ciphers.length;
  }

  // GREASE (RFC 8701) is stripped from ja3/ja4, so look at the raw response:
  // services that list ciphers name it ("TLS_GREASE…") or show a 0x?a?a value.
  // greaseKnown tells us whether the service exposed ciphers at all — if it
  // only gave a ja3 string, absence of GREASE means nothing.
  const greaseKnown = /cipher/i.test(raw) || /grease/i.test(raw);
  const grease = /grease/i.test(raw) || /0x[0-9a-f]a[0-9a-f]a/i.test(raw);

  return {
    ja3, ja3Hash, ja4,
    http2: (d.http2 && (d.http2.akamai_fingerprint_hash || d.http2.akamai_fingerprint)) || null,
    grease, greaseKnown, cipherCount,
  };
}
"""


def _read_tls(page) -> Optional[dict]:
    for url in TLS_ECHO:
        try:
            page.goto(url, wait_until="domcontentloaded", timeout=20000)
            info = page.evaluate(_TLS_PARSE)
            if info and (info.get("ja3Hash") or info.get("ja4")):
                info["source"] = url
                return info
        except Exception:
            continue   # service down or unreachable — try the next
    return None


def check_tls(profile: Profile, store: ProfileStore, context=None,
              engine: Optional[str] = None) -> dict:
    """Read the profile's TLS/HTTP2 handshake back from an echo service.

    This is the layer the whole "JS injection isn't enough" argument is about:
    the TLS ClientHello is sent before the browser has a document, so no page
    script — ours or an anti-detect browser's — can alter it. What shapes it is
    the engine's network stack. Chromium-based engines (playwright, patchright,
    cloak) send Chrome's real handshake because they *are* Chromium; the value
    of checking is to confirm nothing downstream (a proxy that terminates TLS,
    an odd build) has changed it into a tell.
    """
    def run(ctx):
        page = ctx.pages[0] if ctx.pages else ctx.new_page()
        return _read_tls(page)

    info = run(context) if context is not None else None
    if info is None and context is None:
        with session(profile, store, engine) as ctx:
            info = run(ctx)

    if not info:
        return {"ok": False, "error": "no TLS echo service answered",
                "checks": [_check("TLS fingerprint readable", False,
                                  "tls.peet.ws / browserleaks unreachable")]}

    ja4 = info.get("ja4") or ""
    checks = [_check("TLS handshake reaches a real endpoint", True,
                     f"JA3 {info['ja3Hash'] or '?'}" + (f" · JA4 {ja4}" if ja4 else ""))]

    # These are the pre-JS tells a server-side detector reads before any page
    # script runs. A real modern browser negotiates TLS 1.3 and HTTP/2; a
    # scripted TLS client (python, go, a misconfigured imnumerator) usually
    # can't reproduce both. JA4's first block encodes exactly this ("t13…h2").
    if ja4:
        checks.append(_check(
            "Negotiates TLS 1.3 (browser-grade)", ja4.startswith("t13"),
            f"JA4 starts '{ja4[:4]}', not 't13'"))
        checks.append(_check(
            "Speaks HTTP/2 like a browser", "h2" in ja4.split("_")[0],
            "no h2 in the ALPN — browsers negotiate it, most scripts don't"))

    # GREASE is Chrome/Chromium's signature (RFC 8701). Only assert it when the
    # service actually exposed the cipher list — otherwise its absence is just
    # the ja3 spec stripping it, not a real finding.
    if info.get("greaseKnown"):
        checks.append(_check(
            "Sends GREASE like a real Chrome", bool(info.get("grease")),
            "no GREASE values — this isn't a stock Chromium ClientHello"))

    if info.get("cipherCount"):
        checks.append(_check(
            "Cipher suite list is browser-shaped", info["cipherCount"] >= 10,
            f"only {info['cipherCount']} suite(s); real Chrome offers ~15"))

    return {
        "ok": all(c["ok"] for c in checks),
        "ja3": info.get("ja3"),
        "ja3Hash": info.get("ja3Hash"),
        "ja4": info.get("ja4"),
        "http2": info.get("http2"),
        "grease": info.get("grease") if info.get("greaseKnown") else None,
        "checks": checks,
    }


def check_leaks(profile: Profile, store: ProfileStore, context=None,
                exit_ip: Optional[str] = None, engine: Optional[str] = None) -> dict:
    """Look for IPs WebRTC hands out that the proxy was meant to hide."""
    def run(ctx):
        page = ctx.pages[0] if ctx.pages else ctx.new_page()
        try:
            return page.evaluate(_WEBRTC_JS)
        except Exception:
            return {"supported": False, "ips": []}

    result = run(context) if context is not None else None
    if result is None:
        with session(profile, store, engine) as ctx:
            result = run(ctx)

    public = [ip for ip in result.get("ips", []) if not _is_private(ip)]
    exposed = [ip for ip in public if not exit_ip or ip != exit_ip]
    detail = ", ".join(exposed) if exposed else "no public address exposed"
    return {
        "webrtcIps": result.get("ips", []),
        "exposed": exposed,
        "checks": [_check("WebRTC does not leak your real IP", not exposed, detail)],
    }
