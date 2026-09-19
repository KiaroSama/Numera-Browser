/* Device archetypes and the coherence check, mirroring the Python engine.
 *
 * Every value here has a counterpart in `numera/devices.py`,
 * `numera/drivers.py` or `numera/fingerprint.py`. The dashboard needs its own
 * copy so the live coherence meter can score an edit without a round trip -
 * but the engine is the authority, and these must be updated together. */

/* ---- warm-up verticals (mirrors numera/warmup.py PRESETS) ------- */
const WARMUP_PRESETS = [
  { key: "general", label: "General" },
  { key: "ads", label: "Ads / media" },
  { key: "ecommerce", label: "E-commerce" },
  { key: "crypto", label: "Crypto" },
  { key: "social", label: "Social" },
  { key: "news", label: "News" },
];

/* ---- device data (mirrors numera/devices.py) -------------------- */
const OS_LIST = ["Windows", "macOS", "Linux", "Android"];

const GPUS = {
  Windows: [
    "ANGLE (NVIDIA GeForce RTX 3060 Direct3D11)",
    "ANGLE (NVIDIA GeForce GTX 1660 Ti Direct3D11)",
    "ANGLE (Intel UHD Graphics 630 Direct3D11)",
    "ANGLE (AMD Radeon RX 6600 Direct3D11)",
  ],
  macOS: [
    "ANGLE (Apple M1, OpenGL 4.1)",
    "ANGLE (Apple M2, OpenGL 4.1)",
    "ANGLE (Intel Iris Plus Graphics, OpenGL 4.1)",
  ],
  Linux: [
    "ANGLE (NVIDIA GeForce GTX 1050 Ti, OpenGL 4.5)",
    "ANGLE (Mesa Intel UHD Graphics, OpenGL 4.6)",
  ],
  Android: [
    "ANGLE (Qualcomm Adreno 660, OpenGL ES 3.2)",
    "ANGLE (ARM Mali-G78, OpenGL ES 3.2)",
  ],
};

const SCREENS = {
  Windows: ["1920x1080", "1366x768", "2560x1440", "1536x864"],
  macOS: ["2560x1600", "1440x900", "1728x1117"],
  Linux: ["1920x1080", "1366x768"],
  Android: ["360x800", "393x873", "412x915"],
};

const CORES = { Windows: [4, 6, 8, 12, 16], macOS: [8, 10, 12], Linux: [4, 8, 16], Android: [6, 8] };

const RAM = { Windows: [8, 16, 32], macOS: [8, 16, 32], Linux: [8, 16], Android: [4, 6, 8] };

const LOCALES = {
  "en-US": "America/New_York", "en-GB": "Europe/London", "en-PK": "Asia/Karachi",
  "en-IN": "Asia/Kolkata", "de-DE": "Europe/Berlin", "fr-FR": "Europe/Paris",
  "es-ES": "Europe/Madrid", "tr-TR": "Europe/Istanbul",
  "it-IT": "Europe/Rome", "nl-BE": "Europe/Brussels",
};

/* `UK` is an ALIAS, not a country. ISO 3166-1 alpha-2 spells the United
 * Kingdom `GB`; `UK` is a reserved code that proxy lists and CSV exports use
 * anyway, so the lookup accepts it - the engine's `devices.py` carries the same
 * pair for the same reason. What it must not do is appear in a PICKER: the bulk
 * dialog built its country chips from `Object.keys` here, so `GB` and `UK` sat
 * side by side as if they were two places. Choosing both spread one country
 * across two stored values, and every later comparison of `proxy.country` -
 * the coherence rule below included - reads those as different. */
const COUNTRY_LOCALE = { US: "en-US", GB: "en-GB", UK: "en-GB", PK: "en-PK", IN: "en-IN", DE: "de-DE", FR: "fr-FR", ES: "es-ES", TR: "tr-TR", IT: "it-IT", BE: "nl-BE" };

//: The countries a user may CHOOSE - the canonical code for each place, once.
const COUNTRIES = Object.keys(COUNTRY_LOCALE).filter(c => c !== "UK");

/* Every IANA zone the runtime knows - 418 of them on current Chrome and Node,
 * `Asia/Tehran` and the rest included.
 *
 * Asked from the platform rather than written down. A hardcoded list is wrong
 * the first time a zone is added or renamed, and `LOCALES` above - the only
 * timezone source this file used to have - knows ten, which is what the editor
 * could offer while accepting any string typed into it.
 *
 * The fallback matters for old runtimes only: `supportedValuesOf` is Chrome 99
 * and Firefox 93. Sorted so the picker reads alphabetically wherever it lands. */
