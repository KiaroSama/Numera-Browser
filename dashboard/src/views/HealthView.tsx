import type { ReactNode } from "react";
import type { Grade, ProbeResult, Profile, ProfileId, ProfileHealth, ProfileViewProps } from "../types";
import { errorMessage } from "../types";
/* One row per profile: coherence, proxy, cached trust grade, schedule, and
 * which profiles share a fingerprint. */

import { useEffect, useState } from "react";
import { Activity, Check, Clock, Link2, RefreshCw, ShieldAlert, ShieldCheck, Sparkles } from "lucide-react";
import { api } from "../api";
import { T } from "../theme";
import { coherence } from "../deviceData";
import { toast } from "../toast";

function GradeBadge({ grade }: { grade?: string | undefined }) {
  if (!grade) return <span className="ps-mono dim">—</span>;
  const c = grade === "A" ? T.mint : grade === "B" ? T.lilac : grade === "C" ? T.amber : T.red;
  return <span className="ps-grade" style={{ color: c, borderColor: c + "55", background: c + "18" }}>{grade}</span>;
}

function HealthChip({ label, value, color }: { label: string; value: ReactNode; color?: string }) {
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
function fpSignature(p: Profile) {
  return [p.os, p.userAgent, p.screen, p.cores, p.memory, p.webglRenderer].join("|");
}

function collidingIds(profiles: Profile[]): Set<ProfileId> {
  const bySig: Record<string, ProfileId[]> = {};
  for (const p of profiles) (bySig[fpSignature(p)] ||= []).push(p.id);
  const out = new Set<ProfileId>();
  for (const ids of Object.values(bySig)) if (ids.length > 1) ids.forEach(id => out.add(id));
  return out;
}

function HealthView({ profiles, live, refresh, setEditor }: ProfileViewProps & { setEditor: (profile: Profile | null) => void }) {
  const [data, setData] = useState<ProfileHealth | null>(null);
  const [busy, setBusy] = useState<Record<string, string>>({});           // id -> action label
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
  /* Both of these open a real browser and change the profile, and neither said
   * so. `act` reported failures and nothing else, so a trust check that
   * re-graded a profile from B to C, and an auto-adjust that rewrote its OS,
   * screen, memory and GPU to what the browser actually presents, both finished
   * in silence - the row quietly redrew and that was the entire feedback. It is
   * the reason these read as buttons that do nothing: they do a great deal.
   *
   * So each action says what it did, in its own words, from what the endpoint
   * already returns. `describe` is per action because "graded B, 15 of 16
   * checks" and "changed 4 fields, C → B" are not the same sentence. */
  const act = async (id: string, label: string, fn: () => Promise<ProbeResult>, describe?: (result: ProbeResult) => string) => {
    setBusy(b => ({ ...b, [id]: label }));
    try {
      const res = await fn();
      await load(); await refresh();
      const said = describe?.(res);
      if (said) toast.success(said);
    } catch (e) { toast.error(`${label} failed: ` + errorMessage(e)); }
    setBusy(b => { const n = { ...b }; delete n[id]; return n; });
  };

  //: "B (94/100), 15 of 16 checks passed" - the grade and what it rests on.
  const gradeLine = (g: Partial<Grade> | undefined) => g?.grade
    ? `${g.grade} (${g.score}/100)` + (g.total ? `, ${g.passed} of ${g.total} checks passed` : "")
    : "no grade came back";

  const saidTrust = (name: string) => (res: ProbeResult) => `${name}: trust ${gradeLine(res?.after || res)}.`;

  const saidAlign = (name: string) => (res: ProbeResult) => {
    const changed = res?.changed || [];
    const before = res?.before?.grade, after = res?.after?.grade;
    const moved = before && after && before !== after ? ` ${before} → ${after}` : "";
    return changed.length
      ? `${name}: aligned${moved} — rewrote ${changed.join(", ")} to what the browser presents.`
      : `${name}: already aligned${moved || ""} — nothing needed changing.`;
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
                        onClick={() => act(r.id, "Trust check", () => api.trust(r.id), saidTrust(r.name))}>
                        {busy[r.id] === "Trust check" ? <RefreshCw size={13} className="ps-spin" /> : <ShieldCheck size={13} />}
                      </button>
                      <button title="Auto-adjust to grade A" className="run" disabled={!!busy[r.id] || r.status === "running"}
                        onClick={() => act(r.id, "Auto-adjust", () => api.align(r.id), saidAlign(r.name))}>
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

export { GradeBadge, HealthChip, SharedTag, fpSignature, collidingIds, HealthView };
