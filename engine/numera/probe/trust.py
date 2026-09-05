"""Would this browser pass an inspection right now?

Fully offline: the page being graded is served from memory, so nothing about
the profile touches the network. `_grade` is pure, which is what lets the
aligner score a would-be fingerprint against an observation it already has.
"""

from __future__ import annotations

from typing import Optional

from ..devices import same_timezone as _same_timezone
from ..drivers import session
from ..models import Profile
from ..store import ProfileStore
from .shared import _check, _serve_probe_page

# --------------------------------------------------------------------------
# Trust report
# --------------------------------------------------------------------------
# Everything a fingerprinting script can read in one pass. Kept as one evaluate
# so the page is measured in a single consistent state.
_TRUST_JS = """
async (probe) => {
  const out = {};
  out.webdriver = navigator.webdriver;
  out.webdriverOwn = Object.prototype.hasOwnProperty.call(navigator, 'webdriver');
  out.hasChrome = !!window.chrome;
  out.plugins = navigator.plugins.length;
  out.languages = [...navigator.languages];
  out.platform = navigator.platform;
  out.userAgent = navigator.userAgent;
  out.cores = navigator.hardwareConcurrency;
  out.memory = navigator.deviceMemory;
  out.screenW = screen.width;
  out.screenH = screen.height;
  out.timezone = Intl.DateTimeFormat().resolvedOptions().timeZone;

  try {
    const gl = document.createElement('canvas').getContext('webgl');
    const ext = gl && gl.getExtension('WEBGL_debug_renderer_info');
    out.webglVendor = ext ? gl.getParameter(37445) : null;
    out.webglRenderer = ext ? gl.getParameter(37446) : null;
  } catch (e) { out.webglVendor = out.webglRenderer = null; }

  // A patched function must still report native code — and so must the
  // machinery doing the masking, or the mask is the tell.
  const native = (fn) => /^function [^(]*\\(\\) \\{ \\[native code\\] \\}$/
    .test(Function.prototype.toString.call(fn));
  out.toStringNative = native(navigator.permissions.query)
    && native(Function.prototype.toString)
    && native(Object.getOwnPropertyDescriptor(Navigator.prototype, 'webdriver').get);

  try {
    const p = await navigator.permissions.query({ name: 'notifications' });
    out.permissionsAgree = p.state === Notification.permission;
  } catch (e) { out.permissionsAgree = false; }

  // Canvas must read the same twice — noise that moves is itself a signal.
  try {
    const c = document.createElement('canvas');
    c.width = 240; c.height = 60;
    const ctx = c.getContext('2d');
    ctx.textBaseline = 'top';
    ctx.font = '14px Arial';
    ctx.fillStyle = '#f60';
    ctx.fillRect(0, 0, 100, 30);
    ctx.fillStyle = '#069';
    ctx.fillText('numera-probe', 2, 15);
    out.canvasStable = c.toDataURL() === c.toDataURL();
    out.canvasEmpty = c.toDataURL().length < 100;
  } catch (e) { out.canvasStable = false; }

  // Font presence by width measurement — the technique real fingerprinters
  // use. document.fonts.check() is unreliable here: a real Chromium (e.g.
  // CloakBrowser, which doesn't run our patch) answers "yes" for *any* family
  // because it always has a fallback, so it can't tell a real font from a
  // made-up one. Rendering a string in the candidate font vs. a base fallback
  // and comparing widths does tell them apart.
  try {
    const host = document.body || document.documentElement;
    const span = document.createElement('span');
    span.style.cssText = 'position:absolute;left:-9999px;top:-9999px;font-size:72px;white-space:nowrap;';
    span.textContent = 'mmmmmmmmmmlli WwMiI0129@';
    host.appendChild(span);
    const bases = {};
    for (const g of ['monospace', 'serif', 'sans-serif']) { span.style.fontFamily = g; bases[g] = span.offsetWidth; }
    const installed = (fam) => {
      for (const g of ['monospace', 'serif', 'sans-serif']) {
        span.style.fontFamily = '"' + fam + '",' + g;
        if (span.offsetWidth !== bases[g]) return true;
      }
      return false;
    };
    out.fontKnown = installed(probe.knownFont);
    out.fontAlien = installed(probe.alienFont);
    host.removeChild(span);
  } catch (e) { out.fontKnown = out.fontAlien = null; }

  try {
    const devs = await navigator.mediaDevices.enumerateDevices();
    out.cameras = devs.filter((d) => d.kind === 'videoinput').length;
    out.microphones = devs.filter((d) => d.kind === 'audioinput').length;
  } catch (e) { out.cameras = out.microphones = null; }

  return out;
}
"""

# A font that exists on no mainstream OS. If font probing says it's installed,
# the answer isn't coming from a real font list.
_ALIEN_FONT = "Numera Nonexistent Grotesk"

# Ask the page which of a candidate list of fonts actually render. Under the
# Chromium stealth engine this is answered by our patched document.fonts.check
# (so it returns the profile's declared set); under cloak/camoufox it hits the
# real OS font list — which is exactly what the aligner wants to read back.
_FONT_PROBE_JS = """
(fonts) => {
  const out = [];
  try {
    const host = document.body || document.documentElement;
    const span = document.createElement('span');
    span.style.cssText = 'position:absolute;left:-9999px;top:-9999px;font-size:72px;white-space:nowrap;';
    span.textContent = 'mmmmmmmmmmlli WwMiI0129@';
    host.appendChild(span);
    const bases = {};
    for (const g of ['monospace', 'serif', 'sans-serif']) { span.style.fontFamily = g; bases[g] = span.offsetWidth; }
    for (const f of fonts) {
      for (const g of ['monospace', 'serif', 'sans-serif']) {
        span.style.fontFamily = '"' + f + '",' + g;
        if (span.offsetWidth !== bases[g]) { out.push(f); break; }
      }
    }
    host.removeChild(span);
  } catch (e) {}
  return out;
}
"""


