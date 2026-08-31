/* The default extension set: install from a Web Store link, list what is
 * installed by name and logo, push the set onto existing profiles.
 *
 * Backed by `default_extensions` in /api/config — which already existed. This
 * page is the interface to it, not a new mechanism.
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

import React, { useEffect, useState } from "react";
import { Check, Download, Info, Plus, Puzzle, RefreshCw, ShieldAlert, Trash2, Upload, WifiOff } from "lucide-react";
import { api } from "../api";
import { T } from "../theme";
import { SectionHead } from "../ui";

/* One row of the extension table: the extension's own logo, its name, its folder.
 *
 * The icon comes from the engine, which reads it out of the unpacked manifest -
 * so it is the real thing the browser shows, not a stand-in. It can still be
 * missing (no manifest yet, a folder that moved, an extension that declares no
 * usable icon), and a broken image is worse than a glyph, so `bad` falls back. */
function ExtensionRow({ path, meta, index, busy, onRemove }) {
  const [bad, setBad] = useState(false);
  const label = meta?.name || path;
  return (
    <div className="ps-ext-row ps-rise" style={{ animationDelay: `${index * 45}ms` }}>
      <span className="ps-ext-ic">
        {meta?.icon && !bad
          ? <img className="ps-ext-logo" src={api.extensionIcon(path)} alt="" onError={() => setBad(true)} />
          : <Puzzle size={15} color={T.violet} />}
      </span>
      <span className="ps-ext-named" title={path}>
        <b>{label}</b>
        {meta?.name && <code className="ps-mono ps-ext-sub">{path}</code>}
      </span>
      <button className="ps-iconbtn" title={`Remove ${label}`} disabled={busy} onClick={onRemove}>
        <Trash2 size={14} />
      </button>
    </div>
  );
}

function ExtensionsView({ config, live, onSaved, profiles, refresh }) {
  const [items, setItems] = useState(() => config?.default_extensions || []);
  const [draft, setDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState("");
  const [applied, setApplied] = useState(null);   // {changed, skipped} after a run
  const [installed, setInstalled] = useState(""); // result of a Web Store install
  const [names, setNames] = useState({});         // folder path -> extension name

  // Re-sync when the engine hands us a config later than our first render.
  useEffect(() => { setItems(config?.default_extensions || []); }, [config]);

  // A store install lands in <data-dir>/extensions/<32-char id>, so a row would
  // otherwise be an unreadable path and nothing else. The engine has already read
  // every manifest; borrow the name and whether it has an icon worth drawing.
  const loadNames = async () => {
    if (!live) return;
    try {
      const r = await api.installedExtensions();
      setNames(Object.fromEntries((r.extensions || []).map(e => [e.path, e])));
    } catch { /* the list still works without names */ }
  };
  // Depend on `live`, never on `items`: `items` is an array whose identity
  // changes on every render, so listing it here re-fires the effect in a loop
  // that starves the fetch it is supposed to make. The list changes only through
  // save() and add(), and both call loadNames() themselves.
  useEffect(() => { loadNames(); }, [live]);

  const save = async (next) => {
    setErr(""); setItems(next);          // optimistic: the list is the source of truth on screen
    if (!live) return;
    setBusy(true);
    try {
      await api.setDefaultExtensions(next);
      loadNames();          // a folder added by hand has a name and a logo too
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
        loadNames();          // so the new row shows its name, not just the id
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
          <div className="ps-ext-head">Name</div>
          {items.map((path, i) => (
            <ExtensionRow key={path} path={path} meta={names[path]} index={i} busy={busy}
                          onRemove={() => save(items.filter(p => p !== path))} />
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

export { ExtensionRow, ExtensionsView };
