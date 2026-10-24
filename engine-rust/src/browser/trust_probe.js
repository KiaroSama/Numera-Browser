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
  // Read so the aligner can use the real values instead of guessing a default
  // for the target OS. Both are OS-shaped - a desktop reports 0 touch points,
  // a phone 5; a Retina Mac is DPR 2 where Windows scaling gives 1/1.25/1.5 -
  // so after an OS transition a stale one is a contradiction a detector reads
  // for free.
  out.maxTouchPoints = navigator.maxTouchPoints;
  out.devicePixelRatio = window.devicePixelRatio;
  out.timezone = Intl.DateTimeFormat().resolvedOptions().timeZone;

  try {
    const gl = document.createElement('canvas').getContext('webgl');
    const ext = gl && gl.getExtension('WEBGL_debug_renderer_info');
    out.webglVendor = ext ? gl.getParameter(37445) : null;
    out.webglRenderer = ext ? gl.getParameter(37446) : null;
  } catch (e) { out.webglVendor = out.webglRenderer = null; }

  // A patched function must still report native code — and so must the
  // machinery doing the masking, or the mask is the tell.
  const native = (fn) => /^function [^(]*\(\) \{ \[native code\] \}$/
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