function knownTimezones() {
  let zones = [];
  try {
    const got = Intl.supportedValuesOf?.("timeZone");
    if (Array.isArray(got)) zones = got;
  } catch { /* an older runtime: fall through to this project's own */ }
  // The platform answers with CANONICAL names, which are not always the ones
  // this project uses: Node and Chrome list `Asia/Calcutta`, while `LOCALES`
  // here and `devices.py` both say `Asia/Kolkata`. Both are real and both
  // resolve, so union them - otherwise the picker cannot offer the default
  // that `en-IN` profiles are already created with.
  return [...new Set([...zones, ...Object.values(LOCALES)])].sort();
}

const TIMEZONES = knownTimezones();

/* Does the platform accept this zone? Membership in `TIMEZONES` is the wrong
 * question - it answers "would we offer it", and the alias above is proof that
 * a zone can be valid without being listed. `DateTimeFormat` throws on a zone
 * that is not one, which is the only check that follows the runtime rather
 * than a table someone has to maintain. */
function validTimezone(zone) {
  if (!zone) return false;
  try {
    new Intl.DateTimeFormat("en", { timeZone: zone });
    return true;
  } catch {
    return false;
  }
}

const MODES = {
  canvas: ["Noise", "Block", "Real"], webrtc: ["Altered", "Disabled", "Real"],
  audio: ["Noise", "Real"], fonts: ["Masked", "Real"], geo: ["Prompt", "Allow", "Block"],
  // Surfaces the engine has always read from the profile - `to_engine_profile`
  // maps every one of these - but which the editor never offered.
  clientRects: ["Noise", "Real"], webglImage: ["Noise", "Real"],
  mediaDevices: ["Masked", "Real"],
};

/* ---- launch engines (mirrors numera/drivers.py) ----------------- */
const ENGINE_ORDER = ["cloak", "camoufox", "patchright", "playwright"];

const ENGINE_META = {
  cloak: {
    label: "CloakBrowser", kind: "Patched Chromium", strength: 4, recommended: true,
    tagline: "Fingerprints patched at the C++ source level.",
    desc: "A real Chromium binary, not JavaScript tricks. Reaches TLS/CDP-grade detection (Cloudflare, DataDome, reCAPTCHA v3) that injection can't touch.",
    install: "pip install cloakbrowser", docs: "https://github.com/CloakHQ/CloakBrowser",
  },
  camoufox: {
    label: "Camoufox", kind: "Patched Firefox", strength: 4, recommended: true,
    tagline: "Engine-level spoofing with its own coherent fingerprint.",
    desc: "A patched Firefox that spoofs at the C++ level and generates its own consistent identity. Among the strongest open options.",
    install: 'pip install "camoufox[geoip]" && camoufox fetch', docs: "https://github.com/daijro/camoufox",
  },
  patchright: {
    label: "Patchright", kind: "Patched Playwright", strength: 3, recommended: false,
    tagline: "Drop-in patched Playwright.",
    desc: "Closes navigator.webdriver and CDP leaks the stock engine leaves behind. Same API you already know, meaningfully stealthier.",
    install: "pip install patchright && patchright install chromium",
    docs: "https://github.com/Kaliiiiiiiiii-Vinyzu/patchright-python",
  },
  playwright: {
    label: "Playwright", kind: "Stock Chromium", strength: 2, recommended: false, builtin: true,
    tagline: "Built in. Works out of the box.",
    desc: "Stock Chromium plus Numera's injected stealth script. Clears basic bot tests (BrowserScan, sannysoft); not Cloudflare-grade.",
    install: null, docs: null,
  },
};

const CHROME_VERSIONS = ["125.0.0.0", "126.0.0.0", "127.0.0.0", "128.0.0.0"];

/* Engines that build their OWN identity and read the profile only as hints.
 *
 * `drivers.py:_camoufox_kwargs` hands Camoufox exactly four things - `os`,
 * `locale`, the proxy, and `geoip` when the profile asked to follow its exit IP.
 * Nothing else is passed. The browser, its version, the user agent, the screen,
 * the cores, the memory and every WebGL/canvas/audio/font setting are generated
 * by Camoufox itself, coherently, as a Firefox.
 *
 * So a profile row that reads "Chrome 128" for a camoufox profile is stating
 * something that will not be true the moment it launches - and camoufox is the
 * recommended engine, which makes it the common case, not the corner one. The
 * dashboard says "generated" for those fields instead, and stops grading the
 * ones the engine never receives. */
const SELF_IDENTIFYING_ENGINES = new Set(["camoufox"]);

