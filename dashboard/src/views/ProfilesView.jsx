/* The profile list: search, filter, select, run, edit. */

import { useEffect, useMemo, useState } from "react";
import { Copy, Download, Filter, Flame, FolderTree, Layers, Pencil, Play, Plus, Search, ShieldAlert, ShieldCheck, Square, Trash2, Wifi, WifiOff, X } from "lucide-react";
import { api } from "../api";
import { T } from "../theme";
import { ENGINE_META, ENGINE_ORDER, OS_LIST, coherence } from "../deviceData";
import { toast } from "../toast";
import { ConfirmModal, Identicon, newProfile } from "../ui";

/* Click the name to rename in place.
 *
 * Renaming used to mean opening the whole editor and saving it back, which is a
 * lot of ceremony for one word - and the editor writes every field, so a rename
 * re-saved the entire profile. This sends only the new name.
 *
 * Enter or blur commits, Escape abandons. An empty name is refused rather than
 * stored, because the list has nothing else to show for that row.
 */
function ProfileName({ p, live, setProfiles }) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(p.name);
  const [busy, setBusy] = useState(false);

  const commit = async () => {
    const name = draft.trim();
    setEditing(false);
    if (!name || name === p.name) { setDraft(p.name); return; }
    setBusy(true);
    // Optimistic: the row updates immediately and rolls back if the write fails,
    // so a rename never looks like it did nothing while the request is in flight.
    setProfiles(ps => ps.map(x => x.id === p.id ? { ...x, name } : x));
    try {
      if (live) await api.update({ ...p, name });
    } catch (e) {
      setProfiles(ps => ps.map(x => x.id === p.id ? { ...x, name: p.name } : x));
      setDraft(p.name);
      toast.error(e.message);
    } finally {
      setBusy(false);
    }
  };

  if (!editing) {
    return (
      <div className={"ps-pname editable" + (busy ? " busy" : "")} title="Click to rename"
           onClick={() => { setDraft(p.name); setEditing(true); }}>
        {p.name}<Pencil size={11} className="ps-pname-pen" />
      </div>
    );
  }
  return (
    <input className="ps-pname-in" value={draft} autoFocus
      onChange={e => setDraft(e.target.value)}
      onBlur={commit}
      onKeyDown={e => {
        if (e.key === "Enter") e.currentTarget.blur();
        if (e.key === "Escape") { setDraft(p.name); setEditing(false); }
      }} />
  );
}

/* One labelled row of filter chips.
 *
 * Module scope, not the render body. Declared inside ProfilesView it was a new
 * component type on every render, so React unmounted and remounted all five
 * rows on each keystroke in the search box - the filter popover lost focus and
 * any pressed state mid-typing. It closes over nothing, so moving it out is the
 * whole fix. */
