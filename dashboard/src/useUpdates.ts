import { useCallback, useEffect, useRef, useState } from "react";
import { readUpdateBridge, updateRequest } from "./updates";
import type { UpdateAction, UpdateStatus } from "./updates";

export interface Updates {
  supported: boolean; status: UpdateStatus | null; busy: boolean; error: string;
  act: (action: Exclude<UpdateAction, "status">, offerId?: string) => void;
}

// App owns this once. Views share its snapshot; none runs a second GitHub check.
export function useUpdates(): Updates {
  const [bridge, setBridge] = useState(readUpdateBridge);
  useEffect(() => {
    const connected = () => setBridge(current => current ?? readUpdateBridge());
    window.addEventListener("numera-updates-ready", connected);
    connected();
    return () => window.removeEventListener("numera-updates-ready", connected);
  }, []);
  const [status, setStatus] = useState<UpdateStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const command = useRef<Updates["act"]>(() => undefined);
  useEffect(() => {
    if (!bridge) return;
    let disposed = false;
    let sequence = 0;
    let controller: AbortController | null = null;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const run = async (action: UpdateAction, offerId?: string) => {
      clearTimeout(timer);
      controller?.abort();
      controller = new AbortController();
      const own = ++sequence;
      setBusy(action !== "status");
      let delay = 15000;
      try {
        const next = await updateRequest(bridge, action, offerId, controller.signal);
        if (disposed || own !== sequence) return;
        setStatus(next);
        setError("");
        if (["checking", "downloading", "activating"].includes(next.state)) delay = 1000;
      } catch {
        if (disposed || own !== sequence) return;
        setError("The desktop update service is unavailable. Check again before approving an update.");
      } finally {
        if (!disposed && own === sequence) {
          setBusy(false);
          timer = setTimeout(() => { void run("status"); }, delay);
        }
      }
    };
    command.current = (action, id) => { void run(action, id); };
    void run("status");
    return () => {
      disposed = true;
      ++sequence;
      clearTimeout(timer);
      controller?.abort();
      command.current = () => undefined;
    };
  }, [bridge]);
  const act = useCallback<Updates["act"]>((action, id) => command.current(action, id), []);
  return { supported: bridge !== null, status, busy, error, act };
}
