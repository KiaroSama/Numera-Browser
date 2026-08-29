import React, { useState, useMemo, useEffect, useRef } from "react";
import { api } from "./api";
import { DARK, LIGHT, T, FONT, makeCss } from "./theme";
import {
  Layers, Globe, FolderTree, Copy, Trash2, Play, Square, Pencil,
  Search, Plus, Filter, ShieldCheck, ShieldAlert, X, Zap, Users,
  Cpu, MonitorSmartphone, Fingerprint, Wifi, WifiOff, Check, Clipboard,
  Settings, Boxes, Activity, MapPin, Clock, Camera, Mic, Blocks,
  RefreshCw, Server, Sparkles, ArrowRight, Star, Circle,
  Cookie, Download, Upload, Flame, Link2, Sun, Moon, Command,
  HardDrive, FolderPlus, Palette, Info, Puzzle,
} from "lucide-react";

/* ================================================================== *
 *  Numera — anti-detect browser profile manager
 *
 *  Numera MANAGES the identity (coherent fingerprint, proxy, session).
 *  A pluggable ENGINE renders it in a real browser. The UI is theme-aware,
 *  dark-first, and connects to the engine API when it's running (live mode),
 *  otherwise falls back to sample data (demo mode).
 * ================================================================== */

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

/* ---- seed data --------------------------------------------------- */
const FOLDERS = ["All profiles", "Facebook Ads", "Amazon Stores", "Crypto", "QA Testing"];
let uid = 100;
// Collision-resistant local id: a counter (unique within a page load) plus a
// random suffix (so ids don't repeat across reloads / other sessions). In live
// mode the server assigns its own id anyway; this is for demo/local state.
const nid = () => (++uid).toString(36) + Math.random().toString(36).slice(2, 6);

const seed = [
  mk("FB-Ads-01", "Facebook Ads", "running", "Windows", ["client-a"], "US"),
  mk("FB-Ads-02", "Facebook Ads", "stopped", "Windows", ["client-a"], "US"),
  mk("AMZ-Store-EU", "Amazon Stores", "running", "macOS", ["eu"], "DE"),
  mk("Crypto-Air-07", "Crypto", "stopped", "Linux", ["airdrop"], null),
  mk("PK-Mobile-01", "QA Testing", "running", "Android", ["mobile", "urdu"], "PK"),
  mk("QA-Sandbox", "QA Testing", "stopped", "Windows", ["internal"], "GB"),
];

