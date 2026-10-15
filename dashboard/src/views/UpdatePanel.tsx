import { useEffect, useRef, useState } from "react";
import { Download, RefreshCw, ShieldCheck } from "lucide-react";
import type { Updates } from "../useUpdates";
import type { UpdateOffer } from "../updates";
import "../updates.css";

const bytes = (n: number) => n >= 1073741824 ? `${(n / 1073741824).toFixed(1)} GiB`
  : n >= 1048576 ? `${(n / 1048576).toFixed(1)} MiB` : `${Math.ceil(n / 1024)} KiB`;

function Approval({ offer, install, valid, onClose, onApprove }: {
  offer: UpdateOffer; install: boolean; valid: boolean; onClose: () => void; onApprove: () => void;
}) {
  const dialog = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const previous = document.activeElement;
    const node = dialog.current;
    node?.querySelector<HTMLButtonElement>("button")?.focus();
    return () => { if (previous instanceof HTMLElement && previous.isConnected) previous.focus(); };
  }, []);
  return <div className="ps-overlay center">
    <div ref={dialog} className="ps-modal ps-update-dialog" role="dialog" aria-modal="true" aria-label="Confirm update"
      onKeyDown={e => {
        if (e.key === "Escape") { e.preventDefault(); onClose(); }
        if (e.key !== "Tab") return;
        const controls = Array.from(e.currentTarget.querySelectorAll<HTMLButtonElement>("button:not(:disabled)"));
        const first = controls[0], last = controls.at(-1);
        if (e.shiftKey && document.activeElement === first) { e.preventDefault(); last?.focus(); }
        else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first?.focus(); }
      }}>
      <div className="ps-drawer-h"><h3 className="ps-drawer-title">{install ? "Install this update and restart?" : "Download this update?"}</h3></div>
      <div className="ps-update-body">
        <p>Numera Browser {offer.version} · generation {offer.generation}</p>
        <p>{install ? "The app will restart after active sessions and jobs are closed. Busy or uncertain work defers installation; it is not force-stopped."
          : `Download ${bytes(offer.downloadBytes)}. The extracted update needs ${bytes(offer.expandedBytes)}, plus space for recovery.`}</p>
        <p>Managed browser SDKs and binaries update together. Profiles, vault data and external browser overrides are not replaced.</p>
        {!valid && <p role="alert">This offer changed. Close this dialog and review the current update.</p>}
      </div>
      <div className="ps-drawer-foot">
        <button className="ps-btn ghost" onClick={onClose}>Later</button>
        <button className="ps-btn primary" disabled={!valid} onClick={onApprove}>{install ? "Approve restart" : "Approve download"}</button>
      </div>
    </div>
  </div>;
}

export function UpdateNotification({ updates, onReview }: { updates: Updates; onReview: () => void }) {
  const [dismissed, setDismissed] = useState("");
  const offer = updates.status?.offer;
  const state = updates.status?.state;
  if (!offer || dismissed === offer.id || !["available", "ready", "error"].includes(state ?? "")) return null;
  return <div className="ps-update-notice" role="status">
    <ShieldCheck size={16} aria-hidden="true" />
    <span>{state === "ready" ? "Update ready to install" : "Software update available"} · {offer.version}</span>
    <button className="ps-btn ghost sm" onClick={onReview}>Review update</button>
    <button className="ps-btn ghost sm" onClick={() => setDismissed(offer.id)}>Later</button>
  </div>;
}

export function UpdatePanel({ updates }: { updates: Updates }) {
  const { supported, status, error, busy, act } = updates;
  const [approval, setApproval] = useState<{ offer: UpdateOffer; install: boolean } | null>(null);
  const offer = status?.offer;
  const active = busy || ["checking", "downloading", "activating"].includes(status?.state ?? "");
  const ready = ["ready", "deferred"].includes(status?.state ?? "");
  const percent = status?.progress.totalBytes ? Math.min(100, Math.round(status.progress.receivedBytes / status.progress.totalBytes * 100)) : 0;
  return <section className="ps-set-card ps-update-panel" aria-label="Software updates">
    <h3 className="ps-set-h"><ShieldCheck size={15} aria-hidden="true" /> Software updates</h3>
    {!supported ? <p className="ps-set-note">Source and server installations use their package or deployment workflow. Open a signed portable desktop build for managed updates.</p> : <>
      <p className="ps-set-note">The desktop checks GitHub after startup and once a day while open. Nothing downloads or installs without your approval.</p>
      {status && <p className="ps-update-current">Installed: {status.current.version} · generation {status.current.generation}<br />
        <code>{status.current.sourceSha.slice(0, 12)}</code> · {status.current.platform}</p>}
      <p className="ps-set-note" role={error || status?.state === "error" ? "alert" : "status"}>
        {error || status?.message || "Connecting to the desktop update service…"}
      </p>
      {offer && <div className="ps-update-offer">
        <div className="ps-set-label">Available: {offer.version} · generation {offer.generation}</div>
        <p className="ps-set-note"><code>{offer.sourceSha.slice(0, 12)}</code> · {bytes(offer.downloadBytes)} download · restart required</p>
        <details><summary>Browser and SDK versions</summary><pre className="ps-code">{JSON.stringify(offer.components, null, 2)}</pre></details>
      </div>}
      {status?.state === "downloading" && <div className="ps-update-progress">
        <progress max={100} value={percent} aria-label="Update download progress" />
        <span>{bytes(status.progress.receivedBytes)} / {bytes(status.progress.totalBytes)} ({percent}%)</span>
      </div>}
      <div className="ps-update-actions">
        <button className="ps-btn ghost sm" disabled={active} onClick={() => act("check")}><RefreshCw size={13} /> Check for updates</button>
        {offer && !active && !error && status?.current.portable && ["available", "ready", "deferred", "error"].includes(status.state) && <button className="ps-btn primary sm"
          onClick={() => setApproval({ offer, install: ready })}><Download size={13} />{ready ? "Install and restart" : "Download update"}</button>}
        {status?.canCancel && <button className="ps-btn ghost sm" disabled={busy} onClick={() => act("cancel", offer?.id ?? "")}>Cancel update</button>}
      </div>
    </>}
    {approval && <Approval offer={approval.offer} install={approval.install}
      valid={!active && !error && offer?.id === approval.offer.id && (approval.install ? ready : !ready)}
      onClose={() => setApproval(null)} onApprove={() => {
        act(approval.install ? "install" : "download", approval.offer.id);
        setApproval(null);
      }} />}
  </section>;
}
