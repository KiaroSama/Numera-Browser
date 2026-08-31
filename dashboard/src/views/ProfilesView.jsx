/* The profile list: search, filter, select, run, edit. */

import React, { useEffect, useState } from "react";
import { Copy, Download, Filter, Flame, FolderTree, Layers, Pencil, Play, Plus, Search, ShieldAlert, ShieldCheck, Square, Trash2, Wifi, WifiOff, X } from "lucide-react";
import { api } from "../api";
import { T } from "../theme";
import { ENGINE_META, ENGINE_ORDER, OS_LIST, coherence } from "../deviceData";
import { toast } from "../toast";
import { newProfile } from "../ui";

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

export { ProfilesView };
