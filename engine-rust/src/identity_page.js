(() => {
  const cfg = {{CONFIG}};

  // Make an overridden function report native code, so its toString()
  // doesn't reveal the patch.
  const nativeStr = Function.prototype.toString;
  const masked = new WeakMap();
  const asNative = (fn, name) => {
    masked.set(fn, name || fn.name || '');
    return fn;
  };
  const toStringProxy = new Proxy(nativeStr, {
    apply(target, thisArg, args) {
      if (masked.has(thisArg)) return `function ${masked.get(thisArg)}() { [native code] }`;
      return Reflect.apply(target, thisArg, args);
    },
  });
  // eslint-disable-next-line no-extend-native
  Function.prototype.toString = toStringProxy;

  const define = (obj, prop, getter) => {
    try {
      Object.defineProperty(obj, prop, { get: asNative(getter, `get ${prop}`), configurable: true, enumerable: true });
    } catch (e) {}
  };

  // --- webdriver: false, and defined on the prototype like real Chrome ---
  try { delete Object.getPrototypeOf(navigator).webdriver; } catch (e) {}
  try { delete navigator.webdriver; } catch (e) {}
  define(Navigator.prototype, 'webdriver', () => false);

  // --- Navigator basics ---
  define(Navigator.prototype, 'platform', () => cfg.platform);
  define(Navigator.prototype, 'hardwareConcurrency', () => cfg.hardwareConcurrency);
  define(Navigator.prototype, 'languages', () => Object.freeze(cfg.languages.slice()));
  define(Navigator.prototype, 'maxTouchPoints', () => cfg.maxTouchPoints);

  // deviceMemory: 0 is not '0 GB', it is 'this platform has no such
  // property' — WebKit never shipped it, so an iOS profile must not have
  // one. Answering with a number would be a harder tell than the value.
  if (cfg.deviceMemory) {
    define(Navigator.prototype, 'deviceMemory', () => cfg.deviceMemory);
  } else {
    try { delete Navigator.prototype.deviceMemory; } catch (e) {}
    try { delete navigator.deviceMemory; } catch (e) {}
  }

  // --- Touch surface ---
  // maxTouchPoints alone is not enough: the usual mobile test is feature
  // detection ('ontouchstart' in window), and a phone that reports five
  // touch points while failing that check is contradicting itself.
  try {
    if (cfg.maxTouchPoints > 0) {
      if (!('ontouchstart' in window))
        Object.defineProperty(window, 'ontouchstart', { value: null, writable: true, configurable: true });
    } else if ('ontouchstart' in window) {
      delete window.ontouchstart;
    }
  } catch (e) {}

  // --- devicePixelRatio ---
  // Playwright sets a context-level device scale factor, but it is a flat
  // 2-for-mobile / 1-for-desktop guess, so real display scaling (a 125%
  // Windows laptop, a 3x phone panel) never reaches the page without this.
  try {
    define(window, 'devicePixelRatio', () => cfg.devicePixelRatio);
  } catch (e) {}

  // --- User-Agent Client Hints ---
  // The header is set on the context, but navigator.userAgentData was left
  // reporting the *host* — a macOS profile on a Windows box answered
  // platform 'Windows' in JS while navigator.platform said 'MacIntel'.
  // Patch the real prototype getters rather than substituting an object,
  // so the shape, prototype chain and instanceof all stay genuine.
  try {
    if (window.NavigatorUAData) {
      const uaProto = NavigatorUAData.prototype;
      if (!cfg.hasUaData) {
        delete Navigator.prototype.userAgentData;
      } else {
        const values = {
          brands: () => cfg.uaBrands.map((b) => ({ brand: b.brand, version: b.version })),
          mobile: () => cfg.isMobile,
          platform: () => cfg.uaPlatform,
        };
        for (const key of Object.keys(values)) define(uaProto, key, values[key]);
        const high = uaProto.getHighEntropyValues;
        uaProto.getHighEntropyValues = asNative(function getHighEntropyValues(hints) {
          return Promise.resolve(high.call(this, hints)).catch(() => ({})).then((base) => {
            const out = Object.assign({}, base, {
              brands: values.brands(), mobile: cfg.isMobile, platform: cfg.uaPlatform,
            });
            const want = Array.isArray(hints) ? hints : [];
            if (want.includes('platformVersion')) out.platformVersion = cfg.platformVersion;
            if (want.includes('architecture')) out.architecture = cfg.architecture;
            if (want.includes('bitness')) out.bitness = '64';
            if (want.includes('model')) out.model = cfg.deviceModel || '';
            if (want.includes('uaFullVersion')) out.uaFullVersion = cfg.fullVersion;
            if (want.includes('fullVersionList')) out.fullVersionList =
              cfg.uaBrands.map((b) => ({ brand: b.brand,
                version: b.brand === 'Not-A.Brand' ? '99.0.0.0' : cfg.fullVersion }));
            if (want.includes('wow64')) out.wow64 = false;
            if (want.includes('formFactors')) out.formFactors = [cfg.isMobile ? 'Mobile' : 'Desktop'];
            return out;
          });
        }, 'getHighEntropyValues');
      }
    }
  } catch (e) {}

  // --- window.chrome (present on real Chrome, missing under automation) ---
  try {
    if (!window.chrome) window.chrome = {};
    if (!window.chrome.runtime) window.chrome.runtime = {};
    if (!window.chrome.app) window.chrome.app = { isInstalled: false };
  } catch (e) {}

  // --- Plugins & mimeTypes ---
  // Headed Chrome already exposes the authentic PDF PluginArray, so we
  // only synthesize one when the list is empty (typically headless), and
  // we wire up the real prototypes/tags so it still reads as native.
  try {
    if (navigator.plugins.length === 0 &&
        window.PluginArray && window.MimeTypeArray && window.Plugin && window.MimeType) {
      const pdfNames = [
        'PDF Viewer', 'Chrome PDF Viewer', 'Chromium PDF Viewer',
        'Microsoft Edge PDF Viewer', 'WebKit built-in PDF',
      ];
      const mkMime = (type) => {
        const m = { type, suffixes: 'pdf', description: 'Portable Document Format' };
        Object.setPrototypeOf(m, MimeType.prototype);
        return m;
      };
      const mimes = [mkMime('application/pdf'), mkMime('text/pdf')];
      const mkArr = (items, proto, key) => {
        const arr = {};
        items.forEach((it, i) => { arr[i] = it; });
        Object.defineProperty(arr, 'length', { value: items.length, enumerable: false });
        arr.item = asNative((i) => items[i] || null, 'item');
        arr.namedItem = asNative((n) => items.find((x) => x[key] === n) || null, 'namedItem');
        Object.setPrototypeOf(arr, proto);
        return arr;
      };
      const plugins = pdfNames.map((name) => {
        const p = { name, filename: 'internal-pdf-viewer', description: 'Portable Document Format' };
        mimes.forEach((m, i) => { p[i] = m; });
        Object.defineProperty(p, 'length', { value: mimes.length, enumerable: false });
        p.item = asNative((i) => mimes[i] || null, 'item');
        p.namedItem = asNative((t) => mimes.find((m) => m.type === t) || null, 'namedItem');
        Object.setPrototypeOf(p, Plugin.prototype);
        return p;
      });
      const pluginArray = mkArr(plugins, PluginArray.prototype, 'name');
      const mimeArray = mkArr(mimes, MimeTypeArray.prototype, 'type');
      define(Navigator.prototype, 'plugins', () => pluginArray);
      define(Navigator.prototype, 'mimeTypes', () => mimeArray);
    }
  } catch (e) {}

  // --- permissions.query agrees with Notification.permission ---
  try {
    const orig = navigator.permissions.query.bind(navigator.permissions);
    navigator.permissions.query = asNative(function query(params) {
      if (params && params.name === 'notifications')
        return Promise.resolve({ state: Notification.permission, onchange: null });
      return orig(params);
    }, 'query');
  } catch (e) {}

  // --- WebGL vendor / renderer (UNMASKED params 37445 / 37446) ---
  const patchGL = (proto) => {
    if (!proto) return;
    const orig = proto.getParameter;
    proto.getParameter = asNative(function getParameter(p) {
      if (p === 37445) return cfg.webglVendor;
      if (p === 37446) return cfg.webglRenderer;
      return orig.call(this, p);
    }, 'getParameter');
    // The vendor/renderer strings are metadata. The stronger WebGL
    // fingerprint is the *rendered image*: a page draws a known scene and
    // hashes readPixels(), which reports the real GPU's rasterisation no
    // matter what getParameter claims. Spoofing only the strings leaves
    // that hash tied to the host, so every profile on one machine shares
    // it. Perturb the low bit of a sparse, seed-chosen set of samples:
    // enough to move the hash, far too little to be visible.
    if (cfg.webglImage === 'real') return;
    const origRead = proto.readPixels;
    if (!origRead) return;
    proto.readPixels = asNative(function readPixels(x, y, w, h, fmt, type, pixels) {
      const out = origRead.apply(this, arguments);
      try {
        if (pixels && pixels.length) {
          // Reseeded per call from the request geometry, so reading the
          // same region twice gives the same bytes — per-call randomness
          // is itself detectable (see the canvas note below).
          const r = mkRnd(cfg.seed ^ (w * 73856093) ^ (h * 19349663));
          for (let i = 0; i < pixels.length; i += 4) {
            if (r() < 0.02) pixels[i] ^= 1;
          }
        }
      } catch (e) {}
      return out;
    }, 'readPixels');
  };

  // --- Deterministic noise ---
  // Every noise source draws from a PRNG restarted from the profile seed,
  // never from a single advancing stream. That matters: on a real machine
  // reading the same canvas twice gives the same bytes, so noise that
  // changes per call is itself a detection signal (CreepJS checks for it).
  const mkRnd = (seed) => {
    let s = (seed >>> 0) || 1;
    return () => { s = (s * 1664525 + 1013904223) >>> 0; return s / 4294967296; };
  };
  const rnd = mkRnd(cfg.seed);   // init-time draws (device ids, rect jitter)

  // Applied here rather than where patchGL is defined: its readPixels
  // noise needs mkRnd, which is declared just above.
  if (window.WebGLRenderingContext) patchGL(WebGLRenderingContext.prototype);
  if (window.WebGL2RenderingContext) patchGL(WebGL2RenderingContext.prototype);

  // --- Canvas readback -------------------------------------------
  // A page can read a canvas three ways: toDataURL, toBlob, and
  // getImageData. Only the first was covered, so the other two returned
  // the real pixels - both a bypass and, worse, a contradiction: the
  // same canvas hashed differently depending on which call was used, and
  // nothing on a real machine behaves that way.
  const origToDataURL = HTMLCanvasElement.prototype.toDataURL;
  const origToBlob = HTMLCanvasElement.prototype.toBlob;
  const origGetImageData = window.CanvasRenderingContext2D &&
    CanvasRenderingContext2D.prototype.getImageData;

  // Noise is applied to a throwaway copy, never the caller's canvas, so
  // the page keeps its real pixels and repeated reads stay identical.
  // The seed comes from the profile and the canvas size only - not from
  // a call counter - because a hash that changes between two reads of
  // one canvas is itself what CreepJS looks for.
  const blankLike = (c) => {
    const b = document.createElement('canvas');
    b.width = c.width || 1; b.height = c.height || 1;
    return b;
  };
  const noisyCopy = (canvas) => {
    if (cfg.canvas === 'block') return blankLike(canvas);
    // A WebGL canvas has no 2d context but is still a valid drawImage
    // source. Ask for 2d first: on a canvas with no context that call
    // creates one, which stops the WebGL probe turning a blank canvas
    // into a GL one behind the page's back.
    const ctx2d = canvas.getContext('2d');
    const isGl = !ctx2d && cfg.webglImage !== 'real' &&
      !!(canvas.getContext('webgl') || canvas.getContext('webgl2') ||
         canvas.getContext('experimental-webgl'));
    if (!(ctx2d || isGl) || !canvas.width || !canvas.height) return null;
    const copy = document.createElement('canvas');
    copy.width = canvas.width;
    copy.height = canvas.height;
    const cctx = copy.getContext('2d');
    cctx.drawImage(canvas, 0, 0);
    const r = mkRnd(cfg.seed ^ (canvas.width * 73856093) ^ (canvas.height * 19349663));
    // The *original* getImageData: the patched one below would send this
    // straight back through here and noise the copy twice.
    const img = origGetImageData.call(cctx, 0, 0, copy.width, copy.height);
    for (let i = 0; i < img.data.length; i += 4) {
      if (r() < 0.02) {
        img.data[i]   ^= (r() * 2) | 0;
        img.data[i+1] ^= (r() * 2) | 0;
        img.data[i+2] ^= (r() * 2) | 0;
      }
    }
    cctx.putImageData(img, 0, 0);
    return copy;
  };

  if (cfg.canvas !== 'real') {
    HTMLCanvasElement.prototype.toDataURL = asNative(function toDataURL(...args) {
      try {
        const c = noisyCopy(this);
        if (c) return origToDataURL.apply(c, args);
      } catch (e) {}
      return origToDataURL.apply(this, args);
    }, 'toDataURL');

    if (origToBlob) {
      HTMLCanvasElement.prototype.toBlob = asNative(function toBlob(...args) {
        try {
          const c = noisyCopy(this);
          if (c) return origToBlob.apply(c, args);
        } catch (e) {}
        return origToBlob.apply(this, args);
      }, 'toBlob');
    }

    if (origGetImageData) {
      CanvasRenderingContext2D.prototype.getImageData = asNative(
        function getImageData(sx, sy, sw, sh, ...rest) {
          try {
            const c = noisyCopy(this.canvas);
            if (c) {
              return origGetImageData.call(c.getContext('2d'), sx, sy, sw, sh, ...rest);
            }
          } catch (e) {}
          return origGetImageData.call(this, sx, sy, sw, sh, ...rest);
        }, 'getImageData');
    }
  }

  // --- Installed fonts ---
  // Font probing asks 'can you render X?'. Answer from the archetype's
  // real font set so the reply matches the OS we claim to be.
  try {
    if (cfg.fontMode !== 'real' && cfg.fonts.length && document.fonts && document.fonts.check) {
      const installed = new Set(cfg.fonts.map((f) => f.toLowerCase()));
      const generic = new Set(['serif', 'sans-serif', 'monospace', 'cursive',
                               'fantasy', 'system-ui', 'ui-sans-serif', 'ui-serif',
                               'ui-monospace', 'ui-rounded', '-apple-system']);
      const origCheck = document.fonts.check.bind(document.fonts);
      document.fonts.check = asNative(function check(font, text) {
        try {
          // font looks like '12px "Segoe UI"' — pull out the family.
          const family = String(font).replace(/^\s*[\w.]+\s+/, '')
                                     .replace(/["']/g, '').split(',')[0].trim();
          const key = family.toLowerCase();
          if (generic.has(key)) return origCheck(font, text);
          return installed.has(key);
        } catch (e) { return origCheck(font, text); }
      }, 'check');

      // `document.fonts.check` is the polite way to ask. Nobody serious
      // uses it: the standard probe sets an inline font-family on a span
      // and compares offsetWidth against the generic fallback - which is
      // exactly what this project's own probe.py does. Masking only the
      // polite path meant `Fonts: Masked` reported a protection it did
      // not provide, and our own probe could prove it.
      //
      // The measurement is redirected, not faked: an unavailable family
      // is measured with the rest of the stack, which is what the browser
      // would do if the font really were missing. Pages set fonts through
      // stylesheets, so `element.style.fontFamily` is empty for them and
      // this returns immediately - only inline-styled probes take the
      // slow path.
      const familyOf = (spec) => String(spec || '').split(',')[0]
        .replace(/["']/g, '').trim().toLowerCase();
      const hidden = (spec) => {
        const key = familyOf(spec);
        return !!key && !generic.has(key) && !installed.has(key);
      };
      const fallbackOf = (spec) => {
        const rest = String(spec).split(',').slice(1).join(',').trim();
        return rest || 'sans-serif';
      };

      for (const prop of ['offsetWidth', 'offsetHeight']) {
        const d = Object.getOwnPropertyDescriptor(HTMLElement.prototype, prop);
        if (!d || !d.get) continue;
        Object.defineProperty(HTMLElement.prototype, prop, {
          configurable: true, enumerable: d.enumerable,
          get: asNative(function () {
            try {
              const spec = this.style && this.style.fontFamily;
              if (spec && hidden(spec)) {
                const prev = this.style.fontFamily;
                this.style.fontFamily = fallbackOf(spec);
                const v = d.get.call(this);
                this.style.fontFamily = prev;
                return v;
              }
            } catch (e) {}
            return d.get.call(this);
          }, prop),
        });
      }

      // The canvas version of the same probe.
      if (window.CanvasRenderingContext2D) {
        const cproto = CanvasRenderingContext2D.prototype;
        const om = cproto.measureText;
        cproto.measureText = asNative(function measureText(text) {
          try {
            const spec = String(this.font || '');
            const fam = spec.replace(/^\s*[\w.]+\s+/, '');
            if (hidden(fam)) {
              const prev = this.font;
              this.font = spec.replace(fam, fallbackOf(fam));
              const m = om.call(this, text);
              this.font = prev;
              return m;
            }
          } catch (e) {}
          return om.call(this, text);
        }, 'measureText');
      }
    }
  } catch (e) {}

  // --- Media devices (cameras / microphones) ---
  // Ids are derived from the seed so they stay stable for this profile.
  try {
    if (cfg.mediaMode === 'real') throw 0;   // report the host's real devices
    if (navigator.mediaDevices && navigator.mediaDevices.enumerateDevices) {
      const hex = (n) => {
        let out = '';
        for (let i = 0; i < n; i++) out += Math.floor(rnd() * 16).toString(16);
        return out;
      };
      // MediaDeviceInfo exposes its fields as prototype getters, so the
      // values have to come from there too — an object with own
      // properties would be the wrong shape and easy to spot.
      const values = new WeakMap();
      const proto = MediaDeviceInfo.prototype;
      for (const key of ['deviceId', 'kind', 'label', 'groupId']) {
        const desc = Object.getOwnPropertyDescriptor(proto, key);
        if (!desc || !desc.get) continue;
        const origGet = desc.get;
        Object.defineProperty(proto, key, {
          get: asNative(function () {
            const own = values.get(this);
            return own ? own[key] : origGet.call(this);
          }, `get ${key}`),
          enumerable: desc.enumerable, configurable: true,
        });
      }
      const mkDevice = (kind) => {
        // Only inputs are InputDeviceInfo; speakers stay MediaDeviceInfo.
        const base = (kind !== 'audiooutput' && window.InputDeviceInfo)
          ? InputDeviceInfo.prototype : proto;
        const dev = Object.create(base);
        values.set(dev, { deviceId: hex(64), kind, label: '', groupId: hex(64) });
        return dev;
      };
      const build = () => {
        const list = [];
        for (let i = 0; i < cfg.cameras; i++) list.push(mkDevice('videoinput'));
        for (let i = 0; i < cfg.microphones; i++) list.push(mkDevice('audioinput'));
        for (let i = 0; i < cfg.speakers; i++) list.push(mkDevice('audiooutput'));
        return list;
      };
      const devices = build();
      navigator.mediaDevices.enumerateDevices = asNative(
        function enumerateDevices() { return Promise.resolve(devices.slice()); },
        'enumerateDevices');
    }
  } catch (e) {}

  // --- ClientRects: sub-pixel noise, seeded and stable ---
  // Real machines differ by fractions of a pixel; identical rects across
  // profiles are the giveaway, so each profile gets its own tiny offset.
  try {
    if (cfg.clientRects === 'real') throw 0;   // leave layout measurement alone
    const jitter = (rnd() - 0.5) * 0.0002;
    const nudge = (v) => (typeof v === 'number' && v !== 0 ? v + v * jitter : v);
    const patchRect = (proto, name) => {
      const orig = proto[name];
      if (!orig) return;
      proto[name] = asNative(function (...args) {
        const r = orig.apply(this, args);
        if (name === 'getClientRects') {
          for (const rect of r) shift(rect);
          return r;
        }
        return shift(r);
      }, name);
    };
    const shift = (r) => {
      if (!r) return r;
      const out = { x: nudge(r.x), y: nudge(r.y), width: nudge(r.width),
                    height: nudge(r.height), top: nudge(r.top), right: nudge(r.right),
                    bottom: nudge(r.bottom), left: nudge(r.left) };
      for (const k of Object.keys(out)) {
        try { Object.defineProperty(r, k, { get: asNative(() => out[k], `get ${k}`), configurable: true }); }
        catch (e) {}
      }
      return r;
    };
    patchRect(Element.prototype, 'getBoundingClientRect');
    patchRect(Element.prototype, 'getClientRects');
    patchRect(Range.prototype, 'getBoundingClientRect');
  } catch (e) {}

  // --- Audio context: seeded noise, restarted per read so it stays stable ---
  try {
    if (cfg.audio === 'real') throw 0;   // leave the audio stack untouched
    if (window.AnalyserNode) {
      const origFreq = AnalyserNode.prototype.getFloatFrequencyData;
      AnalyserNode.prototype.getFloatFrequencyData = asNative(
        function getFloatFrequencyData(array) {
          const out = origFreq.apply(this, arguments);
          const r = mkRnd(cfg.seed ^ 0x5eed1);
          for (let i = 0; i < array.length; i += 100) array[i] += (r() - 0.5) * 1e-7;
          return out;
        }, 'getFloatFrequencyData');
    }
    if (window.AudioBuffer) {
      // getChannelData hands back the live buffer, so noise it once and
      // remember it — adding on every call would make the value drift.
      const noised = new WeakSet();
      const origGet = AudioBuffer.prototype.getChannelData;
      AudioBuffer.prototype.getChannelData = asNative(function getChannelData(ch) {
        const data = origGet.call(this, ch);
        if (!noised.has(data)) {
          noised.add(data);
          const r = mkRnd(cfg.seed ^ 0x5eed2 ^ (ch | 0));
          for (let i = 0; i < data.length; i += 500) data[i] += (r() - 0.5) * 1e-7;
        }
        return data;
      }, 'getChannelData');
    }
  } catch (e) {}

  // --- Do Not Track ---
  // The header is set on the context; navigator.doNotTrack has to agree with
  // it, or the mismatch is itself a signal.
  try {
    Object.defineProperty(Navigator.prototype, 'doNotTrack', {
      get: asNative(function doNotTrack() { return cfg.doNotTrack ? '1' : null; },
                    'doNotTrack'),
      configurable: true,
    });
  } catch (e) {}

  // ---- WebRTC -------------------------------------------------
  // Chromium's --force-webrtc-ip-handling-policy is the first line of
  // defence, but it does not reliably stop ICE from offering a public
  // address that is not the proxy's, which links the session straight
  // back to the real IP. Private and mDNS candidates are deliberately left
  // alone: a real browser behind NAT offers those, so stripping them would
  // be its own tell.
  try {
    if (cfg.webrtc === 'disabled') {
      for (const k of ['RTCPeerConnection', 'webkitRTCPeerConnection',
                       'RTCDataChannel', 'mozRTCPeerConnection']) {
        try { delete window[k]; } catch (e) { window[k] = undefined; }
      }
    } else if (cfg.webrtc !== 'real' && window.RTCPeerConnection) {
      const RTC = window.RTCPeerConnection;
      // The connection-address is the 5th field of a candidate line:
      //   candidate:<foundation> <component> <transport> <priority> <addr> <port> ...
      const addrOf = (line) => {
        const m = /^(?:a=)?candidate:\S+ \d+ \S+ \d+ (\S+) /.exec(line || '');
        return m ? m[1] : null;
      };
      // Anything not routable on the public internet is safe to keep: a
      // real browser behind NAT offers those, so stripping them would be
      // its own tell. Everything else is the address we are hiding.
      const privateV4 = (ip) =>
        /^(10\.|127\.|0\.|169\.254\.|192\.168\.|172\.(1[6-9]|2\d|3[01])\.)/.test(ip);
      // fe80::/10 link-local, fc00::/7 unique-local, ::1 loopback, ::
      // unspecified. A global IPv6 used to fall through this check
      // entirely, so on a v6-capable network the real address was still
      // offered while the v4 one was being scrubbed.
      const privateV6 = (ip) => {
        const a = ip.toLowerCase().split('%')[0];   // drop any zone id
        if (a === '::1' || a === '::') return true;
        if (/^fe[89ab]/.test(a)) return true;       // fe80::/10
        if (/^f[cd]/.test(a)) return true;          // fc00::/7
        return false;
      };
      const safe = (line) => {
        if (/\.local\b/.test(line)) return true;   // mDNS, already anonymous
        const ip = addrOf(line);
        if (!ip) return true;                        // no address to leak
        if (/\.local$/i.test(ip)) return true;
        return ip.indexOf(':') >= 0 ? privateV6(ip) : privateV4(ip);
      };
      const scrub = (sdp) => typeof sdp === 'string'
        ? sdp.split(/\r?\n/).filter(l => !/^a=candidate:/.test(l) || safe(l)).join('\r\n')
        : sdp;
      const drop = (ev) => {
        const c = ev && ev.candidate && ev.candidate.candidate;
        return !!c && !safe(c);   // keep the null end-of-candidates event
      };

      const wrapped = new WeakMap();
      const origAdd = RTC.prototype.addEventListener;
      RTC.prototype.addEventListener = asNative(function addEventListener(type, fn) {
        if (type === 'icecandidate' && typeof fn === 'function') {
          let w = wrapped.get(fn);
          if (!w) {
            w = function (ev) { if (!drop(ev)) return fn.apply(this, arguments); };
            wrapped.set(fn, w);
          }
          return origAdd.call(this, type, w, arguments[2]);
        }
        return origAdd.apply(this, arguments);
      }, 'addEventListener');
      const origRemove = RTC.prototype.removeEventListener;
      RTC.prototype.removeEventListener = asNative(function removeEventListener(type, fn) {
        if (type === 'icecandidate' && wrapped.has(fn)) {
          return origRemove.call(this, type, wrapped.get(fn), arguments[2]);
        }
        return origRemove.apply(this, arguments);
      }, 'removeEventListener');

      const onice = Object.getOwnPropertyDescriptor(RTC.prototype, 'onicecandidate');
      if (onice && onice.set) {
        const handlers = new WeakMap();
        Object.defineProperty(RTC.prototype, 'onicecandidate', {
          configurable: true,
          enumerable: onice.enumerable,
          get() { return handlers.get(this) || null; },
          set(fn) {
            handlers.set(this, fn);
            onice.set.call(this, typeof fn === 'function'
              ? function (ev) { if (!drop(ev)) return fn.apply(this, arguments); }
              : fn);
          },
        });
      }

      // Candidates also travel inside the SDP, so a page that never listens
      // for the event can still read them off the description.
      for (const prop of ['localDescription', 'currentLocalDescription',
                          'pendingLocalDescription']) {
        const d = Object.getOwnPropertyDescriptor(RTC.prototype, prop);
        if (!d || !d.get) continue;
        Object.defineProperty(RTC.prototype, prop, {
          configurable: true,
          enumerable: d.enumerable,
          get() {
            const desc = d.get.call(this);
            if (!desc) return desc;
            const sdp = scrub(desc.sdp);
            return { type: desc.type, sdp, toJSON: () => ({ type: desc.type, sdp }) };
          },
        });
      }
      for (const m of ['createOffer', 'createAnswer']) {
        const orig = RTC.prototype[m];
        if (!orig) continue;
        RTC.prototype[m] = asNative(function () {
          const out = orig.apply(this, arguments);
          return out && typeof out.then === 'function'
            ? out.then(d => (d && d.sdp ? { type: d.type, sdp: scrub(d.sdp) } : d))
            : out;
        }, m);
      }
    }
  } catch (e) {}
})();
