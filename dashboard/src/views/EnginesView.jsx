/* The pluggable stealth backends and their install state. */

import React, { useState } from "react";
import { ArrowRight, Blocks, Check, Clipboard, Copy, Fingerprint, MonitorSmartphone, Sparkles, Star } from "lucide-react";
import { T } from "../theme";
import { ENGINE_META, ENGINE_ORDER } from "../deviceData";
import { SectionHead } from "../ui";

function StatusDot({ state }) {
  const map = {
    builtin: [T.mint, "Built-in"], ready: [T.mint, "Installed"],
    missing: [T.dim, "Not installed"], unknown: [T.violet, "Available"],
  };
  const [c, label] = map[state];
  return <span className="ps-eng-status" style={{ color: c }}><span className="ps-eng-status-dot" style={{ background: c }} />{label}</span>;
}

function StrengthMeter({ value }) {
  return (
    <div className="ps-strength" title={`Stealth strength ${value}/4`}>
      <span className="ps-strength-label">Stealth</span>
      <div className="ps-strength-bars">
        {[1, 2, 3, 4].map(n => <span key={n} className={"ps-sb" + (n <= value ? " on" : "")} />)}
      </div>
    </div>
  );
}

function EngineCard({ id, status, live, delay, featured }) {
  const m = ENGINE_META[id];
  const [copied, setCopied] = useState(false);
  const copy = () => { navigator.clipboard?.writeText(m.install); setCopied(true); setTimeout(() => setCopied(false), 1400); };
  // status: true=installed, false=not, undefined=unknown (demo / builtin)
  const state = m.builtin ? "builtin" : status === true ? "ready" : status === false ? "missing" : "unknown";
  return (
    <div className={"ps-eng-card ps-rise" + (featured ? " featured" : "")} style={{ animationDelay: `${delay}ms` }}>
      <div className="ps-eng-card-top">
        <div>
          <div className="ps-eng-name">{m.label} {m.recommended && <span className="ps-rec"><Star size={10} /> Recommended</span>}</div>
          <div className="ps-eng-kind">{m.kind}</div>
        </div>
        <StatusDot state={state} />
      </div>
      <StrengthMeter value={m.strength} />
      <p className="ps-eng-desc">{m.desc}</p>
      {m.install ? (
        <div className="ps-eng-install">
          <code>{m.install}</code>
          <button onClick={copy} title="Copy install command">{copied ? <Check size={13} color={T.mint} /> : <Clipboard size={13} />}</button>
        </div>
      ) : (
        <div className="ps-eng-builtin"><Check size={13} color={T.mint} /> Ships with Numera — nothing to install.</div>
      )}
      {m.docs && <a className="ps-eng-docs" href={m.docs} target="_blank" rel="noreferrer">Setup guide <ArrowRight size={12} /></a>}
    </div>
  );
}

function EnginesView({ engines, live }) {
  const recommended = ENGINE_ORDER.filter(k => ENGINE_META[k].recommended);
  const others = ENGINE_ORDER.filter(k => !ENGINE_META[k].recommended);
  return (
    <div className="ps-scroll">
      <div className="ps-eng-hero ps-rise">
        <div className="ps-eng-hero-badge"><Blocks size={13} /> Pluggable engines</div>
        <h1 className="ps-eng-title">Numera manages the identity.<br /><span className="grad">The engine renders it.</span></h1>
        <p className="ps-eng-lead">
          Every profile carries a coherent fingerprint, a proxy and a session — that part is Numera's job.
          Plug in the stealth backend that matches your target. Install it, pick it per profile, launch.
        </p>
        <div className="ps-eng-flow">
          <span className="ps-flow-node"><Fingerprint size={14} /> Identity</span>
          <ArrowRight size={15} color={T.dim} />
          <span className="ps-flow-node"><Blocks size={14} /> Engine</span>
          <ArrowRight size={15} color={T.dim} />
          <span className="ps-flow-node"><MonitorSmartphone size={14} /> Real browser</span>
        </div>
      </div>

      <SectionHead icon={<Star size={13} />} text="Recommended" note="Strongest against modern detection" />
      <div className="ps-eng-grid">
        {recommended.map((k, i) => <EngineCard key={k} id={k} status={engines?.[k]} live={live} delay={i * 60} featured />)}
      </div>

      <SectionHead icon={<Blocks size={13} />} text="Also available" />
      <div className="ps-eng-grid">
        {others.map((k, i) => <EngineCard key={k} id={k} status={engines?.[k]} live={live} delay={i * 60} />)}
      </div>

      <div className="ps-eng-note">
        <Sparkles size={14} color={T.violet} />
        <span>Add your own backend by registering it in <code>engine/numera/drivers.py</code> — one function, one decorator.</span>
      </div>
    </div>
  );
}

export { StatusDot, StrengthMeter, EngineCard, EnginesView };
