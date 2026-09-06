import React, { useCallback, useEffect, useMemo, useState } from "react";
import { Activity, Blocks, Boxes, Command, Download, FolderTree, Globe, KeyRound, Layers, Moon, Plus, Puzzle, RefreshCw, Server, Settings, ShieldAlert, ShieldCheck, Sun, Users, X, Zap } from "lucide-react";
import { api, auth, setAuthLostHandler } from "./api";
import { DARK, FONT, LIGHT, T, makeCss } from "./theme";
import { FOLDERS, seed } from "./demoData";
import { ENGINE_META, LOCALES, coherence, nid } from "./deviceData";
import { Toaster, toast } from "./toast";
import { Placeholder, Stat, newProfile } from "./ui";
import { ProfilesView } from "./views/ProfilesView";
import { ExtensionsView } from "./views/ExtensionsView";
import { EnginesView } from "./views/EnginesView";
import { Editor } from "./views/Editor";
import { HealthView } from "./views/HealthView";
import { ProxiesView } from "./views/ProxiesView";
import { AutomationView } from "./views/AutomationView";
import { FoldersView } from "./views/FoldersView";
import { SettingsView } from "./views/SettingsView";
import { BulkModal, SyncModal } from "./views/bulk";
import { CommandPalette } from "./views/CommandPalette";

/* ================================================================== *
 *  Numera — anti-detect browser profile manager
 *
 *  Numera MANAGES the identity (coherent fingerprint, proxy, session).
 *  A pluggable ENGINE renders it in a real browser. The UI is theme-aware,
 *  dark-first, and connects to the engine API when it's running (live mode),
 *  otherwise falls back to sample data (demo mode).
 * ================================================================== */

/* ---- error boundary ---------------------------------------------- *
 * Without one, a single malformed profile that throws while rendering takes the
 * whole React tree down — a white page with no way back. React only catches
 * render errors in a class component, hence the class. Wrap the parts that draw
 * profile data so a bad one degrades to a readable panel and the rest keeps
 * working. `resetKey` clears the error when you navigate somewhere else.       */
export class ErrorBoundary extends React.Component {
  state = { error: null };
  static getDerivedStateFromError(error) { return { error }; }
  componentDidCatch(error, info) { console.error("Numera Browser UI error:", error, info?.componentStack); }
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
/* Shown when the engine answers 401.
 *
 * This exists because the alternative was worse than an error: a locked engine
 * looked exactly like no engine, and the app filled the screen with sample
 * profiles that were not the user's. Someone could have acted on them. */
function LockedScreen({ detail, onUnlock }) {
  const [value, setValue] = React.useState(auth.get());
  const [busy, setBusy] = React.useState(false);
  const [error, setError] = React.useState(detail || "");

  const submit = async (e) => {
    e?.preventDefault?.();
    setBusy(true);
    setError("");
    auth.set(value);
    const state = await api.health();
    setBusy(false);
    if (state === "ok") return onUnlock();
    setError(state === "locked"
      ? "That token was not accepted."
      : "The engine stopped answering. Is it still running?");
  };

  return (
    <div className="ps-root">
      <style>{CSS}</style>
      <div className="ps-locked">
        <form className="ps-locked-card" onSubmit={submit}>
          <div className="ps-locked-ic"><KeyRound size={22} /></div>
          <h1>This engine needs a token</h1>
          <p>
            It is running and reachable — it just will not answer without one.
            Create a token with <code>numera token create "dashboard"</code>, or
            use the value of <code>NUMERA_API_TOKEN</code>.
          </p>
          <input className="ps-in mono" autoFocus type="password" value={value}
            placeholder="nmr_…" onChange={ev => setValue(ev.target.value)} />
          {error && <div className="ps-locked-err"><ShieldAlert size={13} /> {error}</div>}
          <button className="ps-btn primary" type="submit" disabled={busy || !value.trim()}>
            {busy ? <><RefreshCw size={15} className="ps-spin" /> Checking…</> : "Unlock"}
          </button>
          <div className="ps-locked-note">
            Stored in this browser only. It is never sent anywhere but this engine.
          </div>
        </form>
      </div>
    </div>
  );
}

// Built once.  takes no arguments and emits both themes as CSS
// variables; the theme is switched by a  attribute, so the string
// never changes - and this sits in a component that re-renders on every poll.
const CSS = makeCss();

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
  // connecting | live | locked | demo. "locked" is a healthy engine asking for
  // a token; it must never collapse into "demo", which shows sample data.
  const [mode, setMode] = useState("connecting");
  const [authError, setAuthError] = useState("");
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

