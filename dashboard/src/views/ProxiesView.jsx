/* The shared proxy pool: import, edit, health-check, export, assign.
 *
 * The table is the working surface, so it carries what you need to act on a
 * proxy without opening anything: name, status, how many profiles use it, type,
 * host, port and credentials. Selection drives the bulk bar — testing or
 * deleting forty proxies one row at a time is not a workflow.
 *
 * Everything destructive asks first, and says what it is about to destroy. */

import { useEffect, useMemo, useState } from "react";
import {
  Archive, ArchiveRestore, ArrowRight, Check, Circle, Copy, Download, Eye, EyeOff,
  Globe, Pencil, Plus, Repeat,
  RefreshCw, Server, ShieldAlert, Trash2, Upload, Wifi, X,
} from "lucide-react";
import { api } from "../api";
import { T } from "../theme";
import { toast } from "../toast";
import { ConfirmModal, Field, Row } from "../ui";

const TYPES = ["HTTP", "HTTPS", "SOCKS5", "SOCKS4"];

function ProxyStatus({ p }) {
  const s = p.status;
  if (s === "ok") return <span className="ps-pill ok" title={p.lastCheck?.ip || ""}><Check size={12} /> {p.lastCheck?.country || "OK"}{p.lastCheck?.latencyMs ? ` · ${p.lastCheck.latencyMs}ms` : ""}</span>;
  if (s === "failed") return <span className="ps-pill warn" title={p.lastCheck?.error || ""}><ShieldAlert size={12} /> Failed</span>;
  // "skipped" means the check could not run at all, not "we did not try".
  // SOCKS4 *is* health-checked - the relay speaks SOCKS4a - so this no longer
  // covers it; the comment said otherwise for as long as that was true.
  if (s === "skipped") return <span className="ps-pill" title={p.lastCheck?.error || ""} style={{ background: T.violetDim, color: T.lilac }}><Circle size={10} /> N/A</span>;
  return <span className="ps-pill" style={{ color: T.dim }}><Circle size={10} /> Untested</span>;
}

/* A password is worth showing on demand and worth not shouting by default:
 * this table is the kind of thing people screen-share. */
function Secret({ value }) {
  const [shown, setShown] = useState(false);
  if (!value) return <span className="dim">—</span>;
  return (
    <span className="ps-secret">
      <code className="ps-mono">{shown ? value : "•".repeat(Math.min(value.length, 10))}</code>
      <button className="ps-iconbtn xs" title={shown ? "Hide" : "Show"} onClick={() => setShown(s => !s)}>
        {shown ? <EyeOff size={12} /> : <Eye size={12} />}
      </button>
      <button className="ps-iconbtn xs" title="Copy"
              onClick={() => { navigator.clipboard?.writeText(value); toast.success("Copied."); }}>
        <Copy size={12} />
      </button>
    </span>
  );
}

