/* Toasts: a module-level store and the component that renders it.
 *
 * A tiny pub/sub rather than React context, so any code — including an async
 * catch block outside the tree — can call `toast.error(msg)` without the app
 * threading a context through every component that might need it. */

import React, { useEffect, useState } from "react";
import { Activity, Check, ShieldAlert } from "lucide-react";

let _toastId = 0;

const _toastSubs = new Set();

function _pushToast(kind, message) {
  const t = { id: ++_toastId, kind, message: String(message) };
  _toastSubs.forEach(fn => fn(t));
}

const toast = {
  success: (m) => _pushToast("success", m),
  error: (m) => _pushToast("error", m),
  info: (m) => _pushToast("info", m),
};

function Toaster() {
  const [items, setItems] = useState([]);
  useEffect(() => {
    const on = (t) => {
      setItems(list => [...list, t]);
      setTimeout(() => setItems(list => list.filter(x => x.id !== t.id)), 3800);
    };
    _toastSubs.add(on);
    return () => _toastSubs.delete(on);
  }, []);
  const remove = (id) => setItems(list => list.filter(x => x.id !== id));
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
