/* Primitives shared across views: form scaffolding and the small display
 * pieces more than one view needs.
 *
 * A component used by exactly one view belongs in that view's file instead -
 * this is for the genuinely shared ones. */

import React from "react";
import { Boxes, RefreshCw, Users, X } from "lucide-react";
import { toSvg } from "jdenticon";
import { FONT, T } from "./theme";

import { CORES, GPUS, RAM, SCREENS, nid, uaFor } from "./deviceData";

/* ---- form atoms -------------------------------------------------- */
const Field = ({ label, children }) => (<label className="ps-field"><span className="ps-flabel">{label}</span>{children}</label>);

const Row = ({ children }) => <div className="ps-row">{children}</div>;

const SectionLabel = ({ icon, text }) => (<div className="ps-seclabel">{icon}<span>{text}</span><div className="ps-secline" /></div>);

const SectionHead = ({ icon, text, note }) => (
  <div className="ps-sechead"><span className="ps-sechead-ic">{icon}</span><span className="ps-sechead-t">{text}</span>{note && <span className="ps-sechead-n">{note}</span>}</div>
);

const Pick = ({ label, opts, val, on }) => (
  <div className="ps-field"><span className="ps-flabel">{label}</span>
    <div className="ps-seg">{opts.map(o => <button key={o} className={val === o ? "on" : ""} onClick={() => on(o)}>{o}</button>)}</div>
  </div>
);

/* ---- styles ------------------------------------------------------ */
// A function (not a const string) so it re-reads the live palette T on every
// render — that's what makes the light/dark toggle repaint the whole stylesheet.

/* ---- small pieces ------------------------------------------------ */
function Stat({ icon, label, value, accent, pulse }) {
  return (
    <div className="ps-stat">
      <div className={"ps-stat-ic" + (pulse ? " pulse" : "")} style={{ color: accent || T.muted }}>{icon}</div>
      <div>
        <div className="ps-stat-val">{value}</div>
        <div className="ps-stat-label">{label}</div>
      </div>
    </div>
  );
}

function Placeholder({ view }) {
  const map = {
    team: [Users, "Team", "Invite members, set roles, and share profiles securely."],
  };
  const [Ic, title, desc] = map[view] || [Boxes, "Soon", ""];
  return (
    <div className="ps-scroll">
      <div className="ps-empty" style={{ marginTop: 48 }}>
        <div className="ps-empty-ic"><Ic size={26} color={T.violet} /></div>
        <div style={{ fontSize: 16, fontWeight: 600, fontFamily: FONT.display }}>{title}</div>
        <div style={{ color: T.muted, fontSize: 13, maxWidth: 360, textAlign: "center", lineHeight: 1.6 }}>{desc}</div>
        <span className="ps-pill" style={{ background: T.violetDim, color: T.lilac }}>On the roadmap</span>
      </div>
    </div>
  );
}

// `engine` is null when we have no default from the server: a null engine lets
// the server apply its own stored default instead of us guessing one (B7).
function newProfile(folder, defaultEngine) {
  const os = "Windows";
  return {
    id: nid(), name: "", folder: folder === "All profiles" ? "Facebook Ads" : folder,
    status: "stopped", os, tags: [], engine: defaultEngine || null, browser: "Chrome", browserVersion: "128.0.0.0",
    userAgent: uaFor(os), screen: SCREENS[os][0], cores: CORES[os][1], memory: RAM[os][1],
    webglVendor: "Google Inc. (NVIDIA)", webglRenderer: GPUS[os][0],
    canvas: "Noise", webrtc: "Altered", audio: "Noise", fonts: "Masked",
    locale: "en-US", timezone: "America/New_York", geo: "Prompt",
    mediaVideo: 1, mediaAudio: 1, dnt: false, proxy: null, notes: "", startupUrls: "", extensions: "",
    bookmarks: "", launchArgs: "", dns: "", ipBased: [], clearOnStart: [], switchedOff: [],
    lastActive: "never", _new: true,
  };
}

/* A deterministic avatar per profile.
 *
 * A list of fifty profiles is read by name, one row at a time. A shape is
 * recognised at a glance, so the same row is found without reading anything.
 * Derived from the profile id - which never changes - so a profile always looks
 * the same, nothing is stored, and no network is involved.
 *
 * Jdenticon's defaults are full-saturation rainbow, which fights this palette.
 * Hue stays unconstrained because that is what makes two profiles tell apart;
 * only saturation and lightness are pulled toward the surrounding UI.
 */
const IDENTICON = {
  padding: 0.06,
  backColor: "#00000000",
  saturation: { color: 0.46, grayscale: 0.12 },
  lightness: {
    color: [0.44, 0.74],
    grayscale: [0.32, 0.62],
  },
};

const Identicon = ({ id, size = 30 }) => (
  <span className="ps-identicon" style={{ width: size, height: size }}
        aria-hidden="true"
        dangerouslySetInnerHTML={{ __html: toSvg(String(id || "?"), size, IDENTICON) }} />
);

/* A confirmation before something irreversible, saying what is about to
 * happen rather than "Are you sure?". Shared, because a second copy of a
 * confirmation dialog is how one page ends up asking and another does not. */
function ConfirmModal({ title, body, confirmLabel, danger, busy, onCancel, onConfirm }) {
  return (
    <div className="ps-overlay center" onClick={busy ? undefined : onCancel}>
      <div className="ps-modal ps-pop" onClick={e => e.stopPropagation()} style={{ maxWidth: 460 }}>
        <div className="ps-drawer-h">
          <div><div className="ps-eyebrow">Confirm</div><div className="ps-drawer-title">{title}</div></div>
          <button className="ps-x" onClick={onCancel} disabled={busy}><X size={18} /></button>
        </div>
        <div style={{ padding: 20, fontSize: 13, color: T.muted, lineHeight: 1.65 }}>{body}</div>
        <div className="ps-drawer-foot">
          <button className="ps-btn ghost" onClick={onCancel} disabled={busy}>Cancel</button>
          <button className={"ps-btn " + (danger ? "danger" : "primary")} onClick={onConfirm} disabled={busy}>
            {busy ? <><RefreshCw size={15} className="ps-spin" /> Working…</> : confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}

export { ConfirmModal, Field, Identicon, Row, SectionLabel, SectionHead, Pick, Stat, Placeholder, newProfile };
