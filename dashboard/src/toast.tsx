/* Toasts: a module-level store and the component that renders it.
 *
 * A tiny pub/sub rather than React context, so any code — including an async
 * catch block outside the tree — can call `toast.error(msg)` without the app
 * threading a context through every component that might need it. */

import { useEffect, useState } from "react";
import { Activity, Check, ShieldAlert } from "lucide-react";

let _toastId = 0;

type Toast = { id: number; kind: "success" | "error" | "info"; message: string };
const _toastSubs = new Set<(toast: Toast) => void>();

function _pushToast(kind: Toast["kind"], message: unknown): void {
  const t: Toast = { id: ++_toastId, kind, message: String(message) };
  _toastSubs.forEach(fn => fn(t));
}

const toast = {
  success: (m: unknown) => _pushToast("success", m),
  error: (m: unknown) => _pushToast("error", m),
  info: (m: unknown) => _pushToast("info", m),
};

function Toaster() {
  const [items, setItems] = useState<Toast[]>([]);
  useEffect(() => {
    const on = (t: Toast) => {
      setItems(list => [...list, t]);
      setTimeout(() => setItems(list => list.filter(x => x.id !== t.id)), 3800);
    };
    _toastSubs.add(on);
    return () => { _toastSubs.delete(on); };
  }, []);
  const remove = (id: number) => setItems(list => list.filter(x => x.id !== id));
  return (
    <div className="ps-toaster">
      {items.map(t => (
        <div key={t.id} className={"ps-toast " + t.kind} onClick={() => remove(t.id)}>
          {t.kind === "success" ? <Check size={15} /> : t.kind === "error" ? <ShieldAlert size={15} /> : <Activity size={15} />}
          <span>{t.message}</span>
        </div>
      ))}
    </div>
  );
}

export { toast, Toaster };
