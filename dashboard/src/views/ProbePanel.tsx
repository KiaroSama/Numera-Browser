import { ArrowRight, Check, RefreshCw, ShieldAlert, Sparkles, X } from "lucide-react";
import { T } from "../theme";
import type { Probe } from "../types";
/* ---- Probe result panel (proxy test / trust check) ---------------- */
export function ProbePanel({ probe, kind }: { probe: Probe | null; kind: string }) {
  if (!probe || probe.kind !== kind) return null;
  if (probe.state === "running")
    return <div className="ps-hint"><RefreshCw size={12} className="ps-spin" /> Opening the profile in the background — this takes a few seconds.</div>;
  if (probe.state === "error")
    return <div className="ps-hint"><ShieldAlert size={12} /> {probe.data}</div>;

  const d = probe.data || {};
  const checks = d.checks || [];
  const gColor = (s: number) => (s >= 95 ? T.mint : s >= 70 ? T.amber : T.red);
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
