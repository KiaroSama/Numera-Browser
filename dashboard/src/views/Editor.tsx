import { IpSwitch } from "./IpSwitch";
import type { FieldPath, OS, PoolProxy, Probe, ProbeResult, Profile, ProfileWrite, Proxy, Schedule } from "../types";
import { ProbePanel } from "./ProbePanel";
import { errorMessage } from "../types";
/* The fingerprint editor - every field of one profile, plus the live probe
 * results (proxy test and trust grade) that come back from a real browser. */

import { useEffect, useRef, useState } from "react";
import { Blocks, Bookmark, Camera, Database, Type, Volume2, Check, Cookie, Cpu, Download, Fingerprint, Globe, MapPin, Mic, MonitorSmartphone, RefreshCw, ShieldAlert, ShieldCheck, Sparkles, Terminal, Upload, Wifi, X } from "lucide-react";
import { ApiError, api } from "../api";
import { fieldValue, profileWrite, rebaseProfile, withField } from "../profileEdits";
import { T } from "../theme";
import { CORES, COUNTRIES, ENGINE_META, ENGINE_ORDER, GPUS, LOCALES, MODES, OS_LIST, RAM, SCREENS, WARMUP_PRESETS, coherence, generatesOwnIdentity, uaFor, vendorFor } from "../deviceData";
import { ProfileSummary } from "./ProfileSummary";
import { Field, Pick, Row, SectionLabel } from "../ui";
import { TimezoneField } from "./TimezoneField";

/* Every resolved value of the profile, in one column beside the form.
 *
 * The point is cross-checking: this app's whole argument is that a fingerprint
 * fails on *contradictions*, not on unusual values. A summary you can read
 * top-to-bottom while editing is how you notice the macOS platform under a
 * Windows user-agent - which the coherence meter scores, but does not show. */
/* GoLogin-style "based on IP" switch.
   A stored timezone is a guess made when the profile was created; the proxy it
   ends up using may exit somewhere else, and a clock that disagrees with the IP
   is the cheapest tell there is. Flipping this makes the driver read the real
   exit country at launch - it can't be resolved earlier, because the profile
   may not have a proxy yet. */
// Resolvers Chromium can be pointed at. The value is what the engine stores;
// a bare IP is mapped to the matching DoH endpoint on the way to the flag.
const DOH = [
  { v: "", label: "System default" },
  { v: "1.1.1.1", label: "Cloudflare — 1.1.1.1" },
  { v: "8.8.8.8", label: "Google — 8.8.8.8" },
  { v: "9.9.9.9", label: "Quad9 — 9.9.9.9" },
  { v: "94.140.14.14", label: "AdGuard — 94.140.14.14" },
];

// What a profile can be told not to carry between launches. The stored value is
// the *exceptions* list, so an existing profile - which has none - keeps today's
// full persistence and nothing had to be migrated.
const KEEPABLE: [string, string, string][] = [
  ["cookies", "Cookies", "Logins. Turning this off makes every launch a fresh visitor."],
  ["localStorage", "Local storage", "Site preferences and app state."],
  ["indexedDb", "IndexedDB", "Larger client-side databases."],
  ["sessions", "Open tabs", "Which tabs were open when the window closed."],
  ["history", "History", "Visited pages and the top-sites tiles."],
  ["passwords", "Saved passwords", "Chromium's own password store."],
  ["extensionData", "Extension data", "Wallet state and per-extension settings."],
  ["bookmarks", "Bookmarks", "Reseeded from the Bookmarks tab on the next launch."],
];

// Chromium subsystems worth a named switch. Both are reachable by typing a flag
// into the box above; they are here because "stop Chrome phoning Google" is a
// thing people want, not a flag people remember.
const SWITCHABLE: [string, string, string][] = [
  ["googleServices", "Google services",
   "Sync, Safe Browsing updates, field trials and usage pings - traffic to Google from the proxy's IP that has nothing to do with this profile."],
  ["systemExtensions", "Built-in Chromium extensions",
   "The Web Store connector, Media Router and Cloud Print. They run in the background whether or not anything uses them."],
];

