/* The shared proxy pool: import, health-check, assign. */

import React, { useEffect, useState } from "react";
import { ArrowRight, Check, Circle, Globe, Plus, RefreshCw, Server, ShieldAlert, Trash2, Upload, Wifi, X } from "lucide-react";
import { api } from "../api";
import { T } from "../theme";
import { toast } from "../toast";
import { Field } from "../ui";

/* ---- Proxies view: a managed pool (import, test, assign) --------- */
function ProxyStatus({ p }) {
  const s = p.status;
  if (s === "ok") return <span className="ps-pill ok" title={p.lastCheck?.ip || ""}><Check size={12} /> {p.lastCheck?.country || "OK"}{p.lastCheck?.latencyMs ? ` · ${p.lastCheck.latencyMs}ms` : ""}</span>;
  if (s === "failed") return <span className="ps-pill warn" title={p.lastCheck?.error || ""}><ShieldAlert size={12} /> Failed</span>;
  if (s === "skipped") return <span className="ps-pill" title={p.lastCheck?.error || ""} style={{ background: T.violetDim, color: T.lilac }}><Circle size={10} /> SOCKS</span>;
  return <span className="ps-pill" style={{ color: T.dim }}><Circle size={10} /> Untested</span>;
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

export { ProxyStatus, ImportProxiesModal, ProxiesView };