// `null` means "whatever the server's default is", which the dashboard cannot
// resolve here - so it is graded as a normal engine rather than guessed at.
const generatesOwnIdentity = (engine) => SELF_IDENTIFYING_ENGINES.has(engine);

// What such an engine still honours, in the words the editor uses for them.
const ENGINE_BUILT_LABEL = "generated by the engine";

/* ---- coherence check (mirrors numera/fingerprint.validate) ------ */
// A GPU is coherent if it's one we offer, or if its graphics API matches the OS
// (Windows → Direct3D, macOS/Linux → desktop OpenGL). The second clause lets a
// profile carry the machine's real GPU after "Auto-adjust" without a false flag.
function webglPlausible(os, renderer) {
  if (!renderer) return true;
  if (GPUS[os]?.includes(renderer)) return true;
  const r = renderer.toLowerCase();
  const d3d = r.includes("direct3d") || r.includes("d3d11");
  const gl = r.includes("opengl");
  if (os === "Windows") return d3d;
  if (os === "macOS") return gl && !d3d;
  if (os === "Linux") return gl && !d3d;
  if (os === "Android") return r.includes("opengl es") || r.includes("adreno") || r.includes("mali");
  return false;
}

function screenPlausible(os, screen) {
  if (!screen || SCREENS[os]?.includes(screen)) return true;
  const [w, h] = screen.split("x").map(Number);
  return w >= 320 && w <= 8000 && h >= 480 && h <= 5000;   // any real display
}

/* Only the signals the chosen engine actually receives are graded.
 *
 * An engine that builds its own identity never sees the GPU string, the screen
 * or the core count, so flagging "GPU doesn't belong to Windows" for a camoufox
 * profile is a contradiction between two fields neither of which reaches the
 * browser. Locale, timezone and the proxy's country still count: those ARE
 * passed through, and a locale that disagrees with its zone is a real leak
 * whichever engine renders it. */
function coherence(p) {
  const issues = [];
  const engineBuilt = generatesOwnIdentity(p.engine);
  if (!engineBuilt && p.webglRenderer && !webglPlausible(p.os, p.webglRenderer))
    issues.push("GPU doesn't belong to " + p.os);
  if (!engineBuilt && p.screen && !screenPlausible(p.os, p.screen))
    issues.push("Screen resolution unusual for " + p.os);
  if (p.locale && p.timezone && LOCALES[p.locale] !== p.timezone)
    issues.push("Timezone doesn't match locale");
  if (!engineBuilt && p.cores && !(Number(p.cores) >= 2 && Number(p.cores) <= 128))
    issues.push("CPU cores atypical for " + p.os);
  if (p.proxy?.country && COUNTRY_LOCALE[p.proxy.country] &&
      COUNTRY_LOCALE[p.proxy.country] !== p.locale)
    issues.push("Locale differs from proxy country (" + p.proxy.country + ")");
  return issues;
}

function uaFor(os, chrome = "128.0.0.0") {
  const tok = { Windows: "Windows NT 10.0; Win64; x64", macOS: "Macintosh; Intel Mac OS X 10_15_7", Linux: "X11; Linux x86_64", Android: "Linux; Android 13; Pixel 7" }[os];
  const mob = os === "Android" ? " Mobile" : "";
  return `Mozilla/5.0 (${tok}) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/${chrome}${mob} Safari/537.36`;
}

// Guess the WebGL vendor string from a renderer's GPU family (mirrors server).
function vendorFor(renderer) {
  const r = (renderer || "").toLowerCase();
  for (const [tok, name] of [["nvidia", "NVIDIA"], ["radeon", "AMD"], ["amd", "AMD"],
    ["intel", "Intel"], ["apple", "Apple"], ["adreno", "Qualcomm"], ["mali", "ARM"]])
    if (r.includes(tok)) return `Google Inc. (${name})`;
  return "Google Inc.";
}

/* A local id for a profile that does not have one from the engine yet.
 * Not demo data: a profile created in live mode needs an id before the POST
 * returns, and React needs a stable key for the row in the meantime. */
let uid = 100;
const nid = () => (++uid).toString(36) + Math.random().toString(36).slice(2, 6);

export { nid, WARMUP_PRESETS, OS_LIST, GPUS, SCREENS, CORES, RAM, LOCALES, COUNTRY_LOCALE, COUNTRIES, TIMEZONES, validTimezone,
 MODES, ENGINE_ORDER, ENGINE_META, CHROME_VERSIONS, webglPlausible, screenPlausible,
 coherence, uaFor, vendorFor, generatesOwnIdentity, ENGINE_BUILT_LABEL };