const conflictValue = (profile: Profile, path: FieldPath) => {
  let value = fieldValue(profile, path);
  if (path[0] === "proxy") {
    if (path[1] === "pass") return "(hidden)";
    if (path.length === 1 && value) value = { ...profile.proxy, pass: "(hidden)" };
  }
  return JSON.stringify(value) ?? "(empty)";
};

function Editor({ profile, live, folders = [], onClose, onSave }: { profile: Profile; live: boolean; folders?: string[]; onClose: () => void; onSave: (profile: ProfileWrite) => Promise<unknown> }) {
  const [p, setP] = useState(profile);
  const [base, setBase] = useState(profile);
  const [conflicts, setConflicts] = useState<FieldPath[]>([]);
  const [saveMessage, setSaveMessage] = useState("");
  const [tab, setTab] = useState("general");
  const [saving, setSaving] = useState(false);
  const pending = useRef(false);
  const close = () => { if (!pending.current) onClose(); };
  const submit = async () => {
    if (pending.current || conflicts.length) return;
    pending.current = true;
    setSaving(true);
    setSaveMessage("");
    try { await onSave(live && !p._new ? profileWrite(base, p) : { ...p, _new: false }); }
    catch (error) {
      if (!(error instanceof ApiError) || error.status !== 409 || error.kind !== "StaleProfile") {
        setSaveMessage("Save failed: " + errorMessage(error));
        return;
      }
      try {
        const latest = (await api.list()).find(x => x.id === p.id);
        if (!latest) throw new Error("This profile was removed. Your draft is still open.", { cause: error });
        const rebased = rebaseProfile(base, p, latest);
        setBase(latest);
        setP(rebased.profile);
        setConflicts(rebased.conflicts);
        setSaveMessage(rebased.conflicts.length
          ? "You and another writer both changed these fields. Choose which edits to keep before saving."
          : "Your edits were rebased onto the latest profile. Review them and save again.");
      } catch (refreshError) {
        setSaveMessage("Could not load the latest profile: " + errorMessage(refreshError) + " Your draft is preserved; retry when it is available.");
      }
    }
    finally { pending.current = false; setSaving(false); }
  };
  const resolve = (path: FieldPath, useLatest: boolean) => {
    if (!useLatest && path[0] === "proxy" && path.length === 1 && p.proxy) {
      if (typeof p.proxy.pass !== "string") {
        setSaveMessage("The proxy password is unavailable. Use latest, or cancel and reload after unlocking the vault before restoring this proxy.");
        return;
      }
      // Choosing the whole proxy restores its known credential, even when only
      // its endpoint was edited. An unavailable credential is never a clear.
      setP(current => ({ ...current, _passwordSet: true }));
    }
    if (useLatest) setP(current => {
      const next = withField(current, path, fieldValue(base, path));
      if (path[0] === "proxy" && (path.length === 1 || path[1] === "pass")) next._passwordSet = false;
      return next;
    });
    setConflicts(current => current.filter(p => p !== path));
    if (conflicts.length === 1) setSaveMessage("Conflict resolved. Review your edits and save again.");
  };
  const set = (patch: Partial<Profile>) => {
    if (!pending.current && !conflicts.length) setP(x => ({ ...x, ...patch }));
  };
  /* Switching the OS rewrites the whole device, not just the label.
   *
   * The vendor is in here because it used to be the one field left behind. The
   * server only fills a vendor in when the dashboard sends a blank one
   * (`vendor or _vendor_for(renderer)` in server/convert.py), and the dashboard
   * never sends a blank one - a new profile is seeded "Google Inc. (NVIDIA)".
   * So a Windows profile switched to macOS kept reporting an NVIDIA vendor
   * under an Apple M1 renderer, which is a contradiction no real machine can
   * produce. `coherence()` does not score the vendor, so nothing said a word.
   * bulk.jsx already paired the two through vendorFor; this is the editor
   * catching up. */
  const changeOS = (os: OS) => set({ os, userAgent: uaFor(os), screen: SCREENS[os][0], cores: CORES[os][1], memory: RAM[os][1], webglRenderer: GPUS[os][0], webglVendor: vendorFor(GPUS[os][0]) });
  const changeLocale = (locale: string) => set({ locale, timezone: LOCALES[locale]! });
  /* A profile loaded from the server carries its decrypted proxy password, and
   * PUTting the whole object back told the server to REPLACE it - so editing
   * only the notes of a profile whose two stored copies disagree silently chose
   * the dashboard's credential instead of returning the 409 it owes. The
   * password is now sent only when the user actually set one, and `_passwordSet`
   * is how api.js knows (see forWrite there). It is marked here rather than
   * guessed from value equality, because "typed the same password again" and
   * "never touched the field" produce identical values and mean opposite
   * things. */
  const setProxy = (patch: Partial<Proxy>) => set({
    proxy: { ...(p.proxy || { type: "HTTP", host: "", port: "", user: "", pass: "", country: "" }), ...patch },
    ...("pass" in patch ? { _passwordSet: true } : {}),
  });

  // The saved proxy pool (live only), so attaching one doesn't mean retyping
  // host/port/user/pass by hand. Matched back by host:port so reopening the
  // editor still shows which pool entry is attached.
  const [pool, setPool] = useState<PoolProxy[]>([]);
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
      a.download = `${String(p.name || "profile").replace(/[^\w.-]+/g, "-")}-cookies.json`;
      a.click();
      URL.revokeObjectURL(a.href);
      setCkMsg(`Exported ${cookies.length} cookie${cookies.length === 1 ? "" : "s"}.`);
    } catch (e) { setCkMsg("Export failed: " + errorMessage(e)); }
    setCkBusy("");
  };

  // Probes and warm-up all open the profile headlessly, so they share one
  // "something is running" flag and one result panel per tab.
  const [probe, setProbe] = useState<Probe | null>(null);     // { kind, state, data }
  const runProbe = async (kind: string, fn: () => Promise<ProbeResult>) => {
    setProbe({ kind, state: "running" });
    try { setProbe({ kind, state: "done", data: await fn() }); }
    catch (e) { setProbe({ kind, state: "error", data: errorMessage(e) }); }
  };

  // Auto-adjust: align the profile to what its browser really shows, then pull
  // the rewritten values straight back into the editor so you see the change.
  const autoAdjust = async () => {
    setProbe({ kind: "align", state: "running" });
    try {
      const res = await api.align(p.id);
      if (res.profile) setP(x => ({ ...x, ...res.profile }));
      setProbe({ kind: "align", state: "done", data: res });
    } catch (e) { setProbe({ kind: "align", state: "error", data: errorMessage(e) }); }
  };

  // Warm-up: pick a per-vertical site set, run it now, or schedule it to
  // recur unattended. The schedule for this profile (if any) is loaded live.
  const [preset, setPreset] = useState("general");
  const [sched, setSched] = useState<Schedule | null>(null);
  const [schedBusy, setSchedBusy] = useState(false);
  useEffect(() => {
    if (!cookiesReady) return;
    api.listSchedules().then(list => setSched(list.find(s => s.profileId === p.id) || null)).catch(() => {});
  }, [cookiesReady, p.id]);

  const warmNow = () =>
    api.warmup(p.id, 5, preset)
      .then(() => setCkMsg(`Warm-up started (${preset}) — runs in the background for ~5 minutes.`))
      .catch(e => setCkMsg("Warm-up failed: " + errorMessage(e)));

  const toggleSchedule = async () => {
    setSchedBusy(true);
    try {
      if (sched) { await api.deleteSchedule(sched.id); setSched(null); }
      else { setSched(await api.addSchedule({ profileId: p.id, everyHours: 12, minutes: 5, preset })); }
    } catch (e) { setCkMsg("Schedule failed: " + errorMessage(e)); }
    setSchedBusy(false);
  };
  const patchSchedule = async (patch: Partial<Schedule>) => {
    if (!sched) return;
    setSchedBusy(true);
    try { setSched(await api.updateSchedule(sched.id, patch)); }
    catch (e) { setCkMsg("Schedule failed: " + errorMessage(e)); }
    setSchedBusy(false);
  };

  const importCookies = async (file: File | undefined) => {
    if (!file) return;
    setCkBusy("import"); setCkMsg("");
    try {
      const res = await api.importCookies(p.id, await file.text(), ckReplace);
      setCkMsg(`Imported ${res.imported} cookie${res.imported === 1 ? "" : "s"} from ${file.name}.`);
    } catch (e) { setCkMsg("Import failed: " + errorMessage(e)); }
    setCkBusy("");
  };

  return (
    // A full page, not a 580px drawer. Every field here describes one device and
    // several of them only make sense read together (UA against platform, screen
    // against the GPU, locale against the proxy's country) - squeezing that into
    // a side panel on a wide screen was the wrong shape for the job.
    <div className="ps-overlay ps-page" onClick={close}>
      <div className="ps-editor" aria-busy={saving} onClick={e => e.stopPropagation()}>
        <div className="ps-editor-h" inert={saving || conflicts.length > 0}>
          <div className="ps-editor-id">
            <div className="ps-eyebrow">{p._new ? "New profile" : "Edit profile"}</div>
            <input className="ps-in ps-editor-name" value={p.name ?? ""}
                   onChange={e => set({ name: e.target.value })}
                   placeholder="Untitled profile" aria-label="Profile name" />
          </div>
          <select className="ps-in ps-editor-folder" value={p.folder || ""}
                  onChange={e => set({ folder: e.target.value })} aria-label="Folder">
            <option value="">No folder</option>
            {[...new Set([p.folder, ...folders].filter(f => f && f !== "All profiles"))]
              .map(f => <option key={f} value={f}>{f}</option>)}
          </select>
          <div style={{ flex: 1 }} />
          <button className="ps-x" disabled={saving} aria-label="Close editor" onClick={close}><X size={18} /></button>
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

        <div className="ps-tabs" inert={saving || conflicts.length > 0}>
          {["general", "fingerprint", "proxy", "bookmarks", "advanced"].map(t => (
            <button key={t} className={"ps-tab" + (tab === t ? " active" : "")} onClick={() => setTab(t)}>{t}</button>
          ))}
        </div>

        <div className="ps-editor-main" inert={saving || conflicts.length > 0}>
          <div className="ps-editor-body">
          {tab === "general" && (
            <>
              {/* Name and folder are in the page header - they identify the
                  profile, so they stay visible whichever tab is open. */}
              <Field label="Tags (comma separated)">
                <input className="ps-in" value={p.tags.join(", ")} onChange={e => set({ tags: e.target.value.split(",").map(s => s.trim()).filter(Boolean) })} placeholder="client-a, priority" />
              </Field>
              <Field label="Notes"><textarea className="ps-in" rows={3} value={p.notes} onChange={e => set({ notes: e.target.value })} placeholder="Anything worth remembering about this account…" /></Field>
              <Field label="Launch engine">
                <select className="ps-in" value={p.engine || ""} onChange={e => set({ engine: e.target.value })}>
                  {/* Only offered while unset — we don't know the server's default,
                      so let it decide rather than naming an engine we're guessing. */}
                  {!p.engine && <option value="">— engine default —</option>}
                  {ENGINE_ORDER.map(en => <option key={en} value={en}>{ENGINE_META[en]!.label}{ENGINE_META[en]!.recommended ? "  ★ recommended" : ""}</option>)}
                </select>
              </Field>
              <div className="ps-hint"><Blocks size={12} /> Numera manages this profile; the engine renders it. Manage engines in the <b style={{ color: T.text }}>Engines</b> tab.</div>
            </>
          )}

          {tab === "fingerprint" && (
            <>
              {/* Said once, at the top of the tab whose every control it
                  affects - not repeated beside each field, and not hidden. */}
              {generatesOwnIdentity(p.engine) && (
                <div className="ps-hint"><Fingerprint size={12} />
                  {" "}{ENGINE_META[p.engine ?? ""]?.label || p.engine} generates its own Firefox
                  fingerprint and is given only the OS, the language and the proxy below.
                  The rest of this tab is stored and takes effect on a Chromium engine.
                </div>
              )}
              <SectionLabel icon={<MonitorSmartphone size={13} />} text="Operating system & browser" />
              <Row>
                <Pick label="Operating system" opts={OS_LIST} val={p.os} on={changeOS} />
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
              {/* The vendor string is derived from the renderer on save, but a
                  driver can report a pairing the table does not know, so it is
                  editable rather than inferred-only. */}
              <Field label="WebGL vendor">
                <input className="ps-in mono" value={p.webglVendor || ""}
                       onChange={e => set({ webglVendor: e.target.value })}
                       placeholder="Google Inc. (NVIDIA)" />
              </Field>

              <SectionLabel icon={<Fingerprint size={13} />} text="Spoofing modes" />
              <Row><Pick label="Canvas" opts={MODES.canvas} val={p.canvas} on={v => set({ canvas: v })} /><Pick label="WebRTC" opts={MODES.webrtc} val={p.webrtc} on={v => set({ webrtc: v })} /></Row>
              <Row><Pick label="Audio context" opts={MODES.audio} val={p.audio} on={v => set({ audio: v })} /><Pick label="Fonts" opts={MODES.fonts} val={p.fonts} on={v => set({ fonts: v })} /></Row>
              {/* `to_engine_profile` has always mapped these three; the editor
                  simply never offered them, so every profile silently took the
                  default. Reading a value the user cannot set is the same class
                  of gap as a control that does nothing. */}
              <Row>
                <Pick label="Client rects" opts={MODES.clientRects} val={p.clientRects || "Noise"} on={v => set({ clientRects: v })} />
                <Pick label="WebGL image" opts={MODES.webglImage} val={p.webglImage || "Noise"} on={v => set({ webglImage: v })} />
                <Pick label="Media devices" opts={MODES.mediaDevices} val={p.mediaDevices || "Masked"} on={v => set({ mediaDevices: v })} />
              </Row>

              {p.fonts !== "Real" && p.fonts !== "real" && (
                <Field label="Font list (one per line)">
                  <textarea className="ps-in mono" rows={5}
                    value={p.fontList || ""}
                    onChange={e => set({ fontList: e.target.value })}
                    placeholder="Leave empty to use the OS default set" />
                  <div className="ps-hint"><Type size={12} /> What a font-probing script is told
                    the machine has. Empty means the archetype's own list for {p.os} — which is
                    usually what you want, since an unusual set is itself a signal.</div>
                </Field>
              )}

              <SectionLabel icon={<Globe size={13} />} text="Geo & language" />
              <Row>
                <Field label={<>Locale &amp; languages <IpSwitch p={p} set={set} signal="languages" /></>}>
                  <select className="ps-in" value={p.locale} disabled={(p.ipBased || []).includes("languages")}
                    onChange={e => changeLocale(e.target.value)}>{Object.keys(LOCALES).map(l => <option key={l}>{l}</option>)}</select>
                </Field>
                <TimezoneField p={p} set={set} />
                <Field label={<>Geolocation <IpSwitch p={p} set={set} signal="geolocation" /></>}>
                  <div className="ps-seg">{MODES.geo.map(o =>
                    <button key={o} className={p.geo === o ? "on" : ""} onClick={() => set({ geo: o })}>{o}</button>)}</div>
                </Field>
              </Row>
              <Row>
                <Field label={<span><Camera size={11} /> Cameras</span>}><input type="number" className="ps-in" value={p.mediaVideo} onChange={e => set({ mediaVideo: Number(e.target.value) })} /></Field>
                <Field label={<span><Mic size={11} /> Microphones</span>}><input type="number" className="ps-in" value={p.mediaAudio} onChange={e => set({ mediaAudio: Number(e.target.value) })} /></Field>
                <Field label={<span><Volume2 size={11} /> Speakers</span>}><input type="number" className="ps-in" value={p.mediaSpeakers ?? 1} onChange={e => set({ mediaSpeakers: Number(e.target.value) })} /></Field>
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
                <button className={"ps-toggle" + (p.proxy ? " on" : "")} onClick={() => set(p.proxy ? { proxy: null } : { proxy: { type: "HTTP", host: "", port: "", user: "", pass: "", country: "US" }, _passwordSet: true })}>
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
                    {/* `COUNTRIES`, not `Object.keys(COUNTRY_LOCALE)`: that map
                        carries `UK` as an alias of `GB` so imported proxy lists
                        resolve, and listing both put one country in the picker
                        twice. Picking the alias stores a value every later
                        comparison of `proxy.country` reads as a different place.
                        The bulk dialog was fixed for this; this copy was not.
                        A stored `UK` still shows, because the current value is
                        added to the list below rather than snapping to `GB`. */}
                    <Field label="Country"><select className="ps-in" value={p.proxy.country ?? undefined} onChange={e => setProxy({ country: e.target.value })}>{[...new Set([p.proxy.country, ...COUNTRIES].filter(Boolean))].map(c => <option key={c}>{c}</option>)}</select></Field>
                  </Row>
                  <Row>
                    <Field label="Host"><input className="ps-in mono" value={p.proxy.host} onChange={e => setProxy({ host: e.target.value })} placeholder="host.proxy.io" /></Field>
                    <Field label="Port"><input className="ps-in mono" value={p.proxy.port} onChange={e => setProxy({ port: e.target.value })} placeholder="8080" /></Field>
                  </Row>
                  <Row>
                    <Field label="Username"><input className="ps-in mono" value={p.proxy.user ?? undefined} onChange={e => setProxy({ user: e.target.value })} /></Field>
                    <Field label="Password"><input className="ps-in mono" type="password" value={p.proxy.pass ?? ""} onChange={e => setProxy({ pass: e.target.value })} /></Field>
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

          {tab === "bookmarks" && (
            <>
              <SectionLabel icon={<Bookmark size={13} />} text="Bookmarks bar" />
              <Field label="One per line — “Name | https://url”, or just the URL">
                <textarea className="ps-in mono" rows={10} value={p.bookmarks || ""}
                  onChange={e => set({ bookmarks: e.target.value })}
                  placeholder={"Gmail | https://mail.google.com\nhttps://github.com"} />
              </Field>
              <div className="ps-hint"><Bookmark size={12} /> Written into the profile's own Chromium
                bookmark file just before it launches, and <b style={{ color: T.text }}>merged</b> — anything
                bookmarked from inside the browser stays, and re-launching doesn't duplicate anything.
                An account with an empty bookmark bar reads as new; a handful of plausible ones does not.</div>
              <div className="ps-hint"><Blocks size={12} /> Chromium engines only. Camoufox is Firefox and
                keeps its bookmarks in a database this doesn't write.</div>
            </>
          )}

          {tab === "advanced" && (
            <>
              <Field label="Startup URLs (one per line)"><textarea className="ps-in mono" rows={3} value={p.startupUrls} onChange={e => set({ startupUrls: e.target.value })} placeholder={"https://facebook.com\nhttps://business.facebook.com"} /></Field>

              <SectionLabel icon={<Terminal size={13} />} text="Launch" />
              <Field label="Extra Chromium arguments (one per line)">
                <textarea className="ps-in mono" rows={3} value={p.launchArgs || ""}
                  onChange={e => set({ launchArgs: e.target.value })}
                  placeholder={"--mute-audio\n--disable-background-timer-throttling"} />
              </Field>
              <div className="ps-hint"><Terminal size={12} /> Appended to the flags Numera already sets.
                They land on the real command line, so a bad one stops the browser starting — which is the
                honest outcome, since the alternative is a flag you believe is active and isn't.</div>

              <div className="ps-keep" style={{ marginTop: 4 }}>
                {SWITCHABLE.map(([key, label, why]) => {
                  const on = !(p.switchedOff || []).includes(key);
                  return (
                    <button key={key} type="button" className={"ps-keep-item" + (on ? " on" : "")}
                      title={why}
                      onClick={() => set({ switchedOff: on
                        ? [...(p.switchedOff || []), key]
                        : (p.switchedOff || []).filter(k => k !== key) })}>
                      <span className="ps-keep-box">{on && <Check size={11} />}</span>
                      <span>{label}</span>
                    </button>
                  );
                })}
              </div>
              <div className="ps-hint"><Terminal size={12} /> Ticked is Chromium's own default. Unticked
                adds the flags that switch it off, so a profile makes less background noise of its own -
                the same thing you could type above, without having to remember which flag it was.</div>

              <Field label={<span><Wifi size={11} /> DNS resolver</span>}>
                <select className="ps-in" value={DOH.some(d => d.v === (p.dns || "")) ? (p.dns || "") : "custom"}
                  onChange={e => set({ dns: e.target.value === "custom" ? "https://" : e.target.value })}>
                  {DOH.map(d => <option key={d.v} value={d.v}>{d.label}</option>)}
                  <option value="custom">Custom DoH endpoint…</option>
                </select>
              </Field>
              {!DOH.some(d => d.v === (p.dns || "")) && (
                <Field label="DoH endpoint URL">
                  <input className="ps-in mono" value={p.dns || ""} onChange={e => set({ dns: e.target.value })}
                    placeholder="https://doh.example.net/dns-query" />
                </Field>
              )}
              <div className="ps-hint"><Wifi size={12} /> DNS-over-HTTPS, not a plain nameserver — Chromium
                has no flag for one, and it's the stronger answer anyway: lookups leave over HTTPS instead of
                in the clear beside the proxy, where they'd name every site the profile visits.</div>
              <Field label="WebGL vendor"><input className="ps-in mono" value={p.webglVendor} onChange={e => set({ webglVendor: e.target.value })} /></Field>
              <div className="ps-hint"><Fingerprint size={12} /> Seed & canvas noise are managed by the engine and stay stable across launches for this profile.</div>

              <SectionLabel icon={<Blocks size={13} />} text="Extensions" />
              <Field label="Unpacked extension folders (one per line)">
                <textarea className="ps-in mono" rows={2} value={p.extensions || ""}
                  onChange={e => set({ extensions: e.target.value })}
                  placeholder={"C:\\tools\\metamask\n/home/me/ext/ublock"} />
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

              <SectionLabel icon={<Database size={13} />} text="What carries over between launches" />
              <div className="ps-keep">
                {KEEPABLE.map(([key, label, why]) => {
                  const keeps = !(p.clearOnStart || []).includes(key);
                  return (
                    <button key={key} type="button" className={"ps-keep-item" + (keeps ? " on" : "")}
                      title={why}
                      onClick={() => set({ clearOnStart: keeps
                        ? [...(p.clearOnStart || []), key]
                        : (p.clearOnStart || []).filter(k => k !== key) })}>
                      <span className="ps-keep-box">{keeps && <Check size={11} />}</span>
                      <span>{label}</span>
                    </button>
                  );
                })}
              </div>
              <div className="ps-hint"><Database size={12} /> Unticked stores are wiped just before the
                browser opens, not after it closes — so a session that ends in a crash still starts clean.
                Everything ticked is the default and the behaviour every existing profile already has.</div>

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
          <ProfileSummary p={p} issues={iss} score={score} />
        </div>

        {saveMessage && (
          <div role="status" style={{ padding: "12px 24px", borderTop: `1px solid ${T.line}`, fontSize: 13,
            maxHeight: "35vh", overflowY: "auto", flexShrink: 0 }}>
            <div>{saveMessage}</div>
            {conflicts.map(path => (
              <div key={path.join(".")} role="group" aria-label={`Resolve ${path.join(".")} conflict`}
                style={{ display: "flex", alignItems: "center", gap: 8, flexWrap: "wrap", marginTop: 8 }}>
                <strong>{path.join(".")}</strong>
                <span style={{ flex: "1 1 160px", overflowWrap: "anywhere" }}>
                  My edit: {conflictValue(p, path)} · Latest: {conflictValue(base, path)}
                </span>
                <button className="ps-btn ghost sm" onClick={() => resolve(path, false)}>Keep my edit</button>
                <button className="ps-btn ghost sm" onClick={() => resolve(path, true)}>Use latest</button>
              </div>
            ))}
          </div>
        )}
        <div className="ps-drawer-foot">
          <button className="ps-btn ghost" disabled={saving} onClick={close}>Cancel</button>
          <button className="ps-btn primary" disabled={!p.name || saving || conflicts.length > 0} onClick={submit}>
            <Check size={15} /> {saving ? "Saving…" : p._new ? "Create profile" : "Save changes"}
          </button>
        </div>
      </div>
    </div>
  );
}

export { Editor };
export { ProbePanel } from "./ProbePanel";
