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

from _live import requires_browser        # noqa: E402

requires_browser()

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


# --- all three canvas read paths -------------------------------------------

THREE_READS = """async () => {
  const c = document.createElement('canvas');
  c.width = 220; c.height = 60;
  const x = c.getContext('2d');
  x.textBaseline = 'top';
  x.font = '14px "Arial"';
  x.fillStyle = '#f60'; x.fillRect(8, 4, 90, 32);
  x.fillStyle = '#069'; x.fillText('numera-three-paths', 10, 12);
  const h = (s) => { let v = 5381;
    for (let i = 0; i < s.length; i++) v = ((v * 33) ^ s.charCodeAt(i)) >>> 0;
    return v; };
  const url = c.toDataURL();
  const px = Array.from(x.getImageData(0, 0, c.width, c.height).data).join(',');
  const blob = await new Promise(r => c.toBlob(b => {
    const fr = new FileReader();
    fr.onload = () => r(fr.result);
    fr.readAsDataURL(b);
  }));
  return { url: h(url), pixels: h(px), blob: h(blob), urlAgain: h(c.toDataURL()) };
}"""


def read_canvas(tmp_path, seed):
    from dataclasses import replace as _replace

    from numera import generate
    from numera.models import Profile
    from numera.store import ProfileStore
    from numera import drivers

    store = ProfileStore(tmp_path / str(seed))
    fp = _replace(generate(os="windows"), seed=seed, canvas="noise")
    p = Profile(name=f"c{seed}", fingerprint=fp)
    store.save(p)
    with drivers.session(p, store, headless=True) as ctx:
        page = ctx.pages[0]
        page.goto("about:blank")
        return page.evaluate(THREE_READS)


@pytest.mark.slow
def test_reading_the_same_canvas_twice_gives_the_same_bytes(tmp_path):
    """Noise that changes between two reads of one canvas is itself the signal -
    a real machine returns identical bytes, and CreepJS checks exactly this."""
    r = read_canvas(tmp_path, 12345)
    assert r["url"] == r["urlAgain"], "two reads of one canvas disagreed"


@pytest.mark.slow
def test_a_different_seed_gives_different_pixels(tmp_path):
    """Otherwise the noise is decoration: every profile would share a hash."""
    a = read_canvas(tmp_path, 111)
    b = read_canvas(tmp_path, 222)
    assert a["url"] != b["url"]
    assert a["pixels"] != b["pixels"], "getImageData is identical across profiles"
    assert a["blob"] != b["blob"], "toBlob is identical across profiles"


@pytest.mark.slow
def test_the_three_read_paths_are_not_a_way_around_each_other(tmp_path):
    """`getImageData` and `toBlob` used to return the *real* canvas while
    `toDataURL` returned a noised one - both a bypass and a contradiction, since
    the same drawing hashed differently depending on the call used."""
    a = read_canvas(tmp_path, 777)
    b = read_canvas(tmp_path, 888)
    # Each path has to move with the seed. A path that does not is unpatched.
    for path in ("url", "pixels", "blob"):
        assert a[path] != b[path], f"{path} does not follow the profile seed"


# --- font masking must cover the technique that is actually used ------------

# The same method probe.py uses: set an inline family on a span and compare
# offsetWidth against the generic fallback. If the width moves, the font is
# installed.
WIDTH_PROBE = """(fonts) => {
  const span = document.createElement('span');
  span.style.position = 'absolute';
  span.style.left = '-9999px';
  span.style.fontSize = '72px';
  span.textContent = 'mmmmmmmmmmlli';
  document.body.appendChild(span);
  const bases = {};
  for (const g of ['monospace', 'serif', 'sans-serif']) {
    span.style.fontFamily = g;
    bases[g] = span.offsetWidth;
  }
  const found = [];
  for (const f of fonts) {
    for (const g of ['monospace', 'serif', 'sans-serif']) {
      span.style.fontFamily = `'${f}',${g}`;
      if (span.offsetWidth !== bases[g]) { found.push(f); break; }
    }
  }
  span.remove();
  return found;
}"""

CANVAS_PROBE = """(fonts) => {
  const c = document.createElement('canvas').getContext('2d');
  const base = {};
  for (const g of ['monospace', 'serif', 'sans-serif']) {
    c.font = `72px ${g}`;
    base[g] = c.measureText('mmmmmmmmmmlli').width;
  }
  const found = [];
  for (const f of fonts) {
    for (const g of ['monospace', 'serif', 'sans-serif']) {
      c.font = `72px '${f}',${g}`;
      if (c.measureText('mmmmmmmmmmlli').width !== base[g]) { found.push(f); break; }
    }
  }
  return found;
}"""


def probe_fonts(tmp_path, font_mode, script, candidates):
    from dataclasses import replace as _replace

    from numera import generate
    from numera.models import Profile
    from numera.store import ProfileStore
    from numera import drivers

    store = ProfileStore(tmp_path / font_mode)
    fp = _replace(generate(os="windows"), font_mode=font_mode)
    p = Profile(name="f", fingerprint=fp)
    store.save(p)
    with drivers.session(p, store, headless=True) as ctx:
        page = ctx.pages[0]
        page.goto("about:blank")
        return set(page.evaluate(script, candidates)), fp


@pytest.mark.slow
def test_width_probing_sees_only_the_masked_font_list(tmp_path):
    """`Fonts: Masked` patched `document.fonts.check` and nothing else, while
    every real fingerprinter - including this project's own probe.py - measures
    offsetWidth. The control reported a protection it did not provide."""
    candidates = ["Segoe UI", "Arial", "Comic Sans MS", "Zapfino",
                  "Hiragino Sans", "Papyrus", "Menlo"]
    found, fp = probe_fonts(tmp_path, "masked", WIDTH_PROBE, candidates)
    allowed = {f.lower() for f in fp.fonts}
    leaked = {f for f in found if f.lower() not in allowed}
    assert not leaked, f"width probing found fonts not in the profile's list: {leaked}"


@pytest.mark.slow
def test_canvas_measure_text_agrees_with_the_same_list(tmp_path):
    """Two probes of the same question must not disagree."""
    candidates = ["Segoe UI", "Zapfino", "Hiragino Sans", "Papyrus"]
    found, fp = probe_fonts(tmp_path, "masked", CANVAS_PROBE, candidates)
    allowed = {f.lower() for f in fp.fonts}
    leaked = {f for f in found if f.lower() not in allowed}
    assert not leaked, f"measureText found fonts not in the profile's list: {leaked}"


@pytest.mark.slow
def test_a_font_the_profile_does_claim_is_still_measurable(tmp_path):
    """Masking that hides everything is its own signal: a machine with no fonts
    at all does not exist.

    The whole claimed list, not a slice of it. `fp.fonts` is alphabetical and a
    Windows profile's first six are almost always Windows-only - `Bahnschrift`,
    `Calibri`, `Candara`, `Cambria` - so on a Linux runner the outcome came down
    to whether plain `Arial`, which fontconfig substitutes, happened to land
    inside those six. It does in 38 of 40 generated profiles, which is exactly
    the failure rate this test had in CI: green on Windows, ~5% red on Linux,
    for a reason that has nothing to do with the control it guards.

    Probing all ~34 is no weaker. If masking hid everything, none of them would
    be found either - and it is the *only* way to state "a machine with no fonts
    does not exist" without also asserting which fonts the host happens to have.
    """
    from numera import generate

    have = list(generate(os="windows").fonts)
    found, _ = probe_fonts(tmp_path, "masked", WIDTH_PROBE, have)
    assert found, f"none of the profile's own fonts could be detected: {have}"