  // `useCallback` because the polling effect depends on it: recreated every
  // render, it would tear down and restart the interval every render.
  const refresh = useCallback(async () => setProfiles(await api.list()), []);
  // The authoritative config, refetched. `refresh()` reads profiles only, so
  // config and folder state - the default engine, an empty folder - has nothing
  // that repairs it after a rejected mutation. This is that repair.
  const refreshConfig = async () => {
    try {
      const cfg = await api.getConfig();
      setConfig(cfg);
      if (cfg?.default_engine) setDefaultEngine(cfg.default_engine);
      if (Array.isArray(cfg?.folders)) setFolders(cfg.folders);
    } catch { /* offline; the caller has already reported the failure */ }
  };

  // A 401 can arrive at any moment, not just on the first request: creating the
  // first named token locks the running engine, and the token this session is
  // using can be revoked from the Settings tab. Every one of them lands here, so
  // the app can never sit in live mode issuing requests that are all rejected.
  useEffect(() => {
    setAuthLostHandler(e => { setAuthError(e.message); setMode("locked"); });
    return () => setAuthLostHandler(null);
  }, []);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      const up = await api.health();
      if (cancelled) return;
      if (up === "locked") { setMode("locked"); return; }
      if (up !== "ok") { setMode("demo"); return; }
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
      } catch (e) {
        // A 401 is already on its way to the locked screen via the handler
        // above; calling it demo mode here would paint sample data over it.
        if (e?.name !== "AuthError") setMode("demo");
      }
    })();
    return () => { cancelled = true; };
    // `refresh` is stable, so this still runs exactly once, on mount.
  }, [refresh]);

  // Poll less often once the list is large — refetching thousands of profiles
  // every few seconds is wasteful, and run-status doesn't change that fast.
  // Computed here rather than inside the effect so the dependency is a plain
  // value: `profiles.length > 500` in the array is an expression the linter
  // cannot check, and it hid that the effect reads `profiles` at all.
  const pollEvery = profiles.length > 500 ? 12000 : 3000;

  // Keep run status in sync: the browser can be closed by the user directly, so
  // poll the API in live mode and flip the launch button back to "start".
  useEffect(() => {
    if (!live) return;
    let cancelled = false;
    const id = setInterval(() => {
      if (!cancelled && !editor && !bulk && !sync) refresh().catch(() => {});
    }, pollEvery);
    return () => { cancelled = true; clearInterval(id); };
  }, [live, editor, bulk, sync, pollEvery, refresh]);

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
      // Stopping can genuinely fail now: the engine answers 409 when the
      // browser belongs to another server, desktop or CLI, because it has no
      // way to reach it. Saying "Launch failed" there named the wrong action.
      const stopping = p?.status === "running";
      try { await (stopping ? api.stop(id) : api.launch(id)); }
      catch (e) { toast.error((stopping ? "Stop failed: " : "Launch failed: ") + e.message); }
      await refresh();
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
    if (live) {
      // Server first. `setDefaultEngine` used to run before the request, so a
      // rejected save still moved the engine every *new* profile would use -
      // and the profile refresh does not repair config state, so the wrong
      // value stayed on screen until a reload.
      try {
        const exists = profiles.some(x => x.id === p.id) && !p._new;
        await (exists ? api.update(p) : api.create({ ...p, id: undefined }));
        // Refetched, never inferred from `p.engine`. The server changes the
        // default only when the profile's engine actually changed, so editing
        // the notes of a Playwright profile under a Camoufox default used to
        // flip the UI to Playwright over a server that had correctly left it
        // alone - and the next new profile then sent Playwright explicitly and
        // made the server wrong too.
        await Promise.all([refresh(), refreshConfig()]);
      } catch (e) { toast.error("Save failed: " + e.message); await refreshConfig(); }
      return;
    }
    // Demo mode has no server, so it applies the server's rule itself: a
    // default moves only when this profile's engine really changed.
    const before = profiles.find(x => x.id === p.id);
    if (p.engine && p.engine !== before?.engine) setDefaultEngine(p.engine);
    setProfiles(ps => ps.some(x => x.id === p.id) ? ps.map(x => x.id === p.id ? p : x) : [...ps, p]);
  };

  // ---- folder management (used by the Folders tab) ----
  const createFolder = async (name) => {
    name = (name || "").trim();
    if (!name || folderList.includes(name)) { if (name) toast.info(`Folder "${name}" already exists.`); return; }
    if (live) {
      // An empty folder exists only in the folder list, and `refresh()` reads
      // profiles - so a rejected create left a folder in the sidebar that the
      // server had never heard of, and nothing later removed it.
      try {
        const r = await api.createFolder(name);
        // The server's list, or append if it did not return one. Passing the
        // updater as a *fallback value* would have stored the function itself.
        if (Array.isArray(r?.folders)) setFolders(r.folders); else setFolders(f => [...f, name]);
      }
      catch (e) { toast.error("Create folder failed: " + e.message); await refreshConfig(); }
      return;
    }
    setFolders(f => [...f, name]);
    toast.success(`Folder "${name}" created.`);
  };
  const renameFolder = async (from, to) => {
    to = (to || "").trim();
    if (!to || to === from) return;
    if (folderList.includes(to)) { toast.info(`Folder "${to}" already exists.`); return; }
    if (live) {
      try {
        const r = await api.renameFolder(from, to);
        if (r?.folders) setFolders(r.folders);
        if (folder === from) setFolder(to);
        await refresh();
      } catch (e) {
        // Put the authoritative state back rather than leaving the rename on
        // screen: the selected folder can otherwise point at a name that does
        // not exist, which filters the list down to nothing.
        toast.error("Rename failed: " + e.message);
        await refreshConfig();
        await refresh();
      }
      return;
    }
    setFolders(f => f.map(x => x === from ? to : x));
    setProfiles(ps => ps.map(p => p.folder === from ? { ...p, folder: to } : p));
    if (folder === from) setFolder(to);
  };
  const deleteFolder = async (name) => {
    if (live) {
      try {
        const r = await api.deleteFolder(name);
        if (r?.folders) setFolders(r.folders);
        if (folder === name) setFolder("All profiles");
        await refresh();
      } catch (e) {
        toast.error("Delete folder failed: " + e.message);
        await refreshConfig();
        await refresh();
      }
      return;
    }
    setFolders(f => f.filter(x => x !== name));
    setProfiles(ps => ps.map(p => p.folder === name ? { ...p, folder: "Unfiled" } : p));
    if (folder === name) setFolder("All profiles");
    toast.success(`Folder "${name}" removed — its profiles moved to Unfiled.`);
  };

  // Change the engine used for every new profile (Settings tab).
  const changeDefaultEngine = async (name) => {
    if (live) {
      try {
        await api.setDefaultEngine(name);
        setDefaultEngine(name);
        toast.success(`New profiles will use ${ENGINE_META[name]?.label || name}.`);
      } catch (e) {
        // Nothing refetches config on its own, so an optimistic change that the
        // server rejected simply stayed on the Settings screen looking saved.
        toast.error("Couldn't save default engine: " + e.message);
        await refreshConfig();
      }
      return;
    }
    setDefaultEngine(name);
    toast.info("Connect the engine (numera serve) to save this setting.");
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
  if (mode === "locked") {
    return <LockedScreen detail={authError} onUnlock={() => window.location.reload()} />;
  }

  return (
    <div className="ps-root">
      <style>{CSS}</style>
      <div className="ps-shell">
        {/* ---- Sidebar ---- */}
        <aside className="ps-side">
          <div className="ps-brand">
            <div className="ps-logo"><img src="/logo-512.png" alt="" width="36" height="36" /></div>
            <div>
              <div className="ps-brand-name">Numera</div>
              <div className="ps-brand-sub">Browser</div>
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
            {config?.version && <span className="ps-foot-ver">v{config.version}</span>}
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
          // Same rule as `save`: the default comes back from the server.
          try { await api.bulkUpdate(ids, patch); await Promise.all([refresh(), refreshConfig()]); }
          catch (e) { toast.error("Bulk edit failed: " + e.message); await refreshConfig(); }
        } else {
          const { addTags, ...fields } = patch;
          setProfiles(ps => ps.map(p => !ids.includes(p.id) ? p : {
            ...p, ...fields,
            timezone: patch.locale ? LOCALES[patch.locale] : p.timezone,
            tags: addTags ? [...new Set([...p.tags, ...addTags])] : p.tags,
          }));
        }
        if (!live && patch.engine) setDefaultEngine(patch.engine);
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