function FilterRow({ label, options, value, onPick }) {
  return (
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
}

/* How many sessions a bulk action opens at once.
 *
 * Each one is a real browser, so `Promise.all` over a selection was not
 * concurrency, it was a fork bomb with a progress bar: 200 selected profiles
 * meant 200 Chromiums starting in the same instant. Four keeps a large
 * selection moving without the machine going under. */
const AT_ONCE = 4;

/* Run `task` over every item, at most `limit` in flight, settling all of them.
 *
 * `Promise.all` also abandons the remaining results the moment one rejects,
 * which is half of why a partial failure could be reported as a success. */
async function pool(items, limit, task) {
  const out = [];
  let next = 0;
  const worker = async () => {
    while (next < items.length) {
      const i = next++;
      try { await task(items[i]); out[i] = { ok: true, id: items[i] }; }
      catch (error) { out[i] = { ok: false, id: items[i], error }; }
    }
  };
  await Promise.all(Array.from({ length: Math.min(limit, items.length) }, worker));
  return out;
}

function ProfilesView({ filtered, profiles, query, setQuery, folder, sel, setSel, toggleSel, allSel, toggleRun, clone, setEditor, setProfiles, setSync, filters, setFilters, allTags, live, refresh, defaultEngine }) {
  const [filterOpen, setFilterOpen] = useState(false);
  // Deleting a profile destroys its browser session too, so it asks first -
  // it used to go the moment the bin was clicked, with no way back.
  const [confirm, setConfirm] = useState(null);
  const [deleting, setDeleting] = useState(false);
  const [busy, setBusy] = useState(false);   // a bulk session action is in flight

  /* SELECTION SCOPE - one policy, every bulk action.
   *
   * The selection is global: filtering changes what is *shown* and never what
   * is selected, and every action resolves the whole selection against the full
   * list. Export and Delete used to resolve against `filtered` while Launch and
   * Warm-up used the raw selection, so "3 selected" could export two profiles
   * and - with no undo and no explanation - delete one. */
  const byId = useMemo(() => new Map(profiles.map(p => [p.id, p])), [profiles]);
  const selected = useMemo(() => [...sel].map(id => byId.get(id)).filter(Boolean), [sel, byId]);

  /* Say what actually happened, and leave the failures selected so the action
   * can simply be pressed again. Reporting nothing was the warm-up bug. */
  const reportBulk = (verb, results, skipped, skipNote) => {
    const failed = results.filter(r => !r.ok);
    const done = results.length - failed.length;
    setSel(new Set(failed.map(r => r.id)));
    const note = skipped ? ` ${skipped} ${skipNote}.` : "";
    if (failed.length) {
      toast.error(`${verb} ${done} of ${results.length}. ${failed.length} failed: ${failed[0].error.message}.${note}`);
    } else {
      toast.success(`${verb} ${done} profile${done === 1 ? "" : "s"}.${note}`);
    }
  };
  const activeFilters = Object.values(filters).filter(Boolean).length;
  const setFilter = (key, value) => setFilters(f => ({ ...f, [key]: f[key] === value ? "" : value }));
  const clearFilters = () => setFilters({ status: "", engine: "", os: "", proxy: "", tag: "" });

  const askDelete = (rows) => {
    if (!rows.length) return;
    const running = rows.filter(p => p.status === "running").length;
    setConfirm({
      title: rows.length === 1 ? "Delete this profile?" : `Delete ${rows.length} profiles?`,
      body: (
        <>
          {rows.length === 1
            ? <>Deleting <b style={{ color: T.text }}>{rows[0].name}</b>.</>
            : <>Deleting <b style={{ color: T.text }}>{rows.length}</b> profiles.</>}
          <div style={{ marginTop: 10 }}>
            The browser session goes too - cookies, logins and local storage. There is no undo.
          </div>
          {running > 0 && (
            <div style={{ marginTop: 10, color: T.amber }}>
              <ShieldAlert size={13} style={{ verticalAlign: -2 }} /> {running} of them {running === 1 ? "is" : "are"} running right now.
            </div>
          )}
        </>
      ),
      run: async () => {
        const ids = rows.map(p => p.id);
        setSel(new Set());
        if (live) {
          // One request. This looped a DELETE per id, so removing a large
          // selection was that many round trips - and a failure part-way left
          // the toast claiming every one had gone.
          // Report what actually happened. The success toast used to fire
          // unconditionally - after a partial failure and even after the
          // request threw - which is the misleading success the bulk route was
          // added to remove.
          let removed;
          try {
            const res = await api.removeMany(ids);
            removed = res?.removed?.length ?? 0;
            if (res?.failed?.length) {
              toast.error(`${res.failed.length} could not be deleted: ${res.failed[0].error}`);
            }
            if (res?.missing?.length) {
              toast.error(`${res.missing.length} no longer existed.`);
            }
          } catch (e) {
            toast.error(e.message);
            await refresh();
            return;
          }
          await refresh();
          if (removed) {
            toast.success(`Deleted ${removed} profile${removed === 1 ? "" : "s"}.`);
          }
          return;
        } else {
          setProfiles(ps => ps.filter(p => !ids.includes(p.id)));
        }
        toast.success(`Deleted ${ids.length} profile${ids.length === 1 ? "" : "s"}.`);
      },
    });
  };

  /* Launch, and only launch.
   *
   * This called the row's toggle for every selected id, and the row toggle
   * *stops* a profile that is running - so pressing Launch on a mixed selection
   * started some sessions and killed the rest, losing whatever they held. An
   * already-running profile is now a skip: launching is idempotent. */
  const launchSelected = async () => {
    if (busy) return;
    if (!selected.length) { toast.info("Those profiles are no longer in the list."); return; }
    const idle = selected.filter(p => p.status !== "running");
    const skipped = selected.length - idle.length;
    if (!idle.length) {
      toast.info(`All ${skipped} selected profile${skipped === 1 ? " is" : "s are"} already running.`);
      return;
    }
    if (!live) {
      const ids = new Set(idle.map(p => p.id));
      setProfiles(ps => ps.map(p => ids.has(p.id) ? { ...p, status: "running", lastActive: "now" } : p));
      setSel(new Set());
      return;
    }
    setBusy(true);
    try {
      reportBulk("Launched", await pool(idle.map(p => p.id), AT_ONCE, id => api.launch(id)),
                 skipped, "already running");
    } finally { setBusy(false); }
    await refresh();
  };

  // Warm up every selected profile (live only — it opens real headless browsers).
  // Each rejection used to go into an empty catch while the selection was
  // cleared up front, so a run in which every profile failed - a busy session, a
  // locked credential, a missing engine - finished looking exactly like success,
  // with nothing left selected to retry.
  const warmSelected = async () => {
    if (busy) return;
    if (!live) { toast.info("Warm-up runs against the live engine — start `numera serve` first."); return; }
    if (!selected.length) { toast.info("Those profiles are no longer in the list."); return; }
    setBusy(true);
    try {
      reportBulk("Warmed up", await pool(selected.map(p => p.id), AT_ONCE, id => api.warmup(id, 5, "general")),
                 sel.size - selected.length, "no longer exist");
    } finally { setBusy(false); }
    await refresh();
  };

  // Export the selected profiles as one JSON file (config: fingerprint, proxy,
  // tags — not the browser session; use `numera backup` for a full session).
  const exportSelected = () => {
    const blob = new Blob([JSON.stringify(selected, null, 2)], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url; a.download = `numera-profiles-${selected.length}.json`; a.click();
    URL.revokeObjectURL(url);
  };

  // Pagination keeps the grid responsive when there are thousands of profiles.
  const PAGE = 100;
  const [page, setPage] = useState(0);
  useEffect(() => { setPage(0); }, [query, folder, filters.status, filters.engine, filters.os, filters.proxy]);
  const pageCount = Math.max(1, Math.ceil(filtered.length / PAGE));
  const cur = Math.min(page, pageCount - 1);
  const rows = filtered.slice(cur * PAGE, cur * PAGE + PAGE);

  return (
    <>
      <div className="ps-toolbar">
        <div className="ps-search">
          <Search size={15} color={T.dim} />
          <input placeholder="Search profiles, tags…" value={query} onChange={e => setQuery(e.target.value)} />
        </div>
        <div className="ps-crumb"><FolderTree size={13} /> {folder}</div>
        <div style={{ flex: 1 }} />
        {selected.length > 0 && (
          <div className="ps-bulkbar">
            {/* The count is what every button below acts on - the same resolved
                selection, filter or no filter. */}
            <span>{selected.length} selected</span>
            <button disabled={busy} onClick={launchSelected}><Play size={13} /> Launch</button>
            <button disabled={busy} onClick={warmSelected}><Flame size={13} /> Warm up</button>
            <button onClick={() => setSync(true)}><Copy size={13} /> Edit all</button>
            <button onClick={exportSelected}><Download size={13} /> Export</button>
            <button className="danger" onClick={() => askDelete(selected)}><Trash2 size={13} /> Delete</button>
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
                  {/* Adds or removes the rows on screen and leaves the rest of
                      the selection alone, because a filter never decides what is
                      selected - replacing the whole set here silently dropped
                      every selected profile the current filter was hiding. */}
                  <input type="checkbox" checked={allSel} onChange={() => setSel(s => {
                    const next = new Set(s);
                    for (const p of filtered) allSel ? next.delete(p.id) : next.add(p.id);
                    return next;
                  })} />
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
                        <Identicon id={p.id} />
                        <div>
                          <ProfileName p={p} live={live} setProfiles={setProfiles} />
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
                        <button title="Delete" className="del" onClick={() => askDelete([p])}><Trash2 size={13} /></button>
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
      {confirm && <ConfirmModal
        title={confirm.title} body={confirm.body} danger busy={deleting}
        confirmLabel={<><Trash2 size={15} /> Delete</>}
        onCancel={() => setConfirm(null)}
        onConfirm={async () => {
          setDeleting(true);
          try { await confirm.run(); setConfirm(null); } finally { setDeleting(false); }
        }} />}
    </>
  );
}

export { ProfilesView };