def _observe(profile: Profile, store: ProfileStore, engine: Optional[str] = None,
             font_candidates: Optional[list] = None) -> dict:
    """Open the profile and read back everything a fingerprinting script can see.

    Optionally probe ``font_candidates`` to learn which fonts the browser can
    actually render (used by the aligner to discover the real OS font set)."""
    fp = profile.fingerprint
    known_font = fp.fonts[0] if fp.fonts else "Arial"
    with session(profile, store, engine) as context:
        page = _serve_probe_page(context)
        seen = page.evaluate(_TRUST_JS, {"knownFont": known_font, "alienFont": _ALIEN_FONT})
        if font_candidates:
            seen["fontsRendered"] = page.evaluate(_FONT_PROBE_JS, font_candidates)
    return seen


def _grade(fp, seen: dict, engine_name: str) -> dict:
    """Score an observation against a fingerprint. Pure — no browser — so the
    aligner can grade a would-be fingerprint against the same observation."""
    checks = [
        _check("navigator.webdriver is false", seen["webdriver"] is False,
               f"reads {seen['webdriver']!r}"),
        _check("webdriver isn't an own property of navigator", not seen["webdriverOwn"],
               "real Chrome defines it on the prototype"),
        _check("window.chrome exists", seen["hasChrome"],
               "missing on plain automated Chrome"),
        _check("navigator.plugins isn't empty", seen["plugins"] > 0,
               f"{seen['plugins']} plugin(s)"),
        _check("User-Agent matches the profile", seen["userAgent"] == fp.user_agent,
               seen["userAgent"]),
        _check("navigator.platform matches the OS", seen["platform"] == fp.platform,
               f"expected {fp.platform}, got {seen['platform']}"),
        _check("Languages match the locale", seen["languages"] == list(fp.languages),
               f"expected {fp.languages}, got {seen['languages']}"),
        _check("CPU cores match", seen["cores"] == fp.hardware_concurrency,
               f"expected {fp.hardware_concurrency}, got {seen['cores']}"),
        _check("Device memory matches", seen["memory"] == fp.device_memory,
               f"expected {fp.device_memory}, got {seen['memory']}"),
        # Alias-aware: the browser answers from CLDR, which still calls
        # `Asia/Kolkata` by its old name, so a string compare failed a profile
        # that was in fact coherent.
        _check("Timezone matches the locale",
               _same_timezone(seen["timezone"], fp.timezone),
               f"expected {fp.timezone}, got {seen['timezone']}"),
        _check("Patched functions still look native", seen["toStringNative"],
               "toString() on a patched function must read '[native code]'"),
        _check("permissions.query agrees with Notification.permission",
               seen["permissionsAgree"]),
        _check("Canvas reads the same twice", bool(seen.get("canvasStable")),
               "noise that changes between reads is a detection signal"),
    ]

    # WebGL and font/media spoofing only apply to the Chromium engines — Camoufox
    # brings its own, so a mismatch there isn't a fault of this profile.
    if engine_name != "camoufox":
        checks.append(_check(
            "WebGL renderer matches the GPU",
            seen["webglRenderer"] == fp.webgl_renderer,
            f"expected {fp.webgl_renderer}, got {seen['webglRenderer']}"))

        # Prefer the full font probe when we have it (the aligner passes it): it
        # lets us re-grade a rewritten font set against the same observation.
        known_font = fp.fonts[0] if fp.fonts else "Arial"
        if "fontsRendered" in seen:
            rendered = set(seen["fontsRendered"])
            font_known = bool(fp.fonts) and known_font in rendered
            font_alien = _ALIEN_FONT in rendered
        else:
            font_known = seen.get("fontKnown")
            font_alien = seen.get("fontAlien")
        if font_known is not None:
            checks.append(_check(
                "Font probing answers from this OS's font set",
                bool(font_known) and not font_alien,
                f"'{known_font}' should exist, '{_ALIEN_FONT}' should not"))

        if seen.get("cameras") is not None:
            checks.append(_check(
                "Media devices match the profile",
                seen["cameras"] == fp.cameras and seen["microphones"] == fp.microphones,
                f"expected {fp.cameras} camera(s)/{fp.microphones} mic(s), "
                f"got {seen['cameras']}/{seen['microphones']}"))

    passed = sum(1 for c in checks if c["ok"])
    score = round(100 * passed / len(checks))
    grade = "A" if score >= 95 else "B" if score >= 85 else "C" if score >= 70 else "D"
    return {"score": score, "grade": grade, "passed": passed,
            "total": len(checks), "checks": checks}


def trust_report(profile: Profile, store: ProfileStore,
                 engine: Optional[str] = None) -> dict:
    """Grade the profile the way a fingerprinting script would.

    Returns ``{score, grade, checks}``. The score is the share of checks that
    pass, so it moves for a real reason and not on a curve.
    """
    seen = _observe(profile, store, engine)
    engine_name = engine or profile.engine or "playwright"
    return _grade(profile.fingerprint, seen, engine_name)
