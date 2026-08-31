import { Plus, X } from "lucide-react";
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

const COUNTRY_LOCALE = { US: "en-US", GB: "en-GB", UK: "en-GB", PK: "en-PK", IN: "en-IN", DE: "de-DE", FR: "fr-FR", ES: "es-ES", TR: "tr-TR", IT: "it-IT", BE: "nl-BE" };

const MODES = {
  canvas: ["Noise", "Block", "Real"], webrtc: ["Altered", "Disabled", "Real"],
  audio: ["Noise", "Real"], fonts: ["Masked", "Real"], geo: ["Prompt", "Allow", "Block"],
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

function coherence(p) {
  const issues = [];
  if (p.webglRenderer && !webglPlausible(p.os, p.webglRenderer))
    issues.push("GPU doesn't belong to " + p.os);
  if (p.screen && !screenPlausible(p.os, p.screen))
    issues.push("Screen resolution unusual for " + p.os);
  if (p.locale && p.timezone && LOCALES[p.locale] !== p.timezone)
    issues.push("Timezone doesn't match locale");
  if (p.cores && !(Number(p.cores) >= 2 && Number(p.cores) <= 128))
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

export { nid, WARMUP_PRESETS, OS_LIST, GPUS, SCREENS, CORES, RAM, LOCALES, COUNTRY_LOCALE,
 MODES, ENGINE_ORDER, ENGINE_META, CHROME_VERSIONS, webglPlausible, screenPlausible,
 coherence, uaFor, vendorFor };