function EditProxyModal({ proxy, onClose, onSave }) {
  const [p, setP] = useState({ ...proxy });
  const [busy, setBusy] = useState(false);
  const set = (patch) => setP(x => ({ ...x, ...patch }));
  const moved = ["host", "port", "user", "pass", "type"].some(k => (p[k] || "") !== (proxy[k] || ""));

  const submit = async () => {
    if (!String(p.host || "").trim()) { toast.error("Host cannot be empty."); return; }
    if (!String(p.port || "").trim()) { toast.error("Port cannot be empty."); return; }
    setBusy(true);
    try {
      await onSave({
        label: p.label || null, type: p.type, host: p.host.trim(), port: p.port,
        user: p.user || null, pass: p.pass || null, country: (p.country || "").toUpperCase() || null,
        rotateUrl: (p.rotateUrl || "").trim() || null,
      });
      onClose();
    } catch (e) { toast.error("Could not save: " + e.message); setBusy(false); }
  };

  return (
    <div className="ps-overlay center" onClick={busy ? undefined : onClose}>
      <div className="ps-modal ps-pop" onClick={e => e.stopPropagation()} style={{ maxWidth: 560 }}>
        <div className="ps-drawer-h">
          <div><div className="ps-eyebrow">Proxy pool</div><div className="ps-drawer-title">Edit proxy</div></div>
          <button className="ps-x" onClick={onClose} disabled={busy}><X size={18} /></button>
        </div>
        <div style={{ padding: 20 }}>
          <Field label="Name"><input className="ps-in" value={p.label || ""} placeholder="e.g. IT 1"
            onChange={e => set({ label: e.target.value })} /></Field>
          <Row>
            <Field label="Type">
              <select className="ps-in" value={p.type || "HTTP"} onChange={e => set({ type: e.target.value })}>
                {TYPES.map(t => <option key={t} value={t}>{t}</option>)}
              </select>
            </Field>
            <Field label="Country"><input className="ps-in mono" value={p.country || ""} maxLength={2}
              placeholder="US" onChange={e => set({ country: e.target.value })} /></Field>
          </Row>
          <Row>
            <Field label="Host"><input className="ps-in mono" value={p.host || ""}
              onChange={e => set({ host: e.target.value })} /></Field>
            <Field label="Port"><input className="ps-in mono" value={p.port || ""}
              onChange={e => set({ port: e.target.value })} /></Field>
          </Row>
          <Row>
            <Field label="Login"><input className="ps-in mono" value={p.user || ""}
              onChange={e => set({ user: e.target.value })} /></Field>
            <Field label="Password"><input className="ps-in mono" value={p.pass || ""}
              onChange={e => set({ pass: e.target.value })} /></Field>
          </Row>
          <Field label="Change-IP URL (rotating proxies)">
            <input className="ps-in mono" value={p.rotateUrl || ""}
              placeholder="https://provider.example/api/rotate?key=…"
              onChange={e => set({ rotateUrl: e.target.value })} />
          </Field>
          <div className="ps-hint"><Repeat size={12} /> Set this and the row grows a
            <b style={{ color: T.text }}> rotate</b> button. Numera fetches it, then re-tests the
            proxy and compares the exit IP — a provider that answers 200 and does nothing is
            reported as not moved, because the alternative is a button that lies. Most providers
            rate-limit the endpoint, so a rotate within 20 seconds of the last one is refused.</div>

          {moved && (
            <div className="ps-hint"><Wifi size={12} /> The address changed, so the stored health
              result no longer describes this proxy — it goes back to <b style={{ color: T.text }}>untested</b>.</div>
          )}
        </div>
        <div className="ps-drawer-foot">
          <button className="ps-btn ghost" onClick={onClose} disabled={busy}>Cancel</button>
          <button className="ps-btn primary" onClick={submit} disabled={busy}>
            {busy ? <><RefreshCw size={15} className="ps-spin" /> Saving…</> : <><Check size={15} /> Save</>}
          </button>
        </div>
      </div>
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

function ProxiesView({ profiles, live, refresh }) {
  const [pool, setPool] = useState([]);
  const [importOpen, setImportOpen] = useState(false);
  const [editing, setEditing] = useState(null);
  const [confirm, setConfirm] = useState(null);   // {title, body, label, run}
  const [sel, setSel] = useState(new Set());
  const [testing, setTesting] = useState(new Set());
  const [busy, setBusy] = useState("");
  // All / Errors / Archive. A dead proxy sitting between two live ones while
  // you assign is the workflow problem this splits up.
  const [tab, setTab] = useState("all");
  const [rotating, setRotating] = useState(new Set());

  const load = async () => {
    try {
      const items = await api.listProxies();
      setPool(items);
      // Drop selections for rows that are gone, or "3 selected" outlives them.
      setSel(s => new Set([...s].filter(id => items.some(p => p.id === id))));
    } catch { /* offline */ }
  };
  useEffect(() => { if (live) load(); }, [live]);

  const selected = useMemo(() => pool.filter(p => sel.has(p.id)), [pool, sel]);
  const TABS = [
    ["all", "All", p => !p.archived],
    ["errors", "Errors", p => !p.archived && p.status === "failed"],
    ["archive", "Archive", p => p.archived],
  ];
  // One filter feeds both the list and the tab counts. Two would drift, and the
  // number above a list that disagrees with the rows is worse than no number.
  const view = useMemo(
    () => pool.filter(TABS.find(t => t[0] === tab)[2]),
    [pool, tab]);

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

  const toggle = (id) => setSel(s => { const n = new Set(s); n.has(id) ? n.delete(id) : n.add(id); return n; });
  // Scoped to the tab: "select all" on the Errors tab that silently grabbed the
  // archived rows too would make the next bulk action a surprise.
  const allSel = view.length > 0 && view.every(p => sel.has(p.id));
  const toggleAll = () => setSel(allSel ? new Set() : new Set(view.map(p => p.id)));

  const testOne = async (id) => {
    setTesting(s => new Set(s).add(id));
    try { await api.testProxy(id); await load(); }
    catch (e) { toast.error("Test failed: " + e.message); }
    setTesting(s => { const n = new Set(s); n.delete(id); return n; });
  };

  const testMany = async (ids) => {
    setBusy("test");
    setTesting(new Set(ids));
    try {
      const r = await api.testProxies(ids);
      const bad = r.tested - r.ok;
      toast[bad ? "info" : "success"](`Tested ${r.tested}: ${r.ok} live${bad ? `, ${bad} not` : ""}.`);
      await load();
    } catch (e) { toast.error("Test failed: " + e.message); }
    setTesting(new Set());
    setBusy("");
  };

  // The engine re-checks after hitting the endpoint, so `rotated` means the exit
  // IP genuinely moved. Reporting the HTTP status instead would be the button
  // that looks like it worked and did not.
  const said = (r) => r.rotated
    ? toast.success(`New exit IP: ${r.previousIp ? `${r.previousIp} → ` : ""}${r.ip}`)
    : toast.error(r.error || "The exit IP did not change.");

  const rotateOne = async (id) => {
    setRotating(s => new Set(s).add(id));
    try { said(await api.rotateProxy(id)); await load(); }
    catch (e) { toast.error("Rotate failed: " + e.message); }
    setRotating(s => { const n = new Set(s); n.delete(id); return n; });
  };

  const rotateMany = async (rows) => {
    const ids = rows.filter(p => p.rotateUrl).map(p => p.id);
    if (!ids.length) return toast.error("None of these have a change-IP URL set.");
    setBusy("rotate");
    setRotating(new Set(ids));
    try {
      const r = await api.rotateProxies(ids);
      toast[r.rotated ? "success" : "error"](
        `${r.rotated} of ${r.tried} moved to a new IP.`);
      await load();
    } catch (e) { toast.error("Rotate failed: " + e.message); }
    setRotating(new Set());
    setBusy("");
  };

  const archive = async (rows, archived) => {
    try {
      await api.archiveProxies(rows.map(p => p.id), archived);
      toast.success(`${archived ? "Archived" : "Restored"} ${rows.length} prox${rows.length === 1 ? "y" : "ies"}.`);
      setSel(new Set());
      await load();
    } catch (e) { toast.error(e.message); }
  };

  const askDelete = (rows) => {
    const used = rows.reduce((n, p) => n + (p.profiles || 0), 0);
    setConfirm({
      title: rows.length === 1 ? "Delete this proxy?" : `Delete ${rows.length} proxies?`,
      danger: true,
      label: <><Trash2 size={15} /> Delete {rows.length > 1 ? rows.length : ""}</>,
      body: (
        <>
          {rows.length === 1
            ? <>Removing <b style={{ color: T.text }}>{rows[0].label || `${rows[0].host}:${rows[0].port}`}</b> from the pool.</>
            : <>Removing <b style={{ color: T.text }}>{rows.length}</b> proxies from the pool.</>}
          {used > 0 && (
            <div style={{ marginTop: 10, color: T.amber }}>
              <ShieldAlert size={13} style={{ verticalAlign: -2 }} /> {used} profile{used === 1 ? "" : "s"} currently
              use{used === 1 ? "s" : ""} {rows.length === 1 ? "it" : "them"}. Those profiles keep their proxy
              settings — only the pool entry goes.
            </div>
          )}
          <div style={{ marginTop: 10 }}>This cannot be undone, but you can re-import from an export.</div>
        </>
      ),
      run: async () => {
        const ids = rows.map(p => p.id);
        const r = await api.deleteProxies(ids);
        toast.success(`Deleted ${r.deleted} prox${r.deleted === 1 ? "y" : "ies"}.`);
        await load();
      },
    });
  };

  const exportRows = async (rows) => {
    try {
      const r = await api.exportProxies(rows.map(p => p.id));
      const url = URL.createObjectURL(new Blob([r.text], { type: "text/plain" }));
      const a = document.createElement("a");
      a.href = url;
      a.download = `numera-proxies-${r.count}.txt`;
      a.click();
      URL.revokeObjectURL(url);
      toast.success(`Exported ${r.count} prox${r.count === 1 ? "y" : "ies"}.`);
    } catch (e) { toast.error("Export failed: " + e.message); }
  };

  const assign = () => setConfirm({
    title: "Assign proxies to profiles?",
    label: <><ArrowRight size={15} /> Assign</>,
    body: <>Spreading {pool.length} prox{pool.length === 1 ? "y" : "ies"} round-robin across
      all {profiles.length} profile{profiles.length === 1 ? "" : "s"}. Each profile's locale and
      timezone will be aligned to its proxy's country.</>,
    run: async () => {
      const r = await api.assignProxies();
      toast.success(`Assigned proxies to ${r.assigned} profile(s).`);
      await refresh();
      await load();
    },
  });

  const okCount = pool.filter(p => p.status === "ok").length;
  const n = sel.size;

  return (
    <div className="ps-scroll">
      <div className="ps-toolbar" style={{ paddingLeft: 0, paddingRight: 0 }}>
        <div style={{ fontSize: 13, fontWeight: 600, color: T.muted }}>
          Proxy pool <span className="ps-mono dim">· {pool.length} total{okCount ? ` · ${okCount} live` : ""}</span>
        </div>
        <div style={{ flex: 1 }} />
        <button className="ps-btn ghost sm" disabled={!pool.length || busy === "test"} onClick={() => testMany(pool.map(p => p.id))}>
          {busy === "test" && !n ? <><RefreshCw size={13} className="ps-spin" /> Testing…</> : <><Wifi size={13} /> Test all</>}
        </button>
        <button className="ps-btn ghost sm" disabled={!pool.length} onClick={() => exportRows(pool)}>
          <Download size={13} /> Export
        </button>
        <button className="ps-btn ghost sm" disabled={!pool.length || !profiles.length} onClick={assign}>
          <ArrowRight size={13} /> Assign to profiles
        </button>
        <button className="ps-btn primary sm" onClick={() => setImportOpen(true)}><Plus size={13} /> Import proxies</button>
      </div>

      <div className="ps-tabs" style={{ marginBottom: 12 }}>
        {TABS.map(([key, label, keep]) => {
          const count = pool.filter(keep).length;
          return (
            <button key={key} className={"ps-tab" + (tab === key ? " active" : "")}
              onClick={() => setTab(key)}>
              {label}{count ? <span className="ps-tab-n">{count}</span> : null}
            </button>
          );
        })}
      </div>

      {n > 0 && (
        // The same bar the Profiles page uses, so a selection means the same
        // thing and looks the same wherever you make one.
        <div className="ps-bulkbar ps-rise" style={{ marginBottom: 12 }}>
          <span>{n} selected</span>
          <button disabled={busy === "test"} onClick={() => testMany([...sel])}>
            {busy === "test" ? <><RefreshCw size={13} className="ps-spin" /> Testing…</> : <><Wifi size={13} /> Test</>}
          </button>
          <button disabled={busy === "rotate" || !selected.some(p => p.rotateUrl)}
            title={selected.some(p => p.rotateUrl) ? "" : "None of these have a change-IP URL"}
            onClick={() => rotateMany(selected)}>
            {busy === "rotate"
              ? <><RefreshCw size={13} className="ps-spin" /> Rotating…</>
              : <><Repeat size={13} /> Rotate</>}
          </button>
          <button onClick={() => exportRows(selected)}><Download size={13} /> Export</button>
          {tab === "archive"
            ? <button onClick={() => archive(selected, false)}><ArchiveRestore size={13} /> Restore</button>
            : <button onClick={() => archive(selected, true)}><Archive size={13} /> Archive</button>}
          <button className="danger" onClick={() => askDelete(selected)}><Trash2 size={13} /> Delete</button>
          <button onClick={() => setSel(new Set())}><X size={13} /> Clear</button>
        </div>
      )}

      {view.length === 0
        ? <div className="ps-empty"><Globe size={26} color={T.dim} /><div>No proxies yet.</div>
            <button className="ps-btn primary sm" onClick={() => setImportOpen(true)}><Plus size={14} /> Import proxies</button></div>
        : (
          <table className="ps-table">
            <thead>
              <tr>
                <th style={{ width: 34 }}>
                  <input type="checkbox" checked={allSel} onChange={toggleAll} aria-label="Select every proxy" />
                </th>
                <th>Name</th><th>Status</th><th>Profiles</th><th>Type</th>
                <th>Host</th><th>Port</th><th>Login</th><th>Password</th>
                <th style={{ width: 120, textAlign: "right" }}>Actions</th>
              </tr>
            </thead>
            <tbody>
              {view.map((x, i) => (
                <tr key={x.id} className={"ps-rise" + (sel.has(x.id) ? " sel" : "")}
                    style={{ animationDelay: `${Math.min(i, 12) * 28}ms` }}>
                  <td><input type="checkbox" checked={sel.has(x.id)} onChange={() => toggle(x.id)}
                             aria-label={`Select ${x.label || x.host}`} /></td>
                  {/* A proxy with no label still needs something to call it in a
                      confirmation dialog and in the row, so fall back through
                      country to the address rather than showing a dash. */}
                  <td>{x.label || x.country || <span className="dim">{x.host}</span>}</td>
                  <td><ProxyStatus p={x} /></td>
                  <td className="ps-mono dim">{x.profiles || 0}</td>
                  <td><span className="ps-os">{x.type}</span></td>
                  <td className="ps-mono">{x.host}</td>
                  <td className="ps-mono">{x.port}</td>
                  <td className="ps-mono">{x.user || <span className="dim">—</span>}</td>
                  <td><Secret value={x.pass} /></td>
                  <td>
                    <div className="ps-actions">
                      <button title="Test" className="run" disabled={testing.has(x.id)} onClick={() => testOne(x.id)}>
                        {testing.has(x.id) ? <RefreshCw size={13} className="ps-spin" /> : <Wifi size={13} />}
                      </button>
                      {x.rotateUrl && (
                        <button title="New exit IP" disabled={rotating.has(x.id)}
                          onClick={() => rotateOne(x.id)}>
                          <Repeat size={13} className={rotating.has(x.id) ? "ps-spin" : ""} />
                        </button>
                      )}
                      <button title={x.archived ? "Restore" : "Archive"}
                        onClick={() => archive([x], !x.archived)}>
                        {x.archived ? <ArchiveRestore size={13} /> : <Archive size={13} />}
                      </button>
                      <button title="Edit" onClick={() => setEditing(x)}><Pencil size={13} /></button>
                      <button title="Delete" className="del" onClick={() => askDelete([x])}><Trash2 size={13} /></button>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      <div className="ps-hint"><Globe size={12} /> Import a list, test which are live, then <b style={{ color: T.text }}>Assign to profiles</b> spreads them round-robin and aligns each profile's locale/timezone to its proxy country. SOCKS5 is health-checked through the same loopback relay the browser uses, so the result is the real exit IP.</div>

      {importOpen && <ImportProxiesModal onClose={() => setImportOpen(false)} onImport={async (text) => {
        const r = await api.addProxies(text); await load(); return r;
      }} />}
      {editing && <EditProxyModal proxy={editing} onClose={() => setEditing(null)}
        onSave={async (patch) => { await api.updateProxy(editing.id, patch); await load(); toast.success("Saved."); }} />}
      {confirm && <ConfirmModal
        title={confirm.title} body={confirm.body} confirmLabel={confirm.label}
        danger={confirm.danger} busy={busy === "confirm"}
        onCancel={() => setConfirm(null)}
        onConfirm={async () => {
          setBusy("confirm");
          try { await confirm.run(); setConfirm(null); }
          catch (e) { toast.error(e.message); }
          setBusy("");
        }} />}
    </div>
  );
}

export { ProxyStatus, ImportProxiesModal, EditProxyModal, ProxiesView };