function mk(name, folder, status, os, tags, country) {
  const locale = country && COUNTRY_LOCALE[country] ? COUNTRY_LOCALE[country] : "en-US";
  return {
    id: nid(), name, folder, status, os, tags, engine: "playwright",
    browser: "Chrome", browserVersion: "128.0.0.0", userAgent: uaFor(os),
    screen: SCREENS[os][0], cores: CORES[os][1], memory: RAM[os][1],
    webglVendor: os === "macOS" ? "Google Inc. (Apple)" : "Google Inc. (NVIDIA)",
    webglRenderer: GPUS[os][0],
    canvas: "Noise", webrtc: "Altered", audio: "Noise", fonts: "Masked",
    locale, timezone: LOCALES[locale], geo: "Prompt",
    mediaVideo: 1, mediaAudio: 1, dnt: false,
    proxy: country ? { type: "HTTP", host: "res-" + country.toLowerCase() + ".proxy.io", port: "8080", user: "u" + name.length, pass: "•••••", country } : null,
    notes: "", startupUrls: "", extensions: "",
    lastActive: status === "running" ? "now" : ["2h ago", "yesterday", "3d ago"][name.length % 3],
  };
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
const CHROME_VERSIONS = ["125.0.0.0", "126.0.0.0", "127.0.0.0", "128.0.0.0"];

/* ---- toasts ------------------------------------------------------ *
 * A tiny module-level pub/sub so any code — even outside React — can call
 * toast.error(msg) without the app threading a context through everything.  */
let _toastId = 0;
const _toastSubs = new Set();
function _pushToast(kind, message) {
  const t = { id: ++_toastId, kind, message: String(message) };
  _toastSubs.forEach(fn => fn(t));
}
const toast = {
  success: (m) => _pushToast("success", m),
  error: (m) => _pushToast("error", m),
  info: (m) => _pushToast("info", m),
};

function Toaster() {
  const [items, setItems] = useState([]);
  useEffect(() => {
    const on = (t) => {
      setItems(list => [...list, t]);
      setTimeout(() => setItems(list => list.filter(x => x.id !== t.id)), 3800);
    };
    _toastSubs.add(on);
    return () => _toastSubs.delete(on);
  }, []);
  const remove = (id) => setItems(list => list.filter(x => x.id !== id));
  return (
    <div className="ps-toaster">
      {items.map(t => (
        <div key={t.id} className={"ps-toast " + t.kind} onClick={() => remove(t.id)}>
          {t.kind === "success" ? <Check size={15} /> : t.kind === "error" ? <ShieldAlert size={15} /> : <Activity size={15} />}
          <span>{t.message}</span>
        </div>
      ))}
    </div>
  );
}

/* ---- error boundary ---------------------------------------------- *
 * Without one, a single malformed profile that throws while rendering takes the
 * whole React tree down — a white page with no way back. React only catches
 * render errors in a class component, hence the class. Wrap the parts that draw
 * profile data so a bad one degrades to a readable panel and the rest keeps
 * working. `resetKey` clears the error when you navigate somewhere else.       */
export class ErrorBoundary extends React.Component {
  state = { error: null };
  static getDerivedStateFromError(error) { return { error }; }
  componentDidCatch(error, info) { console.error("Numera UI error:", error, info?.componentStack); }
  componentDidUpdate(prev) {
    if (this.state.error && prev.resetKey !== this.props.resetKey) this.setState({ error: null });
  }
  render() {
    if (!this.state.error) return this.props.children;
    const { title, onReset, overlay } = this.props;
    // Colour is inline because at the root the app's <style> may never have
    // mounted — the panel has to be readable without the stylesheet.
    const panel = (
      <div className="ps-empty" style={{ padding: 40, color: T.muted, fontSize: 13 }}>
        <div className="ps-empty-ic"><ShieldAlert size={26} color={T.amber} /></div>
        <div style={{ fontSize: 15, fontWeight: 600, fontFamily: FONT.display, color: T.text }}>
          {title || "This view couldn't be drawn"}
        </div>
        <div style={{ maxWidth: 400, textAlign: "center", lineHeight: 1.6 }}>
          Usually one malformed profile. Try again, or reload if it keeps happening.
        </div>
        <code className="ps-mono" style={{ fontSize: 11, color: T.dim, textAlign: "center" }}>
          {String(this.state.error?.message || this.state.error)}
        </code>
        <button className="ps-btn ghost sm" onClick={() => (onReset ? onReset() : this.setState({ error: null }))}>
          {onReset ? <><X size={14} /> Close</> : <><RefreshCw size={14} /> Try again</>}
        </button>
      </div>
    );
    if (!overlay) return panel;
    return <div className="ps-overlay center"><div className="ps-modal ps-pop">{panel}</div></div>;
  }
}

/* ================================================================== */
export default function App() {
  const [profiles, setProfiles] = useState(seed);
  const [view, setView] = useState("profiles");
  const [folder, setFolder] = useState("All profiles");
  const [query, setQuery] = useState("");
  const [sel, setSel] = useState(new Set());
  const [editor, setEditor] = useState(null);
  const [bulk, setBulk] = useState(false);
  const [sync, setSync] = useState(false);   // "edit all selected" modal
  const [filters, setFilters] = useState({ status: "", engine: "", os: "", proxy: "", tag: "" });
  const [mode, setMode] = useState("connecting");   // connecting | live | demo
  const [engines, setEngines] = useState(null);      // {name: installed} from API
  // New-profile default. null = not known yet (or no engine to ask): profiles are
  // then created without an engine so the server applies its own stored default,
  // instead of us guessing one and having it persisted back as "last engine wins".
  const [defaultEngine, setDefaultEngine] = useState(null);
  const [config, setConfig] = useState(null);        // {root, version, ...} from API
  const [folders, setFolders] = useState(() => FOLDERS.filter(f => f !== "All profiles"));
  const [palette, setPalette] = useState(() => {     // "dark" | "light"
    const t = (typeof localStorage !== "undefined" && localStorage.getItem("ps-theme")) || "dark";
    Object.assign(T, t === "light" ? LIGHT : DARK);  // paint before children render
    if (typeof document !== "undefined") document.documentElement.dataset.theme = t;
    return t;
  });
  const [, forceRepaint] = useState(0);              // bump to re-read T after mutating it
  const [cmdOpen, setCmdOpen] = useState(false);     // ⌘K command palette

  const toggleTheme = () => {
    const next = palette === "light" ? "dark" : "light";
    Object.assign(T, next === "light" ? LIGHT : DARK);
    try { localStorage.setItem("ps-theme", next); } catch { /* private mode */ }
    if (typeof document !== "undefined") document.documentElement.dataset.theme = next;
    setPalette(next); forceRepaint(n => n + 1);
  };

  // ⌘K / Ctrl-K opens the command palette; Esc closes it.
  useEffect(() => {
    const onKey = (e) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") { e.preventDefault(); setCmdOpen(o => !o); }
      else if (e.key === "Escape") setCmdOpen(false);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const live = mode === "live";

  const refresh = async () => setProfiles(await api.list());
  useEffect(() => {
    let cancelled = false;
    (async () => {
      const up = await api.health();
      if (cancelled) return;
      if (up) {
        try {
          await refresh();
          try { setEngines(await api.engines()); } catch { /* optional */ }
          try {
            const cfg = await api.getConfig();
            setConfig(cfg);
            if (cfg?.default_engine) setDefaultEngine(cfg.default_engine);
            if (Array.isArray(cfg?.folders) && cfg.folders.length) setFolders(cfg.folders);
          } catch { /* optional */ }
          setMode("live");
        } catch { setMode("demo"); }
      } else { setMode("demo"); }
    })();
    return () => { cancelled = true; };
  }, []);

  // Keep run status in sync: the browser can be closed by the user directly, so
  // poll the API in live mode and flip the launch button back to "start".
  useEffect(() => {
    if (!live) return;
    let cancelled = false;
    // Poll less often once the list is large — refetching thousands of profiles
    // every few seconds is wasteful, and run-status doesn't change that fast.
    const every = profiles.length > 500 ? 12000 : 3000;
    const id = setInterval(() => {
      if (!cancelled && !editor && !bulk && !sync) refresh().catch(() => {});
    }, every);
    return () => { cancelled = true; clearInterval(id); };
  }, [live, editor, bulk, sync, profiles.length > 500]);

  const filtered = useMemo(() =>
    profiles.filter(p =>
      (folder === "All profiles" || p.folder === folder) &&
      (p.name.toLowerCase().includes(query.toLowerCase()) ||
       p.tags.some(t => t.includes(query.toLowerCase()))) &&
      (!filters.status || p.status === filters.status) &&
      (!filters.engine || (p.engine || "playwright") === filters.engine) &&
      (!filters.os || p.os === filters.os) &&
      (!filters.tag || (p.tags || []).includes(filters.tag)) &&
      (!filters.proxy || (filters.proxy === "with" ? !!p.proxy : !p.proxy))),
    [profiles, folder, query, filters]);

  // Every distinct tag in use, for the filter popover.
  const allTags = useMemo(
    () => [...new Set(profiles.flatMap(p => p.tags || []))].sort(),
    [profiles]);

  // Folders shown everywhere: the persisted/created ones, plus any a profile
  // currently lives in (so an imported "Unfiled" bucket appears on its own).
  const folderList = useMemo(
    () => [...new Set([...folders, ...profiles.map(p => p.folder).filter(Boolean)])],
    [folders, profiles]);

  // Memoised so a 10k-profile list isn't re-scanned on every render / poll tick.
  const { running, proxyCount, coherentCount } = useMemo(() => ({
    running: profiles.filter(p => p.status === "running").length,
    proxyCount: new Set(profiles.filter(p => p.proxy).map(p => p.proxy.host)).size,
    coherentCount: profiles.filter(p => coherence(p).length === 0).length,
  }), [profiles]);

  const toggleRun = async (id) => {
    const p = profiles.find(x => x.id === id);
    if (live) {
      try { await (p?.status === "running" ? api.stop(id) : api.launch(id)); await refresh(); }
      catch (e) { toast.error("Launch failed: " + e.message); }
      return;
    }
    setProfiles(ps => ps.map(x => x.id === id ? { ...x, status: x.status === "running" ? "stopped" : "running", lastActive: "now" } : x));
  };
  const clone = async (p) => {
    const copy = { ...p, id: undefined, name: p.name + "-copy", status: "stopped", lastActive: "never", _new: true };
    if (live) { try { await api.create(copy); await refresh(); } catch (e) { toast.error("Clone failed: " + e.message); } return; }
    setProfiles(ps => [...ps, { ...copy, id: nid() }]);
  };
  const remove = async (id) => {
    if (live) { try { await api.remove(id); await refresh(); } catch (e) { toast.error("Delete failed: " + e.message); } return; }
    setProfiles(ps => ps.filter(p => p.id !== id));
  };
  const save = async (p) => {
    if (p.engine) setDefaultEngine(p.engine);   // "last engine wins" for the next new profile
    if (live) {
      try { const exists = profiles.some(x => x.id === p.id) && !p._new; await (exists ? api.update(p) : api.create({ ...p, id: undefined })); await refresh(); }
      catch (e) { toast.error("Save failed: " + e.message); }
      return;
    }
    setProfiles(ps => ps.some(x => x.id === p.id) ? ps.map(x => x.id === p.id ? p : x) : [...ps, p]);
  };

  // ---- folder management (used by the Folders tab) ----
  const createFolder = async (name) => {
    name = (name || "").trim();
    if (!name || folderList.includes(name)) { if (name) toast.info(`Folder "${name}" already exists.`); return; }
    setFolders(f => [...f, name]);
    if (live) { try { await api.createFolder(name); } catch (e) { toast.error("Create folder failed: " + e.message); } }
    else toast.success(`Folder "${name}" created.`);
  };
  const renameFolder = async (from, to) => {
    to = (to || "").trim();
    if (!to || to === from) return;
    if (folderList.includes(to)) { toast.info(`Folder "${to}" already exists.`); return; }
    setFolders(f => f.map(x => x === from ? to : x));
    setProfiles(ps => ps.map(p => p.folder === from ? { ...p, folder: to } : p));
    if (folder === from) setFolder(to);
    if (live) { try { const r = await api.renameFolder(from, to); if (r?.folders) setFolders(r.folders); await refresh(); } catch (e) { toast.error("Rename failed: " + e.message); } }
  };
  const deleteFolder = async (name) => {
    setFolders(f => f.filter(x => x !== name));
    setProfiles(ps => ps.map(p => p.folder === name ? { ...p, folder: "Unfiled" } : p));
    if (folder === name) setFolder("All profiles");
    if (live) { try { const r = await api.deleteFolder(name); if (r?.folders) setFolders(r.folders); await refresh(); } catch (e) { toast.error("Delete folder failed: " + e.message); } }
    else toast.success(`Folder "${name}" removed — its profiles moved to Unfiled.`);
  };

  // Change the engine used for every new profile (Settings tab).
  const changeDefaultEngine = async (name) => {
    setDefaultEngine(name);
    if (live) { try { await api.setDefaultEngine(name); toast.success(`New profiles will use ${ENGINE_META[name]?.label || name}.`); } catch (e) { toast.error("Couldn't save default engine: " + e.message); } }
    else toast.info("Connect the engine (numera serve) to save this setting.");
  };

  // Import profiles exported from GoLogin / AdsPower / Multilogin.
  const importProfiles = () => {
    if (!live) { toast.info("Start the engine (numera serve) to import profiles."); return; }
    const inp = document.createElement("input");
    inp.type = "file"; inp.accept = ".json,application/json";
    inp.onchange = async () => {
      const f = inp.files?.[0];
      if (!f) return;
      try {
        const res = await api.importProfiles(await f.text());
        await refresh();
        const flagged = (res.results || []).filter(r => r.issues?.length).length;
        toast.success(`Imported ${res.imported} profile${res.imported === 1 ? "" : "s"}.`
          + (flagged ? ` ${flagged} flagged for review — open them to see the coherence notes.` : ""));
      } catch (e) { toast.error("Import failed: " + e.message); }
    };
    inp.click();
  };

  const toggleSel = (id) => setSel(s => { const n = new Set(s); n.has(id) ? n.delete(id) : n.add(id); return n; });
  const allSel = filtered.length > 0 && filtered.every(p => sel.has(p.id));

  // .ps-root is styled from tokens rather than T: the app root is the one
  // surface that must follow a theme switch even if a re-render is skipped,
  // because everything else is painted on top of it.
  return (
    <div className="ps-root">
      <style>{makeCss()}</style>
      <div className="ps-shell">
        {/* ---- Sidebar ---- */}
        <aside className="ps-side">
          <div className="ps-brand">
            <div className="ps-logo"><span className="ps-logo-glow" /><Fingerprint size={19} color="#fff" strokeWidth={2.2} /></div>
            <div>
              <div className="ps-brand-name">Numera</div>
              <div className="ps-brand-sub">Studio</div>
            </div>
          </div>

          <nav className="ps-nav">
            {[
              ["profiles", Layers, "Profiles"],
              ["health", Activity, "Health"],
              ["engines", Blocks, "Engines"],
              ["extensions", Puzzle, "Extensions"],
              ["proxies", Globe, "Proxies"],
              ["folders", FolderTree, "Folders"],
              ["automation", Zap, "Automation"],
              ["team", Users, "Team"],
              ["settings", Settings, "Settings"],
            ].map(([k, Ic, label]) => (
              <button key={k} onClick={() => setView(k)} className={"ps-navitem" + (view === k ? " active" : "")}>
                <Ic size={16} /> <span>{label}</span>
                {k === "engines" && <span className="ps-nav-tag">new</span>}
              </button>
            ))}
          </nav>

          <div className="ps-folders">
            <div className="ps-folders-h">
              <span style={{ flex: 1 }}>Folders</span>
              <button className="ps-folders-manage" title="Manage folders" onClick={() => setView("folders")}>Manage</button>
            </div>
            {["All profiles", ...folderList].map(f => (
              <button key={f} onClick={() => { setView("profiles"); setFolder(f); }}
                className={"ps-folder" + (folder === f && view === "profiles" ? " active" : "")}>
                <FolderTree size={13} />
                <span style={{ flex: 1, textAlign: "left" }}>{f}</span>
                <span className="ps-count">{f === "All profiles" ? profiles.length : profiles.filter(p => p.folder === f).length}</span>
              </button>
            ))}
          </div>

          <div className={"ps-side-foot " + mode} title={live ? "Connected to the engine — launching opens a real browser." : "Engine API not found — sample data only. Start it with: numera serve"}>
            <span className="ps-foot-dot" />
            <span>{mode === "connecting" ? "Connecting…" : live ? "Engine online" : "Demo mode"}</span>
            <span className="ps-foot-ver">v0.1.0</span>
          </div>
        </aside>

        {/* ---- Main ---- */}
        <main className="ps-main">
          <div className="ps-stats">
            <Stat icon={<Boxes size={16} />} label="Profiles" value={profiles.length} />
            <Stat icon={<Activity size={16} />} label="Running" value={running} accent={T.mint} pulse={running > 0} />
            <Stat icon={<Server size={16} />} label="Proxies" value={proxyCount} accent={T.lilac} />
            <Stat icon={<ShieldCheck size={16} />} label="Coherent"
              value={coherentCount + "/" + profiles.length} accent={T.violet} />
            <div style={{ flex: 1 }} />
            <button className="ps-btn ghost" onClick={importProfiles}><Download size={14} /> Import</button>
            <button className="ps-btn ghost" onClick={() => setBulk(true)}><Boxes size={14} /> Bulk create</button>
            <button className="ps-btn primary" onClick={() => setEditor(newProfile(folder, defaultEngine))}><Plus size={15} /> New profile</button>
            <button className="ps-iconbtn" title="Command palette (⌘K / Ctrl-K)" onClick={() => setCmdOpen(true)}><Command size={15} /></button>
            <button className="ps-iconbtn" title={palette === "light" ? "Switch to dark" : "Switch to light"} onClick={toggleTheme}>
              {palette === "light" ? <Moon size={15} /> : <Sun size={15} />}
            </button>
          </div>

          {/* Keyed on the view so switching tabs clears a failed one. The sidebar
              and toolbar live outside, so navigation survives a bad profile. */}
          <ErrorBoundary resetKey={view}>
          {view === "profiles" && (
            <ProfilesView {...{ filtered, query, setQuery, folder, sel, setSel, toggleSel, allSel, toggleRun, clone, remove, setEditor, setProfiles, setSync, filters, setFilters, allTags, live, refresh, defaultEngine }} />
          )}
          {view === "health" && <HealthView profiles={profiles} live={live} refresh={refresh} setEditor={setEditor} />}
          {view === "engines" && <EnginesView engines={engines} live={live} />}
          {view === "extensions" && (
            <ExtensionsView config={config} live={live} profiles={profiles} refresh={refresh}
                            onSaved={async () => { try { setConfig(await api.getConfig()); } catch { /* offline */ } }} />
          )}
          {view === "proxies" && <ProxiesView profiles={profiles} live={live} refresh={refresh} />}
          {view === "automation" && <AutomationView profiles={profiles} live={live} refresh={refresh} />}
          {view === "folders" && (
            <FoldersView folders={folderList} profiles={profiles}
              onOpen={(f) => { setFolder(f); setView("profiles"); }}
              onCreate={createFolder} onRename={renameFolder} onDelete={deleteFolder} />
          )}
          {view === "settings" && (
            <SettingsView live={live} config={config} engines={engines}
              defaultEngine={defaultEngine} onChangeEngine={changeDefaultEngine}
              palette={palette} toggleTheme={toggleTheme}
              counts={{ profiles: profiles.length, folders: folderList.length, proxies: proxyCount }} />
          )}
          {!["profiles", "health", "engines", "extensions", "proxies", "automation", "folders", "settings"].includes(view) && <Placeholder view={view} />}
          </ErrorBoundary>
        </main>
      </div>

      {editor && (
        <ErrorBoundary overlay title="This profile couldn't be opened" onReset={() => setEditor(null)}>
          <Editor profile={editor} live={live} folders={folderList} onClose={() => setEditor(null)} onSave={(p) => { save(p); setEditor(null); }} />
        </ErrorBoundary>
      )}
      {sync && <SyncModal ids={[...sel]} folders={folderList} onClose={() => setSync(false)} onApply={async (patch) => {
        const ids = [...sel];
        if (live) {
          try { await api.bulkUpdate(ids, patch); await refresh(); }
          catch (e) { toast.error("Bulk edit failed: " + e.message); }
        } else {
          const { addTags, ...fields } = patch;
          setProfiles(ps => ps.map(p => !ids.includes(p.id) ? p : {
            ...p, ...fields,
            timezone: patch.locale ? LOCALES[patch.locale] : p.timezone,
            tags: addTags ? [...new Set([...p.tags, ...addTags])] : p.tags,
          }));
        }
        if (patch.engine) setDefaultEngine(patch.engine);
        setSel(new Set());
        setSync(false);
      }} />}
      {bulk && <BulkModal onClose={() => setBulk(false)} folder={folder} defaultEngine={defaultEngine}
        onCreate={async (list, onProgress) => {
          if (live) {
            const CHUNK = 500;   // one request per chunk keeps 10k creates snappy
            for (let i = 0; i < list.length; i += CHUNK) {
              await api.batchCreate(list.slice(i, i + CHUNK));
              onProgress?.(Math.min(list.length, i + CHUNK), list.length);
            }
            await refresh();
          } else {
            setProfiles(ps => [...ps, ...list]);
            onProgress?.(list.length, list.length);
          }
        }} />}
      {cmdOpen && <CommandPalette onClose={() => setCmdOpen(false)} profiles={profiles}
        setView={setView} setEditor={setEditor} toggleRun={toggleRun} toggleTheme={toggleTheme}
        newProfile={() => setEditor(newProfile(folder, defaultEngine))} />}
      <Toaster />
    </div>
  );
}

/* ---- Profiles view ----------------------------------------------- */
function ProfilesView({ filtered, query, setQuery, folder, sel, setSel, toggleSel, allSel, toggleRun, clone, remove, setEditor, setProfiles, setSync, filters, setFilters, allTags, live, refresh, defaultEngine }) {
  const [filterOpen, setFilterOpen] = useState(false);
  const activeFilters = Object.values(filters).filter(Boolean).length;
  const setFilter = (key, value) => setFilters(f => ({ ...f, [key]: f[key] === value ? "" : value }));
  const clearFilters = () => setFilters({ status: "", engine: "", os: "", proxy: "", tag: "" });

  // Warm up every selected profile (live only — it opens real headless browsers).
  const warmSelected = async () => {
    const ids = [...sel];
    if (!live) { toast.info("Warm-up runs against the live engine — start `numera serve` first."); return; }
    setSel(new Set());
    await Promise.all(ids.map(id => api.warmup(id, 5, "general").catch(() => {})));
    await refresh();
  };

  // Export the selected profiles as one JSON file (config: fingerprint, proxy,
  // tags — not the browser session; use `numera backup` for a full session).
  const exportSelected = () => {
    const chosen = filtered.filter(p => sel.has(p.id));
    const blob = new Blob([JSON.stringify(chosen, null, 2)], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url; a.download = `numera-profiles-${chosen.length}.json`; a.click();
    URL.revokeObjectURL(url);
  };

  // Pagination keeps the grid responsive when there are thousands of profiles.
  const PAGE = 100;
  const [page, setPage] = useState(0);
  useEffect(() => { setPage(0); }, [query, folder, filters.status, filters.engine, filters.os, filters.proxy]);
  const pageCount = Math.max(1, Math.ceil(filtered.length / PAGE));
  const cur = Math.min(page, pageCount - 1);
  const rows = filtered.slice(cur * PAGE, cur * PAGE + PAGE);

  const FilterRow = ({ label, options, value, onPick }) => (
    <div className="ps-filter-row">
      <span className="ps-filter-label">{label}</span>
      <div className="ps-filter-opts">
        {options.map(([val, text]) => (
          <button key={val} className={"ps-chip" + (value === val ? " on" : "")}
            onClick={() => onPick(val)}>{text}</button>
        ))}
      </div>
    </div>
  );

  return (
    <>
      <div className="ps-toolbar">
        <div className="ps-search">
          <Search size={15} color={T.dim} />
          <input placeholder="Search profiles, tags…" value={query} onChange={e => setQuery(e.target.value)} />
        </div>
        <div className="ps-crumb"><FolderTree size={13} /> {folder}</div>
        <div style={{ flex: 1 }} />
        {sel.size > 0 && (
          <div className="ps-bulkbar">
            <span>{sel.size} selected</span>
            <button onClick={() => { sel.forEach(toggleRun); setSel(new Set()); }}><Play size={13} /> Launch</button>
            <button onClick={warmSelected}><Flame size={13} /> Warm up</button>
            <button onClick={() => setSync(true)}><Copy size={13} /> Edit all</button>
            <button onClick={exportSelected}><Download size={13} /> Export</button>
            <button className="danger" onClick={async () => { const ids = [...sel]; setSel(new Set()); if (live) { for (const id of ids) { try { await api.remove(id); } catch {} } await refresh(); } else { setProfiles(ps => ps.filter(p => !ids.includes(p.id))); } }}><Trash2 size={13} /> Delete</button>
          </div>
        )}
        <div className="ps-filter-wrap">
          <button className={"ps-btn ghost sm" + (activeFilters ? " active" : "")}
            onClick={() => setFilterOpen(o => !o)}>
            <Filter size={13} /> Filter{activeFilters ? ` · ${activeFilters}` : ""}
          </button>
          {filterOpen && (
            <>
              <div className="ps-filter-scrim" onClick={() => setFilterOpen(false)} />
              <div className="ps-filter-pop">
                <div className="ps-filter-head">
                  <span>Filter profiles</span>
                  {activeFilters > 0 && <button className="ps-filter-clear" onClick={clearFilters}>Clear all</button>}
                </div>
                <FilterRow label="Status" value={filters.status} onPick={v => setFilter("status", v)}
                  options={[["running", "Running"], ["stopped", "Stopped"]]} />
                <FilterRow label="Engine" value={filters.engine} onPick={v => setFilter("engine", v)}
                  options={ENGINE_ORDER.map(en => [en, ENGINE_META[en].label])} />
                <FilterRow label="OS" value={filters.os} onPick={v => setFilter("os", v)}
                  options={OS_LIST.map(o => [o, o])} />
                <FilterRow label="Proxy" value={filters.proxy} onPick={v => setFilter("proxy", v)}
                  options={[["with", "With proxy"], ["without", "Direct"]]} />
                {allTags.length > 0 && (
                  <FilterRow label="Tag" value={filters.tag} onPick={v => setFilter("tag", v)}
                    options={allTags.map(t => [t, t])} />
                )}
              </div>
            </>
          )}
        </div>
      </div>

      <div className="ps-scroll">
        {filtered.length === 0 ? (
          (activeFilters || query) ? (
            <div className="ps-empty">
              <Filter size={28} color={T.dim} />
              <div>No profiles match your {activeFilters ? "filters" : "search"}.</div>
              <button className="ps-btn ghost sm" onClick={() => { clearFilters(); setQuery(""); }}>
                <X size={14} /> Clear {activeFilters ? "filters" : "search"}
              </button>
            </div>
          ) : (
          <div className="ps-empty">
            <Layers size={28} color={T.dim} />
            <div>No profiles here yet.</div>
            <button className="ps-btn primary sm" onClick={() => setEditor(newProfile(folder, defaultEngine))}><Plus size={14} /> Create one</button>
          </div>
          )
        ) : (
          <table className="ps-table">
            <thead>
              <tr>
                <th style={{ width: 34 }}>
                  <input type="checkbox" checked={allSel} onChange={() => setSel(allSel ? new Set() : new Set(filtered.map(p => p.id)))} />
                </th>
                <th>Profile</th><th>Environment</th><th>Engine</th><th>Proxy</th>
                <th>Coherence</th><th>Last active</th><th style={{ width: 150, textAlign: "right" }}>Actions</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((p, i) => {
                const iss = coherence(p);
                return (
                  <tr key={p.id} className={"ps-rise" + (sel.has(p.id) ? " sel" : "")} style={{ animationDelay: `${Math.min(i, 12) * 28}ms` }}>
                    <td><input type="checkbox" checked={sel.has(p.id)} onChange={() => toggleSel(p.id)} /></td>
                    <td>
                      <div className="ps-name">
                        <span className={"ps-status " + p.status} />
                        <div>
                          <div className="ps-pname">{p.name}</div>
                          <div className="ps-tags">{p.tags.map(t => (
                            <span key={t} className={"ps-tag clickable" + (filters.tag === t ? " on" : "")}
                              title={filters.tag === t ? "Clear tag filter" : `Filter by "${t}"`}
                              onClick={() => setFilter("tag", t)}>{t}</span>
                          ))}</div>
                        </div>
                      </div>
                    </td>
                    <td>
                      <div className="ps-env">
                        <span className="ps-os">{p.os}</span>
                        <span className="ps-mono">{p.browser} {p.browserVersion.split(".")[0]}</span>
                        <span className="ps-mono dim">{p.screen}</span>
                      </div>
                    </td>
                    <td><span className="ps-engine-chip">{ENGINE_META[p.engine]?.label || p.engine || "Playwright"}</span></td>
                    <td>
                      {p.proxy
                        ? <div className="ps-proxy"><Wifi size={13} color={T.mint} /><span className="ps-mono">{p.proxy.country}</span><span className="ps-mono dim">{p.proxy.type}</span></div>
                        : <div className="ps-proxy off"><WifiOff size={13} color={T.dim} /> <span className="dim">Direct</span></div>}
                    </td>
                    <td>
                      {iss.length === 0
                        ? <span className="ps-pill ok"><ShieldCheck size={12} /> Coherent</span>
                        : <span className="ps-pill warn" title={iss.join("\n")}><ShieldAlert size={12} /> {iss.length} issue{iss.length > 1 ? "s" : ""}</span>}
                    </td>
                    <td><span className="ps-mono dim">{p.lastActive}</span></td>
                    <td>
                      <div className="ps-actions">
                        <button title={p.status === "running" ? "Stop" : "Launch"} className={p.status === "running" ? "stop" : "run"} onClick={() => toggleRun(p.id)}>
                          {p.status === "running" ? <Square size={13} /> : <Play size={13} />}
                        </button>
                        <button title="Edit" onClick={() => setEditor({ ...p })}><Pencil size={13} /></button>
                        <button title="Clone" onClick={() => clone(p)}><Copy size={13} /></button>
                        <button title="Delete" className="del" onClick={() => remove(p.id)}><Trash2 size={13} /></button>
                      </div>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
        {pageCount > 1 && (
          <div className="ps-pager">
            <button className="ps-btn ghost sm" disabled={cur === 0} onClick={() => setPage(0)}>« First</button>
            <button className="ps-btn ghost sm" disabled={cur === 0} onClick={() => setPage(cur - 1)}>‹ Prev</button>
            <span className="ps-pager-info">
              Page <b>{cur + 1}</b> of {pageCount} · <b>{filtered.length.toLocaleString()}</b> profiles
            </span>
            <button className="ps-btn ghost sm" disabled={cur >= pageCount - 1} onClick={() => setPage(cur + 1)}>Next ›</button>
            <button className="ps-btn ghost sm" disabled={cur >= pageCount - 1} onClick={() => setPage(pageCount - 1)}>Last »</button>
          </div>
        )}
      </div>
    </>
  );
}

/* ---- Engines view (the signature: pluggable stealth backends) ---- */
/* ---- Extensions view ---------------------------------------------
 * The default set, backed by `default_extensions` in /api/config — which
 * already existed. This page is the interface to it, not a new mechanism.
 *
 * Adding here seeds new profiles; "Apply to all" pushes the set onto the
 * profiles that already exist. That second step **merges** rather than
 * replaces: a profile keeps whatever extensions it already had. Doing it as a
 * bulk patch would have been one request instead of N, but a patch overwrites
 * the field, so every per-profile extension would have been silently deleted —
 * exactly the kind of quiet data loss that is worth N requests to avoid.
 *
 * One limit is stated on the page rather than hidden: Firefox-family engines
 * cannot load unpacked Chromium extensions at all.                            */
function ExtensionsView({ config, live, onSaved, profiles, refresh }) {
  const [items, setItems] = useState(() => config?.default_extensions || []);
  const [draft, setDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState("");
  const [applied, setApplied] = useState(null);   // {changed, skipped} after a run
  const [installed, setInstalled] = useState(""); // result of a Web Store install

  // Re-sync when the engine hands us a config later than our first render.
  useEffect(() => { setItems(config?.default_extensions || []); }, [config]);

  const save = async (next) => {
    setErr(""); setItems(next);          // optimistic: the list is the source of truth on screen
    if (!live) return;
    setBusy(true);
    try {
      await api.setDefaultExtensions(next);
      onSaved?.();
    } catch (e) {
      setErr(String(e?.message || e));
      setItems(config?.default_extensions || []);   // put the real state back
    } finally { setBusy(false); }
  };

  const add = async () => {
    const entry = draft.trim();
    if (!entry) return;

    // A Web Store link is not a path. The engine downloads and unpacks it, and
    // the resulting folder joins the list — Chromium cannot install from the
    // store itself once automation is enabled, which is why this round-trips.
    if (/^https?:\/\//i.test(entry)) {
      if (!live) { setErr("Installing from a link needs the engine running."); return; }
      setErr(""); setInstalled(""); setBusy(true);
      try {
        const r = await api.installExtension(entry);
        setItems(r.default_extensions || items);
        setDraft("");
        setInstalled(`${r.reused ? "Already installed" : "Installed"}: ${r.name}`);
        onSaved?.();
      } catch (e) {
        setErr(String(e?.message || e));
      } finally { setBusy(false); }
      return;
    }

    if (items.includes(entry)) { setErr("That folder is already in the list."); return; }
    setDraft("");
    save([...items, entry]);
  };

  // Push the default set onto every existing profile, merging into whatever
  // each one already carries. A profile is only written when it would actually
  // change, so re-running this is cheap and the count means something.
  const applyToAll = async () => {
    setErr(""); setApplied(null);
    if (!live || items.length === 0) return;
    setBusy(true);
    let changed = 0, skipped = 0, failed = 0;
    try {
      const all = await api.list();
      for (const p of all) {
        const have = (p.extensions || "").split("\n").map(x => x.trim()).filter(Boolean);
        const missing = items.filter(x => !have.includes(x));
        if (missing.length === 0) { skipped++; continue; }
        try {
          await api.update({ ...p, extensions: [...have, ...missing].join("\n") });
          changed++;
        } catch { failed++; }
      }
      setApplied({ changed, skipped, failed });
      await refresh?.();
    } catch (e) {
      setErr(String(e?.message || e));
    } finally { setBusy(false); }
  };

  return (
    <div className="ps-scroll">
      <div className="ps-eng-hero ps-rise">
        <div className="ps-eng-hero-badge"><Puzzle size={13} /> Extensions</div>
        <h1 className="ps-eng-title">One set of extensions,<br /><span className="grad">on every new profile.</span></h1>
        <p className="ps-eng-lead">
          Numera loads unpacked Chromium extensions from folders on this machine. Anything listed
          here is added to each profile you create from now on — from the dashboard, the CLI or a
          bulk run.
        </p>
      </div>

      <SectionHead icon={<Puzzle size={13} />} text="Default set"
                   note="Seeds new profiles; apply to existing ones below" />

      <div className="ps-ext-add">
        <input
          className="ps-in"
          placeholder="Chrome Web Store link, or C:\path\to\unpacked-extension"
          value={draft}
          onChange={e => { setDraft(e.target.value); setErr(""); }}
          onKeyDown={e => { if (e.key === "Enter") add(); }}
          aria-label="Chrome Web Store link or unpacked extension folder"
        />
        <button className="ps-btn primary" onClick={add} disabled={busy || !draft.trim()}>
          {busy ? <><RefreshCw size={14} /> Working…</>
                : /^https?:\/\//i.test(draft.trim()) ? <><Download size={14} /> Install</>
                : <><Plus size={14} /> Add</>}
        </button>
      </div>
      {err && <div className="ps-ext-err"><ShieldAlert size={13} /> {err}</div>}
      {installed && <div className="ps-ext-done"><Check size={13} /> {installed}</div>}

      {items.length === 0 ? (
        <div className="ps-empty">
          <div className="ps-empty-ic"><Puzzle size={26} color={T.dim} /></div>
          <div>No default extensions yet.</div>
          <div className="ps-hint">Add a folder above and every profile you create afterwards will load it.</div>
        </div>
      ) : (
        <div className="ps-ext-list">
          {items.map((path, i) => (
            <div className="ps-ext-row ps-rise" key={path} style={{ animationDelay: `${i * 45}ms` }}>
              <span className="ps-ext-ic"><Puzzle size={15} color={T.violet} /></span>
              <code className="ps-mono ps-ext-path" title={path}>{path}</code>
              <button className="ps-iconbtn" title={`Remove ${path}`} disabled={busy}
                      onClick={() => save(items.filter(p => p !== path))}>
                <Trash2 size={14} />
              </button>
            </div>
          ))}
        </div>
      )}

      {items.length > 0 && (
        <div className="ps-ext-apply">
          <div>
            <div className="ps-ext-apply-t">Apply to existing profiles</div>
            <div className="ps-hint">
              Adds anything missing to all {profiles?.length ?? 0} profile(s). Extensions a profile
              already has are kept — nothing is removed.
            </div>
          </div>
          <button className="ps-btn" onClick={applyToAll} disabled={busy || !live}>
            <Upload size={14} /> {busy ? "Applying…" : "Apply to all"}
          </button>
        </div>
      )}
      {applied && (
        <div className="ps-ext-done">
          <Check size={13} /> Updated {applied.changed}, already had them {applied.skipped}
          {applied.failed ? `, failed ${applied.failed}` : ""}.
        </div>
      )}

      <div className="ps-eng-note">
        <Info size={14} color={T.violet} />
        <span>
          Chromium extensions do not load in the Firefox-family engines, so a profile on
          <code> camoufox</code> ignores this list whatever you do here. The same set is applied by
          <code> numera apply --all --extension &lt;folder&gt;</code> from the CLI.
        </span>
      </div>

      {!live && (
        <div className="ps-eng-note">
          <WifiOff size={14} color={T.amber} />
          <span>Demo mode — the engine is not running, so changes here are not saved.</span>
        </div>
      )}
    </div>
  );
}

function EnginesView({ engines, live }) {
  const recommended = ENGINE_ORDER.filter(k => ENGINE_META[k].recommended);
  const others = ENGINE_ORDER.filter(k => !ENGINE_META[k].recommended);
  return (
    <div className="ps-scroll">
      <div className="ps-eng-hero ps-rise">
        <div className="ps-eng-hero-badge"><Blocks size={13} /> Pluggable engines</div>
        <h1 className="ps-eng-title">Numera manages the identity.<br /><span className="grad">The engine renders it.</span></h1>
        <p className="ps-eng-lead">
          Every profile carries a coherent fingerprint, a proxy and a session — that part is Numera's job.
          Plug in the stealth backend that matches your target. Install it, pick it per profile, launch.
        </p>
        <div className="ps-eng-flow">
          <span className="ps-flow-node"><Fingerprint size={14} /> Identity</span>
          <ArrowRight size={15} color={T.dim} />
          <span className="ps-flow-node"><Blocks size={14} /> Engine</span>
          <ArrowRight size={15} color={T.dim} />
          <span className="ps-flow-node"><MonitorSmartphone size={14} /> Real browser</span>
        </div>
      </div>

      <SectionHead icon={<Star size={13} />} text="Recommended" note="Strongest against modern detection" />
      <div className="ps-eng-grid">
        {recommended.map((k, i) => <EngineCard key={k} id={k} status={engines?.[k]} live={live} delay={i * 60} featured />)}
      </div>

      <SectionHead icon={<Blocks size={13} />} text="Also available" />
      <div className="ps-eng-grid">
        {others.map((k, i) => <EngineCard key={k} id={k} status={engines?.[k]} live={live} delay={i * 60} />)}
      </div>

      <div className="ps-eng-note">
        <Sparkles size={14} color={T.violet} />
        <span>Add your own backend by registering it in <code>engine/numera/drivers.py</code> — one function, one decorator.</span>
      </div>
    </div>
  );
}

function EngineCard({ id, status, live, delay, featured }) {
  const m = ENGINE_META[id];
  const [copied, setCopied] = useState(false);
  const copy = () => { navigator.clipboard?.writeText(m.install); setCopied(true); setTimeout(() => setCopied(false), 1400); };
  // status: true=installed, false=not, undefined=unknown (demo / builtin)
  const state = m.builtin ? "builtin" : status === true ? "ready" : status === false ? "missing" : "unknown";
  return (
    <div className={"ps-eng-card ps-rise" + (featured ? " featured" : "")} style={{ animationDelay: `${delay}ms` }}>
      <div className="ps-eng-card-top">
        <div>
          <div className="ps-eng-name">{m.label} {m.recommended && <span className="ps-rec"><Star size={10} /> Recommended</span>}</div>
          <div className="ps-eng-kind">{m.kind}</div>
        </div>
        <StatusDot state={state} />
      </div>
      <StrengthMeter value={m.strength} />
      <p className="ps-eng-desc">{m.desc}</p>
      {m.install ? (
        <div className="ps-eng-install">
          <code>{m.install}</code>
          <button onClick={copy} title="Copy install command">{copied ? <Check size={13} color={T.mint} /> : <Clipboard size={13} />}</button>
        </div>
      ) : (
        <div className="ps-eng-builtin"><Check size={13} color={T.mint} /> Ships with Numera — nothing to install.</div>
      )}
      {m.docs && <a className="ps-eng-docs" href={m.docs} target="_blank" rel="noreferrer">Setup guide <ArrowRight size={12} /></a>}
    </div>
  );
}

function StatusDot({ state }) {
  const map = {
    builtin: [T.mint, "Built-in"], ready: [T.mint, "Installed"],
    missing: [T.dim, "Not installed"], unknown: [T.violet, "Available"],
  };
  const [c, label] = map[state];
  return <span className="ps-eng-status" style={{ color: c }}><span className="ps-eng-status-dot" style={{ background: c }} />{label}</span>;
}

function StrengthMeter({ value }) {
  return (
    <div className="ps-strength" title={`Stealth strength ${value}/4`}>
      <span className="ps-strength-label">Stealth</span>
      <div className="ps-strength-bars">
        {[1, 2, 3, 4].map(n => <span key={n} className={"ps-sb" + (n <= value ? " on" : "")} />)}
      </div>
    </div>
  );
}

/* ---- small pieces ------------------------------------------------ */
function Stat({ icon, label, value, accent, pulse }) {
  return (
    <div className="ps-stat">
      <div className={"ps-stat-ic" + (pulse ? " pulse" : "")} style={{ color: accent || T.muted }}>{icon}</div>
      <div>
        <div className="ps-stat-val">{value}</div>
        <div className="ps-stat-label">{label}</div>
      </div>
    </div>
  );
}

// `engine` is null when we have no default from the server: a null engine lets
// the server apply its own stored default instead of us guessing one (B7).
function newProfile(folder, defaultEngine) {
  const os = "Windows";
  return {
    id: nid(), name: "", folder: folder === "All profiles" ? "Facebook Ads" : folder,
    status: "stopped", os, tags: [], engine: defaultEngine || null, browser: "Chrome", browserVersion: "128.0.0.0",
    userAgent: uaFor(os), screen: SCREENS[os][0], cores: CORES[os][1], memory: RAM[os][1],
    webglVendor: "Google Inc. (NVIDIA)", webglRenderer: GPUS[os][0],
    canvas: "Noise", webrtc: "Altered", audio: "Noise", fonts: "Masked",
    locale: "en-US", timezone: "America/New_York", geo: "Prompt",
    mediaVideo: 1, mediaAudio: 1, dnt: false, proxy: null, notes: "", startupUrls: "", extensions: "",
    lastActive: "never", _new: true,
  };
}

/* ---- Probe result panel (proxy test / trust check) ---------------- */
function ProbePanel({ probe, kind }) {
  if (!probe || probe.kind !== kind) return null;
  if (probe.state === "running")
    return <div className="ps-hint"><RefreshCw size={12} className="ps-spin" /> Opening the profile in the background — this takes a few seconds.</div>;
  if (probe.state === "error")
    return <div className="ps-hint"><ShieldAlert size={12} /> {probe.data}</div>;

  const d = probe.data || {};
  const checks = d.checks || [];
  const gColor = (s) => (s >= 95 ? T.mint : s >= 70 ? T.amber : T.red);
  return (
    <div className="ps-probe">
      {d.before && d.after && (
        <div className="ps-probe-head">
          <span className="ps-probe-score" style={{ color: gColor(d.before.score) }}>{d.before.grade} {d.before.score}%</span>
          <ArrowRight size={14} color={T.dim} />
          <span className="ps-probe-score" style={{ color: gColor(d.after.score) }}>{d.after.grade} {d.after.score}%</span>
          <span>{d.after.passed}/{d.after.total} checks passed</span>
        </div>
      )}
      {d.changed && (
        <div className="ps-check ok">
          <Sparkles size={12} />
          <span>{d.changed.length
            ? <>Aligned <em>{d.changed.join(", ")}</em> to the real browser{d.os ? ` · now reads as ${d.os}` : ""}.</>
            : "Already matched the browser — nothing to change."}</span>
        </div>
      )}
      {d.score !== undefined && (
        <div className="ps-probe-head">
          <span className="ps-probe-score" style={{ color: d.score >= 95 ? T.mint : d.score >= 70 ? T.amber : T.red }}>
            {d.score}%
          </span>
          <span>grade {d.grade} · {d.passed}/{d.total} checks passed</span>
        </div>
      )}
      {d.ip && (
        <div className="ps-probe-head">
          <span className="ps-probe-score" style={{ color: T.mint, fontSize: 15 }}>{d.ip}</span>
          <span>{[d.city, d.country].filter(Boolean).join(", ")}{d.latencyMs ? ` · ${d.latencyMs} ms` : ""}</span>
        </div>
      )}
      {(d.ja3Hash || d.ja4) && (
        <div className="ps-probe-head" style={{ display: "block" }}>
          {d.ja3Hash && <div className="ps-mono" style={{ fontSize: 11 }}>JA3 {d.ja3Hash}</div>}
          {d.ja4 && <div className="ps-mono" style={{ fontSize: 11 }}>JA4 {d.ja4}</div>}
        </div>
      )}
      {kind === "dns" && d.countries && (
        <div className="ps-probe-head">
          <span className="ps-probe-score" style={{ color: T.lilac, fontSize: 14 }}>
            {d.resolvers?.length || 0} resolver{(d.resolvers?.length || 0) === 1 ? "" : "s"}
          </span>
          <span>{d.countries.join(", ") || "unknown"}</span>
        </div>
      )}
      {d.error && <div className="ps-check">{d.error}</div>}
      {checks.map((c, i) => (
        <div key={i} className={"ps-check" + (c.ok ? " ok" : "")}>
          {c.ok ? <Check size={12} /> : <X size={12} />}
          <span>{c.name}{!c.ok && c.detail ? <em> — {c.detail}</em> : null}</span>
        </div>
      ))}
    </div>
  );
}

/* ---- Fingerprint editor ------------------------------------------ */
function Editor({ profile, live, folders = [], onClose, onSave }) {
  const [p, setP] = useState(profile);
  const [tab, setTab] = useState("general");
  const set = (patch) => setP(x => ({ ...x, ...patch }));
  const changeOS = (os) => set({ os, userAgent: uaFor(os), screen: SCREENS[os][0], cores: CORES[os][1], memory: RAM[os][1], webglRenderer: GPUS[os][0] });
  const changeLocale = (locale) => set({ locale, timezone: LOCALES[locale] });
  const setProxy = (patch) => set({ proxy: { ...(p.proxy || { type: "HTTP", host: "", port: "", user: "", pass: "", country: "" }), ...patch } });

  // The saved proxy pool (live only), so attaching one doesn't mean retyping
  // host/port/user/pass by hand. Matched back by host:port so reopening the
  // editor still shows which pool entry is attached.
  const [pool, setPool] = useState([]);
  useEffect(() => { if (live) api.listProxies().then(setPool).catch(() => { /* offline */ }); }, [live]);
  const poolPick = pool.find(x => x.host === p.proxy?.host && String(x.port) === String(p.proxy?.port));

  const iss = coherence(p);
  const score = Math.max(0, 100 - iss.length * 22);

  // Cookies live in the profile's real browser session, so they can only be
  // moved when the engine is running and the profile has been saved.
  const cookiesReady = live && !p._new;
  const [ckBusy, setCkBusy] = useState("");
  const [ckMsg, setCkMsg] = useState("");
  const [ckReplace, setCkReplace] = useState(false);

  const exportCookies = async () => {
    setCkBusy("export"); setCkMsg("");
    try {
      const { cookies } = await api.exportCookies(p.id);
      const blob = new Blob([JSON.stringify({ cookies, origins: [] }, null, 2)], { type: "application/json" });
      const a = document.createElement("a");
      a.href = URL.createObjectURL(blob);
      a.download = `${(p.name || "profile").replace(/[^\w.-]+/g, "-")}-cookies.json`;
      a.click();
      URL.revokeObjectURL(a.href);
      setCkMsg(`Exported ${cookies.length} cookie${cookies.length === 1 ? "" : "s"}.`);
    } catch (e) { setCkMsg("Export failed: " + e.message); }
    setCkBusy("");
  };

  // Probes and warm-up all open the profile headlessly, so they share one
  // "something is running" flag and one result panel per tab.
  const [probe, setProbe] = useState(null);     // { kind, state, data }
  const runProbe = async (kind, fn) => {
    setProbe({ kind, state: "running" });
    try { setProbe({ kind, state: "done", data: await fn() }); }
    catch (e) { setProbe({ kind, state: "error", data: e.message }); }
  };

  // Auto-adjust: align the profile to what its browser really shows, then pull
  // the rewritten values straight back into the editor so you see the change.
  const autoAdjust = async () => {
    setProbe({ kind: "align", state: "running" });
    try {
      const res = await api.align(p.id);
      if (res.profile) setP(x => ({ ...x, ...res.profile }));
      setProbe({ kind: "align", state: "done", data: res });
    } catch (e) { setProbe({ kind: "align", state: "error", data: e.message }); }
  };

  // Warm-up: pick a per-vertical site set, run it now, or schedule it to
  // recur unattended. The schedule for this profile (if any) is loaded live.
  const [preset, setPreset] = useState("general");
  const [sched, setSched] = useState(null);
  const [schedBusy, setSchedBusy] = useState(false);
  useEffect(() => {
    if (!cookiesReady) return;
    api.listSchedules().then(list => setSched(list.find(s => s.profileId === p.id) || null)).catch(() => {});
  }, [cookiesReady, p.id]);

  const warmNow = () =>
    api.warmup(p.id, 5, preset)
      .then(() => setCkMsg(`Warm-up started (${preset}) — runs in the background for ~5 minutes.`))
      .catch(e => setCkMsg("Warm-up failed: " + e.message));

  const toggleSchedule = async () => {
    setSchedBusy(true);
    try {
      if (sched) { await api.deleteSchedule(sched.id); setSched(null); }
      else { setSched(await api.addSchedule({ profileId: p.id, everyHours: 12, minutes: 5, preset })); }
    } catch (e) { setCkMsg("Schedule failed: " + e.message); }
    setSchedBusy(false);
  };
  const patchSchedule = async (patch) => {
    if (!sched) return;
    setSchedBusy(true);
    try { setSched(await api.updateSchedule(sched.id, patch)); }
    catch (e) { setCkMsg("Schedule failed: " + e.message); }
    setSchedBusy(false);
  };

  const importCookies = async (file) => {
    if (!file) return;
    setCkBusy("import"); setCkMsg("");
    try {
      const res = await api.importCookies(p.id, await file.text(), ckReplace);
      setCkMsg(`Imported ${res.imported} cookie${res.imported === 1 ? "" : "s"} from ${file.name}.`);
    } catch (e) { setCkMsg("Import failed: " + e.message); }
    setCkBusy("");
  };

  return (
    <div className="ps-overlay" onClick={onClose}>
      <div className="ps-drawer" onClick={e => e.stopPropagation()}>
        <div className="ps-drawer-h">
          <div>
            <div className="ps-eyebrow">{p._new ? "New profile" : "Edit profile"}</div>
            <div className="ps-drawer-title">{p.name || "Untitled profile"}</div>
          </div>
          <button className="ps-x" onClick={onClose}><X size={18} /></button>
        </div>

        <div className="ps-coh">
          <div className="ps-coh-top">
            <span className="ps-coh-h">
              {iss.length === 0 ? <ShieldCheck size={14} color={T.mint} /> : <ShieldAlert size={14} color={T.amber} />}
              Fingerprint coherence
            </span>
            <span className="ps-coh-score" style={{ color: iss.length ? T.amber : T.mint }}>{score}%</span>
          </div>
          <div className="ps-coh-bar"><div style={{ width: score + "%", background: iss.length ? T.amber : T.mint }} /></div>
          {iss.length > 0
            ? <div className="ps-coh-iss">{iss.map((i, k) => <div key={k}>· {i}</div>)}</div>
            : <div className="ps-coh-ok">All signals agree — this identity reads like one real device.</div>}
        </div>

        <div className="ps-tabs">
          {["general", "fingerprint", "proxy", "advanced"].map(t => (
            <button key={t} className={"ps-tab" + (tab === t ? " active" : "")} onClick={() => setTab(t)}>{t}</button>
          ))}
        </div>

        <div className="ps-drawer-body">
          {tab === "general" && (
            <>
              <Field label="Profile name"><input className="ps-in" value={p.name} onChange={e => set({ name: e.target.value })} placeholder="e.g. FB-Ads-03" /></Field>
              <Row>
                <Field label="Folder">
                  <select className="ps-in" value={p.folder} onChange={e => set({ folder: e.target.value })}>
                    {[...new Set([p.folder, ...folders].filter(f => f && f !== "All profiles"))].map(f => <option key={f}>{f}</option>)}
                  </select>
                </Field>
                <Field label="Tags (comma separated)">
                  <input className="ps-in" value={p.tags.join(", ")} onChange={e => set({ tags: e.target.value.split(",").map(s => s.trim()).filter(Boolean) })} placeholder="client-a, priority" />
                </Field>
              </Row>
              <Field label="Notes"><textarea className="ps-in" rows={3} value={p.notes} onChange={e => set({ notes: e.target.value })} placeholder="Anything worth remembering about this account…" /></Field>
              <Field label="Launch engine">
                <select className="ps-in" value={p.engine || ""} onChange={e => set({ engine: e.target.value })}>
                  {/* Only offered while unset — we don't know the server's default,
                      so let it decide rather than naming an engine we're guessing. */}
                  {!p.engine && <option value="">— engine default —</option>}
                  {ENGINE_ORDER.map(en => <option key={en} value={en}>{ENGINE_META[en].label}{ENGINE_META[en].recommended ? "  ★ recommended" : ""}</option>)}
                </select>
              </Field>
              <div className="ps-hint"><Blocks size={12} /> Numera manages this profile; the engine renders it. Manage engines in the <b style={{ color: T.text }}>Engines</b> tab.</div>
            </>
          )}

          {tab === "fingerprint" && (
            <>
              <SectionLabel icon={<MonitorSmartphone size={13} />} text="Operating system & browser" />
              <Row>
                <Field label="Operating system">
                  <select className="ps-in" value={p.os} onChange={e => changeOS(e.target.value)}>{OS_LIST.map(o => <option key={o}>{o}</option>)}</select>
                </Field>
                <Field label="Browser version"><input className="ps-in" value={p.browserVersion} onChange={e => set({ browserVersion: e.target.value })} /></Field>
              </Row>
              <Field label="User-Agent"><textarea className="ps-in mono" rows={2} value={p.userAgent} onChange={e => set({ userAgent: e.target.value })} /></Field>

              <SectionLabel icon={<Cpu size={13} />} text="Hardware" />
              <Row>
                <Field label="Screen resolution">
                  <select className="ps-in" value={p.screen} onChange={e => set({ screen: e.target.value })}>{[...new Set([p.screen, ...SCREENS[p.os]])].map(s => <option key={s}>{s}</option>)}</select>
                </Field>
                <Field label="CPU cores">
                  <select className="ps-in" value={p.cores} onChange={e => set({ cores: Number(e.target.value) })}>{[...new Set([p.cores, ...CORES[p.os]])].map(c => <option key={c}>{c}</option>)}</select>
                </Field>
                <Field label="Memory (GB)">
                  <select className="ps-in" value={p.memory} onChange={e => set({ memory: Number(e.target.value) })}>{[...new Set([p.memory, ...RAM[p.os]])].map(m => <option key={m}>{m}</option>)}</select>
                </Field>
              </Row>
              <Field label="WebGL renderer (GPU)">
                <select className="ps-in mono" value={p.webglRenderer} onChange={e => set({ webglRenderer: e.target.value })}>{[...new Set([p.webglRenderer, ...GPUS[p.os]])].map(g => <option key={g}>{g}</option>)}</select>
              </Field>

              <SectionLabel icon={<Fingerprint size={13} />} text="Spoofing modes" />
              <Row><Pick label="Canvas" opts={MODES.canvas} val={p.canvas} on={v => set({ canvas: v })} /><Pick label="WebRTC" opts={MODES.webrtc} val={p.webrtc} on={v => set({ webrtc: v })} /></Row>
              <Row><Pick label="Audio context" opts={MODES.audio} val={p.audio} on={v => set({ audio: v })} /><Pick label="Fonts" opts={MODES.fonts} val={p.fonts} on={v => set({ fonts: v })} /></Row>

              <SectionLabel icon={<Globe size={13} />} text="Geo & language" />
              <Row>
                <Field label="Locale"><select className="ps-in" value={p.locale} onChange={e => changeLocale(e.target.value)}>{Object.keys(LOCALES).map(l => <option key={l}>{l}</option>)}</select></Field>
                <Field label={<span><Clock size={11} /> Timezone</span>}><input className="ps-in mono" value={p.timezone} onChange={e => set({ timezone: e.target.value })} /></Field>
                <Pick label="Geolocation" opts={MODES.geo} val={p.geo} on={v => set({ geo: v })} />
              </Row>
              <Row>
                <Field label={<span><Camera size={11} /> Cameras</span>}><input type="number" className="ps-in" value={p.mediaVideo} onChange={e => set({ mediaVideo: Number(e.target.value) })} /></Field>
                <Field label={<span><Mic size={11} /> Microphones</span>}><input type="number" className="ps-in" value={p.mediaAudio} onChange={e => set({ mediaAudio: Number(e.target.value) })} /></Field>
                <Field label="Do Not Track">
                  <button className={"ps-toggle" + (p.dnt ? " on" : "")} onClick={() => set({ dnt: !p.dnt })}><span className="knob" /> <em>{p.dnt ? "On" : "Off"}</em></button>
                </Field>
              </Row>

              <SectionLabel icon={<ShieldCheck size={13} />} text="Trust check" />
              <div className="ps-cookie-actions">
                <button className="ps-btn ghost sm" disabled={!cookiesReady || probe?.state === "running"}
                  onClick={() => runProbe("trust", () => api.trust(p.id))}>
                  <ShieldCheck size={13} /> Run trust check
                </button>
                <button className="ps-btn ghost sm" disabled={!cookiesReady || probe?.state === "running"}
                  onClick={() => runProbe("tls", () => api.tls(p.id))}>
                  <Wifi size={13} /> Check TLS / JA3
                </button>
                <button className="ps-btn primary sm" disabled={!cookiesReady || probe?.state === "running"}
                  onClick={autoAdjust} title="Rewrite mismatched values to match what the browser actually presents">
                  <Sparkles size={13} /> Auto-adjust
                </button>
              </div>
              <ProbePanel probe={probe} kind="trust" />
              <ProbePanel probe={probe} kind="tls" />
              <ProbePanel probe={probe} kind="align" />
              <div className="ps-hint"><ShieldCheck size={12} /> {cookiesReady
                ? "Trust check grades the real browser; TLS/JA3 reads the handshake JavaScript can't touch. Auto-adjust closes the gap — it rewrites platform, CPU, GPU, fonts and the rest to what the browser actually shows (regenerating the OS identity if the engine renders a different one), so nothing contradicts."
                : "Save the profile and start the engine to grade — and auto-adjust — the real browser."}</div>
            </>
          )}

          {tab === "proxy" && (
            <>
              <div className="ps-proxy-toggle">
                <span style={{ fontSize: 13, fontWeight: 600 }}>Use proxy for this profile</span>
                <button className={"ps-toggle" + (p.proxy ? " on" : "")} onClick={() => set({ proxy: p.proxy ? null : { type: "HTTP", host: "", port: "", user: "", pass: "", country: "US" } })}>
                  <span className="knob" /> <em>{p.proxy ? "On" : "Off"}</em>
                </button>
              </div>
              {p.proxy ? (
                <>
                  {pool.length > 0 && (
                    <Field label="Saved proxy">
                      <select className="ps-in" value={poolPick?.id || ""} onChange={e => {
                        const x = pool.find(q => q.id === e.target.value);
                        if (x) setProxy({ type: x.type || "HTTP", host: x.host, port: String(x.port), user: x.user || "", pass: x.pass || "", country: x.country || "" });
                      }}>
                        <option value="">— manual entry —</option>
                        {pool.map(x => (
                          <option key={x.id} value={x.id}>
                            {x.host}:{x.port}{x.country ? ` · ${x.country}` : ""}{x.status === "ok" ? " · live" : ""}
                          </option>
                        ))}
                      </select>
                    </Field>
                  )}
                  <Row>
                    <Field label="Type"><select className="ps-in" value={p.proxy.type} onChange={e => setProxy({ type: e.target.value })}>{["HTTP", "HTTPS", "SOCKS5"].map(t => <option key={t}>{t}</option>)}</select></Field>
                    <Field label="Country"><select className="ps-in" value={p.proxy.country} onChange={e => setProxy({ country: e.target.value })}>{Object.keys(COUNTRY_LOCALE).map(c => <option key={c}>{c}</option>)}</select></Field>
                  </Row>
                  <Row>
                    <Field label="Host"><input className="ps-in mono" value={p.proxy.host} onChange={e => setProxy({ host: e.target.value })} placeholder="host.proxy.io" /></Field>
                    <Field label="Port"><input className="ps-in mono" value={p.proxy.port} onChange={e => setProxy({ port: e.target.value })} placeholder="8080" /></Field>
                  </Row>
                  <Row>
                    <Field label="Username"><input className="ps-in mono" value={p.proxy.user} onChange={e => setProxy({ user: e.target.value })} /></Field>
                    <Field label="Password"><input className="ps-in mono" type="password" value={p.proxy.pass} onChange={e => setProxy({ pass: e.target.value })} /></Field>
                  </Row>
                  <div className="ps-cookie-actions" style={{ marginTop: 4 }}>
                    <button className="ps-btn ghost sm"
                      disabled={!cookiesReady || probe?.state === "running"}
                      onClick={() => runProbe("proxy", () => api.proxyTest(p.id))}>
                      <RefreshCw size={13} /> Test connection
                    </button>
                    <button className="ps-btn ghost sm"
                      disabled={!cookiesReady || probe?.state === "running"}
                      onClick={() => runProbe("dns", () => api.dns(p.id))}
                      title="Do the profile's DNS lookups exit through the proxy, or leak to your real ISP?">
                      <Globe size={13} /> DNS leak test
                    </button>
                  </div>
                  <ProbePanel probe={probe} kind="proxy" />
                  <ProbePanel probe={probe} kind="dns" />
                  <div className="ps-hint"><MapPin size={12} /> {cookiesReady
                    ? "Test connection routes a real request and reports the exit IP, country and WebRTC leaks. DNS leak test checks whether name lookups escape the proxy to your real ISP's resolver."
                    : "Save the profile and start the engine to test the connection for real."}</div>
                </>
              ) : <div className="ps-hint">No proxy — this profile connects directly. Turn it on to route through a residential or datacenter proxy.</div>}
            </>
          )}

          {tab === "advanced" && (
            <>
              <Field label="Startup URLs (one per line)"><textarea className="ps-in mono" rows={3} value={p.startupUrls} onChange={e => set({ startupUrls: e.target.value })} placeholder={"https://facebook.com\nhttps://business.facebook.com"} /></Field>
              <Field label="WebGL vendor"><input className="ps-in mono" value={p.webglVendor} onChange={e => set({ webglVendor: e.target.value })} /></Field>
              <div className="ps-hint"><Fingerprint size={12} /> Seed & canvas noise are managed by the engine and stay stable across launches for this profile.</div>

              <SectionLabel icon={<Blocks size={13} />} text="Extensions" />
              <Field label="Unpacked extension folders (one per line)">
                <textarea className="ps-in mono" rows={2} value={p.extensions || ""}
                  onChange={e => set({ extensions: e.target.value })}
                  placeholder={"C:\\\\tools\\\\metamask\\n/home/me/ext/ublock"} />
              </Field>
              <div className="ps-hint"><Blocks size={12} /> Point at the folder containing <code>manifest.json</code>. Only these extensions load, so nothing carries over between profiles.</div>

              <SectionLabel icon={<Sparkles size={13} />} text="Warm-up" />
              <Row>
                <Field label="Vertical (which sites to browse)">
                  <select className="ps-in" value={preset} onChange={e => setPreset(e.target.value)}>
                    {WARMUP_PRESETS.map(v => <option key={v.key} value={v.key}>{v.label}</option>)}
                  </select>
                </Field>
                <Field label="Run now">
                  <button className="ps-btn ghost sm" disabled={!cookiesReady} onClick={warmNow} style={{ width: "100%", justifyContent: "center" }}>
                    <Sparkles size={13} /> Warm up 5 min
                  </button>
                </Field>
              </Row>
              <div className="ps-proxy-toggle" style={{ marginTop: 8 }}>
                <span style={{ fontSize: 13, fontWeight: 600 }}>Schedule recurring warm-up</span>
                <button className={"ps-toggle" + (sched ? " on" : "")} disabled={!cookiesReady || schedBusy} onClick={toggleSchedule}>
                  <span className="knob" /> <em>{sched ? "On" : "Off"}</em>
                </button>
              </div>
              {sched && (
                <Row>
                  <Field label="Every (hours)">
                    <input type="number" min="1" className="ps-in" value={sched.everyHours}
                      disabled={schedBusy} onChange={e => patchSchedule({ everyHours: Number(e.target.value) })} />
                  </Field>
                  <Field label="Vertical">
                    <select className="ps-in" value={sched.preset} disabled={schedBusy}
                      onChange={e => patchSchedule({ preset: e.target.value })}>
                      {WARMUP_PRESETS.map(v => <option key={v.key} value={v.key}>{v.label}</option>)}
                    </select>
                  </Field>
                </Row>
              )}
              <div className="ps-hint"><Sparkles size={12} /> A profile created minutes ago with no history is a signal in itself. Warm-up browses that vertical's sites at human pace so the session arrives with a plausible past. Scheduling reruns it unattended (the engine must be running) so the session never goes cold. {cookiesReady ? "" : "Save the profile and start the engine to enable it."}</div>

              <SectionLabel icon={<Cookie size={13} />} text="Session cookies" />
              {cookiesReady ? (
                <>
                  <div className="ps-cookie-actions">
                    <button className="ps-btn ghost sm" disabled={!!ckBusy} onClick={exportCookies}>
                      <Download size={13} /> {ckBusy === "export" ? "Reading session…" : "Export cookies"}
                    </button>
                    <label className={"ps-btn ghost sm" + (ckBusy ? " disabled" : "")}>
                      <Upload size={13} /> {ckBusy === "import" ? "Writing session…" : "Import cookies"}
                      <input type="file" accept=".json,.txt" style={{ display: "none" }} disabled={!!ckBusy}
                        onChange={e => { importCookies(e.target.files?.[0]); e.target.value = ""; }} />
                    </label>
                    <button className={"ps-toggle sm" + (ckReplace ? " on" : "")} onClick={() => setCkReplace(v => !v)}>
                      <span className="knob" /> <em>Replace existing</em>
                    </button>
                  </div>
                  {ckMsg && <div className="ps-hint">{ckMsg}</div>}
                  <div className="ps-hint"><Cookie size={12} /> Reads and writes the profile's real cookie jar. Accepts Cookie-Editor / EditThisCookie JSON, Playwright storage state, and Netscape <code>cookies.txt</code>. Stop the profile first — its session can't be open twice.</div>
                </>
              ) : (
                <div className="ps-hint"><Cookie size={12} /> {p._new
                  ? "Save this profile first, then you can move a session in or out."
                  : "Cookie transfer needs the engine running — start it with numera serve."}</div>
              )}
            </>
          )}
        </div>

        <div className="ps-drawer-foot">
          <button className="ps-btn ghost" onClick={onClose}>Cancel</button>
          <button className="ps-btn primary" disabled={!p.name} onClick={() => onSave({ ...p, _new: false })}>
            <Check size={15} /> {p._new ? "Create profile" : "Save changes"}
          </button>
        </div>
      </div>
    </div>
  );
}

/* ---- Multi-profile synchroniser: one change, applied to many ------ */
function SyncModal({ ids, folders = [], onClose, onApply }) {
  const [engine, setEngine] = useState("");
  const [locale, setLocale] = useState("");
  const [tag, setTag] = useState("");
  const [folder, setFolder] = useState("");

  // Only the fields you actually filled in get sent — a bulk edit should never
  // quietly reset something you left blank.
  const patch = {};
  if (engine) patch.engine = engine;
  if (locale) patch.locale = locale;
  if (folder) patch.folder = folder;
  if (tag.trim()) patch.addTags = tag.split(",").map(s => s.trim()).filter(Boolean);
  const count = Object.keys(patch).length;

  return (
    <div className="ps-overlay center" onClick={onClose}>
      <div className="ps-modal ps-pop" onClick={e => e.stopPropagation()}>
        <div className="ps-drawer-h">
          <div>
            <div className="ps-eyebrow">Edit {ids.length} profiles</div>
            <div className="ps-drawer-title">Change once, apply to all</div>
          </div>
          <button className="ps-x" onClick={onClose}><X size={18} /></button>
        </div>
        <div style={{ padding: 20 }}>
          <Row>
            <Field label="Launch engine">
              <select className="ps-in" value={engine} onChange={e => setEngine(e.target.value)}>
                <option value="">— leave unchanged —</option>
                {ENGINE_ORDER.map(en => <option key={en} value={en}>{ENGINE_META[en].label}</option>)}
              </select>
            </Field>
            <Field label="Locale (moves timezone too)">
              <select className="ps-in" value={locale} onChange={e => setLocale(e.target.value)}>
                <option value="">— leave unchanged —</option>
                {Object.keys(LOCALES).map(l => <option key={l}>{l}</option>)}
              </select>
            </Field>
          </Row>
          <Row>
            <Field label="Move to folder">
              <select className="ps-in" value={folder} onChange={e => setFolder(e.target.value)}>
                <option value="">— leave unchanged —</option>
                {folders.filter(f => f !== "All profiles").map(f => <option key={f}>{f}</option>)}
              </select>
            </Field>
            <Field label="Add tags (comma separated)">
              <input className="ps-in" value={tag} onChange={e => setTag(e.target.value)} placeholder="q3-campaign" />
            </Field>
          </Row>
          <div className="ps-hint"><Copy size={12} /> Blank fields are left alone, so you can run this again later to change one more thing.</div>
        </div>
        <div className="ps-drawer-foot">
          <button className="ps-btn ghost" onClick={onClose}>Cancel</button>
          <button className="ps-btn primary" disabled={!count} onClick={() => onApply(patch)}>
            <Check size={15} /> Apply to {ids.length}
          </button>
        </div>
      </div>
    </div>
  );
}

/* ---- Bulk create ------------------------------------------------- */
// A row of toggle chips for picking several values (OS, country, browser).
function ChipPick({ options, value, onToggle }) {
  return (
    <div className="ps-chiprow">
      {options.map(o => (
        <button key={o} type="button" className={"ps-chip-btn" + (value.includes(o) ? " on" : "")}
          onClick={() => onToggle(o)}>{value.includes(o) && <Check size={11} />}{o}</button>
      ))}
    </div>
  );
}

function BulkModal({ onClose, onCreate, folder, defaultEngine }) {
  const [n, setN] = useState(50);
  const [oses, setOses] = useState(["Windows"]);
  const [countries, setCountries] = useState(["US"]);
  const [browsers, setBrowsers] = useState([...CHROME_VERSIONS]);
  const [engine, setEngine] = useState(defaultEngine || "");   // "" → the server's own default
  const [randomize, setRandomize] = useState(true);
  const [prefix, setPrefix] = useState("Batch");
  const [start, setStart] = useState(1);
  const [tagStr, setTagStr] = useState("bulk");
  const [busy, setBusy] = useState(false);
  const [prog, setProg] = useState(0);

  // Keep at least one selected in each multi-pick.
  const toggle = (setArr) => (v) => setArr(a => a.includes(v) ? (a.length > 1 ? a.filter(x => x !== v) : a) : [...a, v]);
  const pad = Math.max(2, String(start + n - 1).length);

  const build = () => {
    const tags = tagStr.split(",").map(s => s.trim()).filter(Boolean);
    const rnd = (arr) => arr[Math.floor(Math.random() * arr.length)];
    const pick = (arr, dflt) => randomize ? rnd(arr) : (dflt ?? arr[0]);
    const list = new Array(n);
    for (let i = 0; i < n; i++) {
      const os = oses[i % oses.length];
      const country = countries[Math.floor(i / oses.length) % countries.length];
      const chrome = rnd(browsers) || "128.0.0.0";
      const gpu = pick(GPUS[os]);
      const locale = COUNTRY_LOCALE[country] || "en-US";
      list[i] = {
        name: `${prefix}-${String(start + i).padStart(pad, "0")}`,
        folder: folder === "All profiles" ? "Facebook Ads" : folder,
        // null, not "", when unset: the server rejects an engine it doesn't know
        // but treats a null one as "you decide".
        status: "stopped", os, tags, engine: engine || null, browser: "Chrome", browserVersion: chrome,
        userAgent: uaFor(os, chrome), screen: pick(SCREENS[os]),
        cores: pick(CORES[os], CORES[os][1]), memory: pick(RAM[os], RAM[os][1]),
        webglVendor: vendorFor(gpu), webglRenderer: gpu,
        canvas: "Noise", webrtc: "Altered", audio: "Noise", fonts: "Masked",
        locale, timezone: LOCALES[locale], geo: "Prompt", mediaVideo: 1, mediaAudio: 1, dnt: false,
        proxy: null,   // real proxies are attached from the pool (Proxies → Assign)
        notes: "", startupUrls: "", extensions: "", lastActive: "never", _new: true,
      };
    }
    return list;
  };

  const create = async () => {
    setBusy(true); setProg(0);
    try { await onCreate(build(), (done) => setProg(done)); onClose(); }
    catch (e) { toast.error("Bulk create failed: " + e.message); setBusy(false); }
  };

  return (
    <div className="ps-overlay center" onClick={busy ? undefined : onClose}>
      <div className="ps-modal ps-pop" onClick={e => e.stopPropagation()} style={{ maxWidth: 560 }}>
        <div className="ps-drawer-h">
          <div><div className="ps-eyebrow">Bulk create</div><div className="ps-drawer-title">Generate profiles at scale</div></div>
          <button className="ps-x" onClick={onClose} disabled={busy}><X size={18} /></button>
        </div>
        <div style={{ padding: 20, maxHeight: "62vh", overflowY: "auto" }}>
          <Row>
            <Field label="How many (up to 10,000)">
              <input type="number" className="ps-in" value={n} min={1} max={10000}
                onChange={e => setN(Math.min(10000, Math.max(1, Number(e.target.value) || 1)))} />
            </Field>
            <Field label="Launch engine">
              <select className="ps-in" value={engine} onChange={e => setEngine(e.target.value)}>
                {!engine && <option value="">— engine default —</option>}
                {ENGINE_ORDER.map(en => <option key={en} value={en}>{ENGINE_META[en].label}{ENGINE_META[en].recommended ? "  ★" : ""}</option>)}
              </select>
            </Field>
          </Row>

          <Field label="Operating systems (spread across)"><ChipPick options={OS_LIST} value={oses} onToggle={toggle(setOses)} /></Field>
          <Field label="Countries / locales (spread across)"><ChipPick options={Object.keys(COUNTRY_LOCALE)} value={countries} onToggle={toggle(setCountries)} /></Field>
          <Field label="Browser versions (Chrome)"><ChipPick options={CHROME_VERSIONS} value={browsers} onToggle={toggle(setBrowsers)} /></Field>

          <Row>
            <Field label="Name prefix"><input className="ps-in" value={prefix} onChange={e => setPrefix(e.target.value)} /></Field>
            <Field label="Start number"><input type="number" className="ps-in" value={start} min={0} onChange={e => setStart(Math.max(0, Number(e.target.value) || 0))} /></Field>
            <Field label="Tags"><input className="ps-in" value={tagStr} onChange={e => setTagStr(e.target.value)} placeholder="bulk, q3" /></Field>
          </Row>

          <Field label="Vary GPU / screen / CPU / RAM">
            <button className={"ps-toggle" + (randomize ? " on" : "")} onClick={() => setRandomize(v => !v)}><span className="knob" /> <em>{randomize ? "Randomized per profile" : "Fixed per OS"}</em></button>
          </Field>

          <div className="ps-hint"><Boxes size={12} /> Each profile gets its own coherent fingerprint, seed & session — {randomize ? "hardware varies per profile" : "hardware fixed per OS"}. Preview: <b style={{ color: T.text }}>{prefix}-{String(start).padStart(pad, "0")} … {prefix}-{String(start + n - 1).padStart(pad, "0")}</b> · {oses.join("/")} · {countries.join("/")}</div>
          <div className="ps-hint"><Globe size={12} /> These are created without a proxy. Attach real proxies in bulk from <b style={{ color: T.text }}>Proxies → Assign to profiles</b> (round-robin, aligns locale to each proxy's country).</div>
        </div>
        <div className="ps-drawer-foot">
          <button className="ps-btn ghost" onClick={onClose} disabled={busy}>Cancel</button>
          <button className="ps-btn primary" onClick={create} disabled={busy}>
            {busy ? <><RefreshCw size={15} className="ps-spin" /> Creating {prog}/{n}…</> : <><Plus size={15} /> Create {n} profile{n > 1 ? "s" : ""}</>}
          </button>
        </div>
      </div>
    </div>
  );
}

/* ---- Proxies view: a managed pool (import, test, assign) --------- */
function ProxyStatus({ p }) {
  const s = p.status;
  if (s === "ok") return <span className="ps-pill ok" title={p.lastCheck?.ip || ""}><Check size={12} /> {p.lastCheck?.country || "OK"}{p.lastCheck?.latencyMs ? ` · ${p.lastCheck.latencyMs}ms` : ""}</span>;
  if (s === "failed") return <span className="ps-pill warn" title={p.lastCheck?.error || ""}><ShieldAlert size={12} /> Failed</span>;
  if (s === "skipped") return <span className="ps-pill" title={p.lastCheck?.error || ""} style={{ background: T.violetDim, color: T.lilac }}><Circle size={10} /> SOCKS</span>;
  return <span className="ps-pill" style={{ color: T.dim }}><Circle size={10} /> Untested</span>;
}

/* ---- Health overview --------------------------------------------- */
function GradeBadge({ grade }) {
  if (!grade) return <span className="ps-mono dim">—</span>;
  const c = grade === "A" ? T.mint : grade === "B" ? T.lilac : grade === "C" ? T.amber : T.red;
  return <span className="ps-grade" style={{ color: c, borderColor: c + "55", background: c + "18" }}>{grade}</span>;
}

function HealthView({ profiles, live, refresh, setEditor }) {
  const [data, setData] = useState(null);
  const [busy, setBusy] = useState({});           // id -> action label
  const load = async () => { try { setData(await api.profileHealth()); } catch { /* offline */ } };
  useEffect(() => { if (live) load(); }, [live]);

  // Demo mode: no engine to grade against, so derive coherence/proxy from the
  // sample profiles and be honest that trust and schedules need the engine.
  if (!live) {
    const colliding = collidingIds(profiles);
    const rows = profiles.map(p => ({ ...p, coherent: coherence(p).length === 0, issues: coherence(p), collides: colliding.has(p.id) }));
    const coherent = rows.filter(r => r.coherent).length;
    return (
      <div className="ps-scroll">
        <div className="ps-health-summary">
          <HealthChip label="Profiles" value={rows.length} />
          <HealthChip label="Coherent" value={`${coherent}/${rows.length}`} color={T.mint} />
          <HealthChip label="With proxy" value={rows.filter(r => r.proxy).length} color={T.lilac} />
          <HealthChip label="Shared FP" value={colliding.size} color={colliding.size ? T.amber : T.mint} />
        </div>
        <div className="ps-hint"><Activity size={12} /> Start the engine (<code>numera serve</code>) for live trust grades, DNS/proxy status and warm-up schedules. Showing coherence and fingerprint collisions derived from the sample profiles.</div>
        <table className="ps-table" style={{ marginTop: 12 }}>
          <thead><tr><th>Profile</th><th>Engine</th><th>OS</th><th>Coherence</th><th>Proxy</th></tr></thead>
          <tbody>{rows.map((r, i) => (
            <tr key={r.id || i} className={r.collides ? "ps-attention" : ""}>
              <td>{r.name}{r.collides && <SharedTag />}</td>
              <td className="ps-mono dim">{r.engine || "—"}</td>
              <td><span className="ps-os">{r.os}</span></td>
              <td>{r.coherent ? <span className="ps-check ok" style={{ padding: 0, background: "none" }}><Check size={12} /> coherent</span>
                : <span className="ps-check" style={{ padding: 0, background: "none" }}><ShieldAlert size={12} /> {r.issues.length} issue(s)</span>}</td>
              <td className="ps-mono">{r.proxy ? (r.proxy.country || r.proxy.host) : "—"}</td>
            </tr>
          ))}</tbody>
        </table>
      </div>
    );
  }

  const rows = data?.profiles || [];
  const s = data?.summary || {};
  const act = async (id, label, fn) => {
    setBusy(b => ({ ...b, [id]: label }));
    try { await fn(); await load(); await refresh(); }
    catch (e) { toast.error(`${label} failed: ` + e.message); }
    setBusy(b => { const n = { ...b }; delete n[id]; return n; });
  };

  return (
    <div className="ps-scroll">
      <div className="ps-health-summary">
        <HealthChip label="Profiles" value={s.total ?? "—"} />
        <HealthChip label="Coherent" value={`${s.coherent ?? 0}/${s.total ?? 0}`} color={T.mint} />
        <HealthChip label="With proxy" value={`${s.withProxy ?? 0}/${s.total ?? 0}`} color={T.lilac} />
        <HealthChip label="Scheduled" value={s.scheduled ?? 0} color={T.violet} />
        <HealthChip label="Shared FP" value={s.colliding ?? 0} color={s.colliding ? T.amber : T.mint} />
        <HealthChip label="Needs attention" value={s.needsAttention ?? 0} color={s.needsAttention ? T.amber : T.mint} />
        <div style={{ flex: 1 }} />
        <button className="ps-btn ghost sm" onClick={load}><RefreshCw size={13} /> Refresh</button>
      </div>

      {rows.length === 0
        ? <div className="ps-empty"><Activity size={26} color={T.dim} /><div>No profiles yet.</div></div>
        : (
          <table className="ps-table">
            <thead><tr>
              <th>Profile</th><th>Engine</th><th>Coherence</th><th>Proxy</th>
              <th>Trust</th><th>Warm-up</th><th>Last active</th>
              <th style={{ width: 150, textAlign: "right" }}>Actions</th>
            </tr></thead>
            <tbody>
              {rows.map((r, i) => (
                <tr key={r.id} className={"ps-rise" + (r.needsAttention ? " ps-attention" : "")}
                    style={{ animationDelay: `${Math.min(i, 12) * 28}ms` }}>
                  <td>
                    <button className="ps-linkish" onClick={() => setEditor(profiles.find(p => p.id === r.id) || null)}>{r.name}</button>
                    {r.status === "running" && <span className="ps-runtag">live</span>}
                    {r.collides && <SharedTag />}
                  </td>
                  <td className="ps-mono dim">{r.engine}</td>
                  <td>{r.coherent
                    ? <span style={{ color: T.mint, display: "inline-flex", gap: 5, alignItems: "center" }}><Check size={13} /> ok</span>
                    : <span style={{ color: T.amber, display: "inline-flex", gap: 5, alignItems: "center" }} title={(r.issues || []).join("; ")}><ShieldAlert size={13} /> {r.issues.length}</span>}</td>
                  <td className="ps-mono">{r.proxy?.set ? (r.proxy.country || r.proxy.type || "set") : <span className="dim">none</span>}</td>
                  <td><GradeBadge grade={r.trust?.grade} /></td>
                  <td className="ps-mono dim">{r.schedule
                    ? <span title={`${r.schedule.preset}, every ${r.schedule.everyHours}h`} style={{ color: r.schedule.enabled ? T.lilac : T.dim }}><Clock size={11} /> {r.schedule.everyHours}h</span>
                    : "—"}</td>
                  <td className="ps-mono dim">{r.lastActive}</td>
                  <td>
                    <div className="ps-actions">
                      <button title="Run trust check" className="run" disabled={!!busy[r.id] || r.status === "running"}
                        onClick={() => act(r.id, "Trust check", () => api.trust(r.id))}>
                        {busy[r.id] === "Trust check" ? <RefreshCw size={13} className="ps-spin" /> : <ShieldCheck size={13} />}
                      </button>
                      <button title="Auto-adjust to grade A" className="run" disabled={!!busy[r.id] || r.status === "running"}
                        onClick={() => act(r.id, "Auto-adjust", () => api.align(r.id))}>
                        {busy[r.id] === "Auto-adjust" ? <RefreshCw size={13} className="ps-spin" /> : <Sparkles size={13} />}
                      </button>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      <div className="ps-hint"><Activity size={12} /> Coherence and proxy are read from each profile instantly; the <b style={{ color: T.text }}>trust grade is the last one measured</b> (a live check opens a real browser). Use <ShieldCheck size={11} /> to grade a profile and <Sparkles size={11} /> to auto-adjust it to grade A. Rows needing attention (incoherent, grade C/D, or a <Link2 size={11} /> shared fingerprint) are highlighted — regenerate one side of a shared pair so the accounts can't be linked.</div>
    </div>
  );
}

function HealthChip({ label, value, color }) {
  return (
    <div className="ps-hchip">
      <span className="ps-hchip-val" style={color ? { color } : undefined}>{value}</span>
      <span className="ps-hchip-label">{label}</span>
    </div>
  );
}

// Small "shared FP" flag for a profile that isn't fingerprint-unique.
function SharedTag() {
  return (
    <span className="ps-shared" title="Shares a fingerprint with another profile — a linkage risk">
      <Link2 size={10} /> shared FP
    </span>
  );
}

// Demo-mode fingerprint collision: which profiles look like the same device.
// Mirrors engine/collision.py's signature over the fields the sample data has.
function fpSignature(p) {
  return [p.os, p.userAgent, p.screen, p.cores, p.memory, p.webglRenderer].join("|");
}
function collidingIds(profiles) {
  const bySig = {};
  for (const p of profiles) (bySig[fpSignature(p)] ||= []).push(p.id);
  const out = new Set();
  for (const ids of Object.values(bySig)) if (ids.length > 1) ids.forEach(id => out.add(id));
  return out;
}

function ProxiesView({ profiles, live, refresh }) {
  const [pool, setPool] = useState([]);
  const [importOpen, setImportOpen] = useState(false);
  const [testing, setTesting] = useState(new Set());
  const [busy, setBusy] = useState("");

  const load = async () => { try { setPool(await api.listProxies()); } catch { /* offline */ } };
  useEffect(() => { if (live) load(); }, [live]);

  // Demo mode: no engine, so show the proxies derived from profiles, read-only.
  if (!live) {
    const derived = Object.values(profiles.filter(p => p.proxy).reduce((m, p) => {
      const k = p.proxy.host; (m[k] ||= { ...p.proxy, used: 0 }).used++; return m;
    }, {}));
    return (
      <div className="ps-scroll">
        <div className="ps-toolbar" style={{ paddingLeft: 0, paddingRight: 0 }}>
          <div style={{ fontSize: 13, fontWeight: 600, color: T.muted }}>Proxy pool</div>
        </div>
        <div className="ps-hint"><Globe size={12} /> Start the engine (<code>numera serve</code>) to import, test and assign a real proxy pool. Showing proxies in use by current profiles.</div>
        {derived.length > 0 && (
          <table className="ps-table" style={{ marginTop: 12 }}>
            <thead><tr><th>Host</th><th>Type</th><th>Country</th><th>Used by</th></tr></thead>
            <tbody>{derived.map((x, i) => (
              <tr key={i}><td className="ps-mono">{x.host}:{x.port}</td><td><span className="ps-os">{x.type}</span></td><td className="ps-mono">{x.country}</td><td className="ps-mono dim">{x.used}</td></tr>
            ))}</tbody>
          </table>
        )}
      </div>
    );
  }

  const testOne = async (id) => {
    setTesting(s => new Set(s).add(id));
    try { await api.testProxy(id); await load(); }
    catch (e) { toast.error("Test failed: " + e.message); }
    setTesting(s => { const n = new Set(s); n.delete(id); return n; });
  };
  const testAll = async () => {
    setBusy("test");
    for (const p of pool) { try { await api.testProxy(p.id); } catch { /* keep going */ } }
    await load(); setBusy("");
  };
  const del = async (id) => { try { await api.deleteProxy(id); await load(); } catch (e) { toast.error(e.message); } };
  const assign = async () => {
    if (!pool.length) return;
    if (!confirm(`Assign these ${pool.length} proxies round-robin across all ${profiles.length} profiles? Each profile's locale/timezone will align to its proxy's country.`)) return;
    setBusy("assign");
    try { const r = await api.assignProxies(); toast.success(`Assigned proxies to ${r.assigned} profile(s).`); await refresh(); }
    catch (e) { toast.error("Assign failed: " + e.message); }
    setBusy("");
  };

  const okCount = pool.filter(p => p.status === "ok").length;
  return (
    <div className="ps-scroll">
      <div className="ps-toolbar" style={{ paddingLeft: 0, paddingRight: 0 }}>
        <div style={{ fontSize: 13, fontWeight: 600, color: T.muted }}>
          Proxy pool <span className="ps-mono dim">· {pool.length} total{okCount ? ` · ${okCount} live` : ""}</span>
        </div>
        <div style={{ flex: 1 }} />
        <button className="ps-btn ghost sm" disabled={!pool.length || busy === "test"} onClick={testAll}>
          {busy === "test" ? <><RefreshCw size={13} className="ps-spin" /> Testing…</> : <><Wifi size={13} /> Test all</>}
        </button>
        <button className="ps-btn ghost sm" disabled={!pool.length || !profiles.length || busy === "assign"} onClick={assign}>
          {busy === "assign" ? <><RefreshCw size={13} className="ps-spin" /> Assigning…</> : <><ArrowRight size={13} /> Assign to profiles</>}
        </button>
        <button className="ps-btn primary sm" onClick={() => setImportOpen(true)}><Plus size={13} /> Import proxies</button>
      </div>

      {pool.length === 0
        ? <div className="ps-empty"><Globe size={26} color={T.dim} /><div>No proxies yet.</div>
            <button className="ps-btn primary sm" onClick={() => setImportOpen(true)}><Plus size={14} /> Import proxies</button></div>
        : (
          <table className="ps-table">
            <thead><tr><th>Host</th><th>Type</th><th>Country</th><th>Auth</th><th>Status</th><th style={{ width: 120, textAlign: "right" }}>Actions</th></tr></thead>
            <tbody>
              {pool.map((x, i) => (
                <tr key={x.id} className="ps-rise" style={{ animationDelay: `${Math.min(i, 12) * 28}ms` }}>
                  <td className="ps-mono">{x.host}:{x.port}</td>
                  <td><span className="ps-os">{x.type}</span></td>
                  <td className="ps-mono">{x.country || "—"}</td>
                  <td className="ps-mono dim">{x.user ? "user:•••" : "none"}</td>
                  <td><ProxyStatus p={x} /></td>
                  <td>
                    <div className="ps-actions">
                      <button title="Test" className="run" disabled={testing.has(x.id)} onClick={() => testOne(x.id)}>
                        {testing.has(x.id) ? <RefreshCw size={13} className="ps-spin" /> : <Wifi size={13} />}
                      </button>
                      <button title="Delete" className="del" onClick={() => del(x.id)}><Trash2 size={13} /></button>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      <div className="ps-hint"><Globe size={12} /> Import a list, test which are live, then <b style={{ color: T.text }}>Assign to profiles</b> spreads them round-robin and aligns each profile's locale/timezone to its proxy country. Health checks run over HTTP(S); SOCKS proxies are tested when you run a profile's proxy test.</div>

      {importOpen && <ImportProxiesModal onClose={() => setImportOpen(false)} onImport={async (text) => {
        const r = await api.addProxies(text); await load(); return r;
      }} />}
    </div>
  );
}

function ImportProxiesModal({ onClose, onImport }) {
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  const lines = text.split("\n").filter(l => l.trim() && !l.trim().startsWith("#")).length;
  const submit = async () => {
    setBusy(true);
    try { const r = await onImport(text); toast.success(`Added ${r.added} proxy(ies). Pool now has ${r.total}.`); onClose(); }
    catch (e) { toast.error("Import failed: " + e.message); setBusy(false); }
  };
  return (
    <div className="ps-overlay center" onClick={busy ? undefined : onClose}>
      <div className="ps-modal ps-pop" onClick={e => e.stopPropagation()} style={{ maxWidth: 540 }}>
        <div className="ps-drawer-h">
          <div><div className="ps-eyebrow">Proxy pool</div><div className="ps-drawer-title">Import proxies</div></div>
          <button className="ps-x" onClick={onClose} disabled={busy}><X size={18} /></button>
        </div>
        <div style={{ padding: 20 }}>
          <Field label="Paste one proxy per line">
            <textarea className="ps-in mono" rows={9} value={text} onChange={e => setText(e.target.value)}
              placeholder={"host:port\nhost:port:user:pass\nuser:pass@host:port\nhttp://user:pass@host:port\nsocks5://host:port\nhost:port:user:pass:US"} />
          </Field>
          <label className={"ps-btn ghost sm" + (busy ? " disabled" : "")} style={{ marginTop: 8 }}>
            <Upload size={13} /> Load from file
            <input type="file" accept=".txt,.csv,.json" style={{ display: "none" }} disabled={busy}
              onChange={async e => { const f = e.target.files?.[0]; if (f) setText(await f.text()); e.target.value = ""; }} />
          </label>
          <div className="ps-hint"><Server size={12} /> Understands <code>host:port</code>, <code>host:port:user:pass</code>, <code>user:pass@host:port</code> and <code>scheme://…</code>, with an optional trailing country code. {lines > 0 ? <b style={{ color: T.text }}>{lines} line{lines === 1 ? "" : "s"} detected.</b> : ""}</div>
        </div>
        <div className="ps-drawer-foot">
          <button className="ps-btn ghost" onClick={onClose} disabled={busy}>Cancel</button>
          <button className="ps-btn primary" onClick={submit} disabled={busy || !lines}>
            {busy ? <><RefreshCw size={15} className="ps-spin" /> Importing…</> : <><Plus size={15} /> Add {lines || ""} prox{lines === 1 ? "y" : "ies"}</>}
          </button>
        </div>
      </div>
    </div>
  );
}

/* ---- Automation view --------------------------------------------- */
const CDP_LANGS = ["Playwright", "Puppeteer", "Selenium"];
function cdpSnippet(lang, port) {
  const base = `http://127.0.0.1:${port}`;
  if (lang === "Puppeteer")
    return `const browser = await puppeteer.connect({ browserURL: "${base}" });\nconst [page] = await browser.pages();`;
  if (lang === "Selenium")
    return `from selenium import webdriver\nopts = webdriver.ChromeOptions()\nopts.add_experimental_option("debuggerAddress", "127.0.0.1:${port}")\ndriver = webdriver.Chrome(options=opts)`;
  return `from playwright.sync_api import sync_playwright\npw = sync_playwright().start()\nbrowser = pw.chromium.connect_over_cdp("${base}")\npage = browser.contexts[0].pages[0]`;
}

function AutomationView({ profiles, live, refresh }) {
  const [info, setInfo] = useState({});     // pid -> { port, wsUrl, snippets }
  const [busy, setBusy] = useState("");
  const [lang, setLang] = useState("Playwright");
  const [copied, setCopied] = useState("");
  const [headless, setHeadless] = useState(false);
  const CHROMIUM = ["cloak", "patchright", "playwright"];

  const copy = (text, key) => {
    navigator.clipboard?.writeText(text);
    setCopied(key); setTimeout(() => setCopied(c => (c === key ? "" : c)), 1200);
  };
  const attach = async (id) => {
    setBusy(id);
    try {
      const r = await api.attach(id, headless);
      setInfo(m => ({ ...m, [id]: r }));
      toast.success(`Attached on :${r.port}${r.headless ? " (headless)" : ""}`);
      await refresh();
    } catch (e) { toast.error("Attach failed: " + e.message); }
    setBusy("");
  };
  const detach = async (id) => {
    setBusy(id);
    try { await api.stop(id); setInfo(m => { const n = { ...m }; delete n[id]; return n; }); await refresh(); }
    catch (e) { toast.error("Detach failed: " + e.message); }
    setBusy("");
  };

  const intro = (
    <div className="ps-hint"><Zap size={12} /> Numera opens the browser wearing the identity; your script <b style={{ color: T.text }}>attaches to that same window</b> over the Chrome DevTools Protocol — so the session, cookies, proxy and fingerprint stay exactly as set. (A browser your automation launches itself would be un-spoofed.) Chromium engines only — <code>cloak</code>, <code>patchright</code>, <code>playwright</code>.</div>
  );

  // Demo mode: no engine to attach to, so show how it works with the CLI and a
  // static example.
  if (!live) {
    return (
      <div className="ps-scroll">
        {intro}
        <div className="ps-cdp" style={{ marginTop: 12 }}>
          <div className="ps-cdp-h"><Server size={13} /> Start the engine, then attach</div>
          <pre className="ps-code">numera attach acct-01 --port 9222</pre>
          <div className="ps-lang-tabs">{CDP_LANGS.map(l => (
            <button key={l} className={"ps-chip" + (lang === l ? " on" : "")} onClick={() => setLang(l)}>{l}</button>
          ))}</div>
          <pre className="ps-code">{cdpSnippet(lang, 9222)}</pre>
        </div>
        <div className="ps-hint"><Activity size={12} /> Start <code>numera serve</code> to attach a real profile from here with one click.</div>
      </div>
    );
  }

  return (
    <div className="ps-scroll">
      {intro}
      <label className="ps-hcheck">
        <input type="checkbox" checked={headless} onChange={e => setHeadless(e.target.checked)} />
        <span>Headless — attach without a visible window (for servers / CI)</span>
      </label>
      {profiles.length === 0
        ? <div className="ps-empty"><Zap size={26} color={T.dim} /><div>No profiles to attach yet.</div></div>
        : (
          <table className="ps-table" style={{ marginTop: 12 }}>
            <thead><tr>
              <th>Profile</th><th>Engine</th><th>Status</th>
              <th style={{ width: 160, textAlign: "right" }}>Automation</th>
            </tr></thead>
            <tbody>
              {profiles.map((p, i) => {
                const canAttach = CHROMIUM.includes(p.engine || "playwright");
                const conn = info[p.id];
                const attachedHere = !!conn;
                const running = p.status === "running";
                return (
                  <React.Fragment key={p.id}>
                    <tr className={"ps-rise" + (attachedHere ? " sel" : "")} style={{ animationDelay: `${Math.min(i, 12) * 28}ms` }}>
                      <td className="ps-pname">{p.name}</td>
                      <td><span className="ps-engine-chip">{ENGINE_META[p.engine]?.label || p.engine || "Playwright"}</span></td>
                      <td>{attachedHere
                        ? <span style={{ color: T.mint, display: "inline-flex", gap: 5, alignItems: "center" }}><Circle size={9} /> attached :{conn.port}</span>
                        : running ? <span className="ps-mono dim">running</span> : <span className="ps-mono dim">stopped</span>}</td>
                      <td>
                        <div className="ps-actions" style={{ justifyContent: "flex-end" }}>
                          {attachedHere
                            ? <button className="danger" title="Close the attached browser" disabled={busy === p.id}
                                onClick={() => detach(p.id)}>{busy === p.id ? <RefreshCw size={13} className="ps-spin" /> : <Square size={13} />} Detach</button>
                            : <button className="run" disabled={!canAttach || running || busy === p.id}
                                title={!canAttach ? "This engine has no DevTools — switch to cloak/patchright/playwright"
                                       : running ? "Stop the profile first" : "Open with a CDP endpoint"}
                                onClick={() => attach(p.id)}>{busy === p.id ? <RefreshCw size={13} className="ps-spin" /> : <Zap size={13} />} Attach</button>}
                        </div>
                      </td>
                    </tr>
                    {attachedHere && (
                      <tr className="ps-cdp-row"><td colSpan={4}>
                        <div className="ps-cdp">
                          <div className="ps-cdp-h"><Server size={13} /> Connect your automation
                            <span style={{ flex: 1 }} />
                            <button className="ps-copy" onClick={() => copy(`http://127.0.0.1:${conn.port}`, p.id + "url")}>
                              <Clipboard size={12} /> {copied === p.id + "url" ? "Copied" : `127.0.0.1:${conn.port}`}</button>
                          </div>
                          <div className="ps-lang-tabs">{CDP_LANGS.map(l => (
                            <button key={l} className={"ps-chip" + (lang === l ? " on" : "")} onClick={() => setLang(l)}>{l}</button>
                          ))}</div>
                          <div className="ps-code-wrap">
                            <pre className="ps-code">{cdpSnippet(lang, conn.port)}</pre>
                            <button className="ps-copy floaty" onClick={() => copy(cdpSnippet(lang, conn.port), p.id + lang)}>
                              <Clipboard size={12} /> {copied === p.id + lang ? "Copied" : "Copy"}</button>
                          </div>
                        </div>
                      </td></tr>
                    )}
                  </React.Fragment>
                );
              })}
            </tbody>
          </table>
        )}
    </div>
  );
}

/* ---- Command palette (⌘K) ---------------------------------------- */
function CommandPalette({ onClose, profiles, setView, setEditor, toggleRun, toggleTheme, newProfile }) {
  const [q, setQ] = useState("");
  const [sel, setSel] = useState(0);
  const inputRef = useRef(null);
  useEffect(() => { inputRef.current?.focus(); }, []);

  const cmds = useMemo(() => {
    const nav = [
      ["Go to Profiles", () => setView("profiles"), Layers],
      ["Go to Health", () => setView("health"), Activity],
      ["Go to Engines", () => setView("engines"), Blocks],
      ["Go to Proxies", () => setView("proxies"), Globe],
      ["Go to Automation", () => setView("automation"), Zap],
      ["Go to Folders", () => setView("folders"), FolderTree],
      ["Go to Settings", () => setView("settings"), Settings],
    ].map(([label, run, Ic]) => ({ label, run, Ic, group: "Navigate" }));
    const actions = [
      ["New profile", () => newProfile(), Plus],
      ["Toggle light / dark theme", () => toggleTheme(), Sun],
    ].map(([label, run, Ic]) => ({ label, run, Ic, group: "Action" }));
    // Cap the per-profile commands so a 10k list doesn't build 20k entries.
    const profs = profiles.slice(0, 150).flatMap(p => ([
      { label: `Open ${p.name}`, run: () => setEditor(p), Ic: Pencil, group: "Profile" },
      { label: `${p.status === "running" ? "Stop" : "Launch"} ${p.name}`, run: () => toggleRun(p.id),
        Ic: p.status === "running" ? Square : Play, group: "Profile" },
    ]));
    return [...nav, ...actions, ...profs];
  }, [profiles]);

  const filtered = useMemo(() => {
    const s = q.trim().toLowerCase();
    return (s ? cmds.filter(c => c.label.toLowerCase().includes(s)) : cmds).slice(0, 40);
  }, [q, cmds]);
  useEffect(() => { setSel(0); }, [q]);

  const runAt = (i) => { const c = filtered[i]; if (c) { c.run(); onClose(); } };
  const onKey = (e) => {
    if (e.key === "ArrowDown") { e.preventDefault(); setSel(s => Math.min(s + 1, filtered.length - 1)); }
    else if (e.key === "ArrowUp") { e.preventDefault(); setSel(s => Math.max(s - 1, 0)); }
    else if (e.key === "Enter") { e.preventDefault(); runAt(sel); }
  };

  return (
    <div className="ps-cmd-scrim" onClick={onClose}>
      <div className="ps-cmd" onClick={e => e.stopPropagation()}>
        <div className="ps-cmd-in">
          <Search size={16} color={T.dim} />
          <input ref={inputRef} value={q} onChange={e => setQ(e.target.value)} onKeyDown={onKey}
            placeholder="Search commands and profiles…" />
          <kbd className="ps-kbd">esc</kbd>
        </div>
        <div className="ps-cmd-list">
          {filtered.length === 0 && <div className="ps-cmd-empty">No matches</div>}
          {filtered.map((c, i) => (
            <button key={i} className={"ps-cmd-item" + (i === sel ? " on" : "")}
              onMouseEnter={() => setSel(i)} onClick={() => runAt(i)}>
              <c.Ic size={15} />
              <span style={{ flex: 1, textAlign: "left" }}>{c.label}</span>
              <span className="ps-cmd-group">{c.group}</span>
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}

/* ---- Folders manager --------------------------------------------- */
function FoldersView({ folders, profiles, onOpen, onCreate, onRename, onDelete }) {
  const [newName, setNewName] = useState("");
  const [editing, setEditing] = useState(null);   // folder being renamed
  const [draft, setDraft] = useState("");
  const [confirm, setConfirm] = useState(null);    // folder pending delete

  const stat = (name) => {
    const inF = profiles.filter(p => p.folder === name);
    return {
      total: inF.length,
      running: inF.filter(p => p.status === "running").length,
      coherent: inF.filter(p => coherence(p).length === 0).length,
    };
  };
  const submitNew = () => { const n = newName.trim(); if (n) { onCreate(n); setNewName(""); } };
  const commitRename = () => { if (editing) onRename(editing, draft); setEditing(null); setDraft(""); };

  return (
    <div className="ps-scroll">
      <div className="ps-view-head">
        <div>
          <h2 className="ps-view-title">Folders</h2>
          <p className="ps-view-sub">Group profiles by client, platform or project. Renaming or deleting a
            folder moves the profiles inside it — nothing is deleted.</p>
        </div>
        <div className="ps-newfold">
          <FolderPlus size={15} color={T.muted} />
          <input className="ps-in" placeholder="New folder name" value={newName}
            onChange={e => setNewName(e.target.value)}
            onKeyDown={e => e.key === "Enter" && submitNew()} />
          <button className="ps-btn primary sm" onClick={submitNew}><Plus size={14} /> Add</button>
        </div>
      </div>

      <div className="ps-fold-grid">
        {folders.length === 0 && (
          <div className="ps-empty" style={{ gridColumn: "1/-1", padding: 48 }}>
            <div className="ps-empty-ic"><FolderTree size={26} color={T.violet} /></div>
            <div style={{ color: T.muted, fontSize: 13 }}>No folders yet — add one above.</div>
          </div>
        )}
        {folders.map(f => {
          const s = stat(f);
          return (
            <div key={f} className="ps-fold-card">
              <div className="ps-fold-top">
                <div className="ps-fold-ic"><FolderTree size={16} color={T.violet} /></div>
                {editing === f ? (
                  <input autoFocus className="ps-in sm" value={draft}
                    onChange={e => setDraft(e.target.value)}
                    onKeyDown={e => { if (e.key === "Enter") commitRename(); if (e.key === "Escape") { setEditing(null); setDraft(""); } }}
                    onBlur={commitRename} />
                ) : (
                  <button className="ps-fold-name" onClick={() => onOpen(f)} title="Open in Profiles">{f}</button>
                )}
              </div>
              <div className="ps-fold-stats">
                <span><b>{s.total}</b> profile{s.total === 1 ? "" : "s"}</span>
                {s.running > 0 && <span className="ps-fold-run"><Circle size={7} fill={T.mint} color={T.mint} /> {s.running} running</span>}
                <span className="ps-fold-coh"><ShieldCheck size={12} /> {s.coherent}/{s.total}</span>
              </div>
              <div className="ps-fold-actions">
                <button className="ps-btn ghost sm" onClick={() => onOpen(f)}>Open <ArrowRight size={13} /></button>
                <div style={{ flex: 1 }} />
                <button className="ps-iconbtn sm" title="Rename" onClick={() => { setEditing(f); setDraft(f); }}><Pencil size={13} /></button>
                {confirm === f ? (
                  <button className="ps-btn danger sm" onClick={() => { onDelete(f); setConfirm(null); }} onMouseLeave={() => setConfirm(null)}>Confirm?</button>
                ) : (
                  <button className="ps-iconbtn sm danger" title="Delete folder" onClick={() => setConfirm(f)}><Trash2 size={13} /></button>
                )}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}

/* ---- Settings ---------------------------------------------------- */
function SettingsView({ live, config, engines, defaultEngine, onChangeEngine, palette, toggleTheme, counts }) {
  return (
    <div className="ps-scroll">
      <div className="ps-view-head">
        <div>
          <h2 className="ps-view-title">Settings</h2>
          <p className="ps-view-sub">Defaults for new profiles, appearance, and where Numera keeps your data.</p>
        </div>
      </div>

      <div className="ps-set-grid">
        <div className="ps-set-card">
          <div className="ps-set-h"><Palette size={15} color={T.violet} /> Appearance</div>
          <div className="ps-set-row">
            <div>
              <div className="ps-set-label">Theme</div>
              <div className="ps-set-note">Switch the whole console between light and dark.</div>
            </div>
            <button className="ps-btn ghost" onClick={toggleTheme}>
              {palette === "light" ? <><Moon size={14} /> Dark</> : <><Sun size={14} /> Light</>}
            </button>
          </div>
        </div>

        <div className="ps-set-card">
          <div className="ps-set-h"><Blocks size={15} color={T.violet} /> Default launch engine</div>
          <div className="ps-set-note" style={{ marginBottom: 12 }}>Every new profile starts on this engine — you can still change it per profile.</div>
          <div className="ps-seg wrap">
            {ENGINE_ORDER.map(en => (
              <button key={en} className={defaultEngine === en ? "on" : ""} onClick={() => onChangeEngine(en)}
                title={engines && engines[en] === false ? "Not installed" : ""}>
                {ENGINE_META[en].label}{ENGINE_META[en].recommended ? " ★" : ""}
                {engines && engines[en] === false && <span className="ps-set-missing"> · not installed</span>}
              </button>
            ))}
          </div>
        </div>

        <div className="ps-set-card">
          <div className="ps-set-h"><HardDrive size={15} color={T.violet} /> Storage</div>
          {live && config?.root ? (
            <>
              <div className="ps-set-label">Data location</div>
              <pre className="ps-code" style={{ marginTop: 6 }}>{config.root}</pre>
              <div className="ps-set-note" style={{ marginTop: 8 }}>Profiles, sessions, proxies and settings all live here on your machine. Back up this folder to move everything to another computer.</div>
            </>
          ) : (
            <div className="ps-set-note">Connect the engine (<code>numera serve</code>) to see where your data is stored. In demo mode nothing is written to disk.</div>
          )}
          <div className="ps-set-counts">
            <span><b>{counts.profiles}</b> profiles</span>
            <span><b>{counts.folders}</b> folders</span>
            <span><b>{counts.proxies}</b> proxies</span>
          </div>
        </div>

        <div className="ps-set-card">
          <div className="ps-set-h"><Info size={15} color={T.violet} /> About</div>
          <div className="ps-set-row">
            <div className="ps-set-label">Numera</div>
            <span className="ps-pill" style={{ background: T.violetDim, color: T.lilac }}>v{config?.version || "0.1.0"}</span>
          </div>
          <div className="ps-set-note" style={{ marginTop: 6 }}>Self-hosted anti-detect browser &amp; profile manager. {live ? "Engine connected." : "Running in demo mode — start numera serve to go live."}</div>
        </div>
      </div>
    </div>
  );
}

function Placeholder({ view }) {
  const map = {
    team: [Users, "Team", "Invite members, set roles, and share profiles securely."],
  };
  const [Ic, title, desc] = map[view] || [Boxes, "Soon", ""];
  return (
    <div className="ps-scroll">
      <div className="ps-empty" style={{ marginTop: 48 }}>
        <div className="ps-empty-ic"><Ic size={26} color={T.violet} /></div>
        <div style={{ fontSize: 16, fontWeight: 600, fontFamily: FONT.display }}>{title}</div>
        <div style={{ color: T.muted, fontSize: 13, maxWidth: 360, textAlign: "center", lineHeight: 1.6 }}>{desc}</div>
        <span className="ps-pill" style={{ background: T.violetDim, color: T.lilac }}>On the roadmap</span>
      </div>
    </div>
  );
}

/* ---- form atoms -------------------------------------------------- */
const Field = ({ label, children }) => (<label className="ps-field"><span className="ps-flabel">{label}</span>{children}</label>);
const Row = ({ children }) => <div className="ps-row">{children}</div>;
const SectionLabel = ({ icon, text }) => (<div className="ps-seclabel">{icon}<span>{text}</span><div className="ps-secline" /></div>);
const SectionHead = ({ icon, text, note }) => (
  <div className="ps-sechead"><span className="ps-sechead-ic">{icon}</span><span className="ps-sechead-t">{text}</span>{note && <span className="ps-sechead-n">{note}</span>}</div>
);
const Pick = ({ label, opts, val, on }) => (
  <div className="ps-field"><span className="ps-flabel">{label}</span>
    <div className="ps-seg">{opts.map(o => <button key={o} className={val === o ? "on" : ""} onClick={() => on(o)}>{o}</button>)}</div>
  </div>
);

/* ---- styles ------------------------------------------------------ */
// A function (not a const string) so it re-reads the live palette T on every
// render — that's what makes the light/dark toggle repaint the whole stylesheet.
