import { useEffect, useRef, useState } from "react";
import { api } from "../api";
import { errorMessage } from "../types";

type Offer = Awaited<ReturnType<typeof api.cloakOffer>>;

export function CloakAcquisition({ live }: { live: boolean }) {
  const [offer, setOffer] = useState<Offer | null>(null);
  const [approved, setApproved] = useState(false);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const pending = useRef<AbortController | null>(null);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; pending.current?.abort(); };
  }, []);
  const run = async (download: boolean) => {
    pending.current?.abort();
    const controller = new AbortController();
    pending.current = controller;
    const timer = setTimeout(() => controller.abort(), download ? 310000 : 10000);
    setBusy(true);
    setMessage("");
    try {
      if (download) {
        if (!approved || !offer?.offer_id || !offer.available) return;
        const target = offer.offer_id;
        setApproved(false);
        const result = await api.acquireCloak(target, controller.signal);
        if (!controller.signal.aborted) {
          setMessage(`Approved browser acquired: ${result.version}.`);
          setOffer(current => current ? { ...current, installed: true } : null);
        }
      } else {
        const result = await api.cloakOffer(controller.signal);
        if (!controller.signal.aborted) { setOffer(result); setApproved(false); }
      }
    } catch (error) {
      if (mounted.current && download) { setOffer(null); setApproved(false); }
      if (mounted.current) setMessage(controller.signal.aborted
        ? "The vendor operation exceeded its UI deadline. Review local state before retrying; cancellation is not proof the vendor helper stopped."
        : errorMessage(error));
    } finally {
      clearTimeout(timer);
      if (pending.current === controller) {
        pending.current = null;
        if (mounted.current) setBusy(false);
      }
    }
  };
  return <section className="ps-set-card ps-update-panel" aria-label="Cloak browser acquisition">
    <h3 className="ps-set-h">Cloak browser download</h3>
    <p className="ps-set-note">The proprietary browser is not bundled. Review the vendor target before approving its download; opening a profile does not grant download consent.</p>
    {!live ? <p className="ps-set-note">Connect the engine to review locally installed SDK metadata.</p> : <>
      <button className="ps-btn ghost sm" disabled={busy} onClick={() => { void run(false); }}>Review Cloak download</button>
      {offer && <div className="ps-update-offer">
        {offer.version && <p className="ps-set-label">SDK {offer.sdk} · browser {offer.version} · {offer.platform}</p>}
        <p className="ps-set-note">{offer.reason || offer.notice}</p>
        {offer.installed ? <p role="status" className="ps-set-note">A browser is already available locally. Vendor background updates are disabled.</p>
          : offer.available && offer.offer_id && <>
            <label className="ps-set-note"><input type="checkbox" checked={approved} disabled={busy}
              onChange={event => setApproved(event.target.checked)} /> I approve downloading this exact target directly from the vendor under its license.</label>
            <div className="ps-update-actions"><button className="ps-btn primary sm" disabled={!approved || busy}
              onClick={() => { void run(true); }}>Approve vendor download</button></div>
          </>}
      </div>}
      <p className="ps-set-note" role="status">{busy ? "Waiting for the owned vendor operation…" : message}</p>
    </>}
  </section>;
}
