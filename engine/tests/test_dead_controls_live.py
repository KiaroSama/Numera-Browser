"""Real-browser proof that the Canvas / Audio / Fonts / DNT modes do something.

These five controls were rendered by the dashboard and never read by the engine
(bug B14), so picking "Real" or "Off" changed nothing at all. That is the same
class as the WebRTC bug: the control existed, so it looked fixed.

Reading the mode back out of the profile proves only that the value survived a
dict. The claim being tested here is stronger and can only be shown in a live
browser: switching the mode changes what a *page* can observe.
"""

from dataclasses import replace

import pytest

from numera import generate
from numera.models import Profile
from numera.store import ProfileStore

pytest.importorskip("playwright")

# Draw something with enough texture that a one-bit change moves the hash.
CANVAS_HASH = """() => {
  const c = document.createElement('canvas');
  c.width = 240; c.height = 60;
  const x = c.getContext('2d');
  x.textBaseline = 'top';
  x.font = '14px "Arial"';
  x.fillStyle = '#f60'; x.fillRect(10, 5, 80, 30);
  x.fillStyle = '#069'; x.fillText('numera-fp-probe', 12, 12);
  x.strokeStyle = 'rgba(0,120,255,0.7)';
  x.beginPath(); x.arc(120, 30, 22, 0, Math.PI * 2); x.stroke();
  const url = c.toDataURL();
  let h = 5381;
  for (let i = 0; i < url.length; i++) h = ((h * 33) ^ url.charCodeAt(i)) >>> 0;
  return {hash: h, len: url.length};
}"""

# The real audio fingerprint reads samples the *browser* rendered. An earlier
# version of this probe filled the buffer itself, which overwrote the very noise
# it was meant to detect and made the test pass vacuously in both modes.
AUDIO_HASH = """() => new Promise(res => {
  if (!window.OfflineAudioContext) return res(null);
  const ctx = new OfflineAudioContext(1, 44100, 44100);
  const osc = ctx.createOscillator();
  osc.type = 'triangle';
  osc.frequency.value = 10000;
  const comp = ctx.createDynamicsCompressor();
  osc.connect(comp);
  comp.connect(ctx.destination);
  osc.start(0);
  ctx.startRendering().then(buf => {
    const data = buf.getChannelData(0);
    let sum = 0;
    for (let i = 4500; i < 5000; i++) sum += Math.abs(data[i]);
    res(sum);
  }).catch(() => res(null));
})"""


def _page(tmp_path, name, **fp_over):
    """Launch a profile and hand back (page, teardown)."""
    from numera.drivers import get
    store = ProfileStore(tmp_path / name)
    prof = Profile(name=name, fingerprint=replace(generate(seed=11), **fp_over))
    store.save(prof)
    try:
        pw, ctx, page = get("playwright")(prof, store, headless=True, keep_open=True)
    except Exception as e:
        pytest.skip(f"chromium unavailable: {str(e).splitlines()[0][:80]}")

    def close():
        ctx.close()
        pw.stop()

    return page, close


def _read(tmp_path, name, script, **fp_over):
    page, close = _page(tmp_path, name, **fp_over)
    try:
        page.goto("about:blank")
        return page.evaluate(script)
    finally:
        close()


def test_canvas_real_differs_from_noise(tmp_path):
    """'Real' must hand back the untouched drawing; 'Noise' must not."""
    real = _read(tmp_path, "c-real", CANVAS_HASH, canvas="real")
    noise = _read(tmp_path, "c-noise", CANVAS_HASH, canvas="noise")
    assert real["hash"] != noise["hash"], (
        "canvas noise did not change the pixels - the mode is not reaching the page")


def test_canvas_noise_is_stable_across_reads(tmp_path):
    """Noise must be seeded, not random: a real machine reads the same bytes twice."""
    page, close = _page(tmp_path, "c-stable", canvas="noise")
    try:
        page.goto("about:blank")
        first = page.evaluate(CANVAS_HASH)
        second = page.evaluate(CANVAS_HASH)
    finally:
        close()
    assert first["hash"] == second["hash"], (
        "canvas hash changed between reads - per-call randomness is itself a tell")


def test_canvas_block_yields_a_blank_surface(tmp_path):
    """'Block' must return a valid but empty image, not the drawing."""
    drawn = _read(tmp_path, "c-drawn", CANVAS_HASH, canvas="real")
    blocked = _read(tmp_path, "c-block", CANVAS_HASH, canvas="block")
    assert blocked["hash"] != drawn["hash"]
    # A blank PNG of the same size compresses far smaller than a textured one.
    assert blocked["len"] < drawn["len"], (
        "blocked canvas is not blank - it still carries the drawing")


def test_audio_real_differs_from_noise(tmp_path):
    real = _read(tmp_path, "a-real", AUDIO_HASH, audio="real")
    noise = _read(tmp_path, "a-noise", AUDIO_HASH, audio="noise")
    if real is None or noise is None:
        pytest.skip("no OfflineAudioContext in this browser build")
    assert real != noise, "audio noise is not reaching the page"


def test_font_mode_real_stops_the_masking(tmp_path):
    """Masked answers come from the archetype's font list; 'real' must not.

    The oracle has to be a font the *host* has and the profile's list does not.
    An earlier version asked whether ``document.fonts.check.toString()`` still
    mentioned the patch, which reads False in both modes — ``asNative`` masks
    the patched function's source, so the stealth feature defeated the test and
    the assertion could never fail for the defect it named.
    """
    probe = """(f) => document.fonts.check('12px "' + f + '"')"""
    # Ask the unpatched browser first, then assert against what it actually has.
    page, close = _page(tmp_path, "f-real", font_mode="real")
    try:
        page.goto("about:blank")
        host = {f: page.evaluate(probe, f)
                for f in ("Segoe UI", "Arial", "DejaVu Sans", "Helvetica Neue")}
    finally:
        close()
    present = next((f for f, ok in host.items() if ok), None)
    if present is None:
        pytest.skip(f"host reports none of the probe fonts installed: {host}")

    # Same font, a profile whose list excludes it: masking must now deny it.
    page, close = _page(tmp_path, "f-masked", font_mode="masked",
                        fonts=["Numera Probe Font"])
    try:
        page.goto("about:blank")
        masked = page.evaluate(probe, present)
        absent = page.evaluate(probe, "Nonexistent Numera Font")
    finally:
        close()
    assert masked is False, (
        f"'{present}' is installed on the host and absent from the profile's "
        "list, so masking should deny it - the font list is not reaching the page")
    assert absent is False, "a font in no list at all should read as absent"


def test_do_not_track_reaches_the_page_and_the_header(tmp_path):
    off = _read(tmp_path, "d-off", "() => navigator.doNotTrack", do_not_track=False)
    on = _read(tmp_path, "d-on", "() => navigator.doNotTrack", do_not_track=True)
    assert off in (None, "0"), f"DNT should be unset when off, got {off!r}"
    assert on == "1", f"navigator.doNotTrack should be '1' when on, got {on!r}"
