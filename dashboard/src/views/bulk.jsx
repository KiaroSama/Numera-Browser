/* Operations across many profiles at once: apply one change to a selection,
 * or generate a whole batch. */

import { useState } from "react";
import { Boxes, Check, Copy, Globe, Plus, RefreshCw, X } from "lucide-react";
import { T } from "../theme";
import { CHROME_VERSIONS, CORES, COUNTRIES, COUNTRY_LOCALE, ENGINE_META, ENGINE_ORDER, GPUS, LOCALES, OS_LIST, RAM, SCREENS, uaFor, vendorFor } from "../deviceData";
import { toast } from "../toast";
import { Field, Row } from "../ui";

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
          <Field label="Countries / locales (spread across)"><ChipPick options={COUNTRIES} value={countries} onToggle={toggle(setCountries)} /></Field>
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

export { ChipPick, SyncModal, BulkModal };
