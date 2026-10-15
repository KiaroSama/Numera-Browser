export interface UpdateBridge { endpoint: string; token: string }
export type UpdateState = "idle" | "checking" | "available" | "current" | "unavailable" | "downloading" | "ready" | "deferred" | "activating" | "complete" | "cancelled" | "error";
export interface UpdateOffer {
  id: string; version: string; sourceSha: string; generation: number;
  downloadBytes: number; expandedBytes: number;
  components: Record<string, unknown>; restartRequired: true;
}
export interface UpdateStatus {
  state: UpdateState; message: string;
  current: { version: string; sourceSha: string; generation: number; platform: string; portable: boolean };
  offer: UpdateOffer | null;
  progress: { receivedBytes: number; totalBytes: number };
  canCancel: boolean;
}
export type UpdateAction = "status" | "check" | "download" | "install" | "cancel";

declare global {
  interface Window { __NUMERA_UPDATES__?: UpdateBridge }
}

const object = (v: unknown): v is Record<string, unknown> => typeof v === "object" && v !== null && !Array.isArray(v);
const text = (v: unknown, max = 2048): v is string => typeof v === "string" && v.length <= max && !/[\u0000-\u001f\u007f]/.test(v);
const count = (v: unknown): v is number => typeof v === "number" && Number.isSafeInteger(v) && v >= 0;
const states: readonly string[] = ["idle", "checking", "available", "current", "unavailable", "downloading", "ready", "deferred", "activating", "complete", "cancelled", "error"];

export function readUpdateBridge(): UpdateBridge | null {
  const candidate = window.__NUMERA_UPDATES__;
  if (!candidate || !text(candidate.endpoint, 64) || !/^[A-Za-z0-9_-]{32,256}$/.test(candidate.token)) return null;
  try {
    const url = new URL(candidate.endpoint);
    if (url.protocol !== "http:" || url.hostname !== "127.0.0.1" || !url.port || Number(url.port) < 1
      || url.username || url.password || url.search || url.hash || url.pathname !== "/"
      || candidate.endpoint !== url.origin) return null;
    return { endpoint: url.origin, token: candidate.token };
  } catch { return null; }
}

function parseStatus(value: unknown): UpdateStatus {
  if (!object(value) || !text(value.state) || !states.includes(value.state) || !text(value.message)
    || !object(value.current) || !text(value.current.version, 80) || !text(value.current.sourceSha, 80)
    || !count(value.current.generation) || !text(value.current.platform, 80) || typeof value.current.portable !== "boolean"
    || !object(value.progress) || !count(value.progress.receivedBytes) || !count(value.progress.totalBytes)
    || typeof value.canCancel !== "boolean") throw new Error("Invalid update status");
  if (value.offer !== null) {
    const offer = value.offer;
    if (!object(offer) || !text(offer.id, 256) || !offer.id || !text(offer.version, 80)
      || typeof offer.sourceSha !== "string" || !/^[a-f0-9]{40}$/.test(offer.sourceSha)
      || !count(offer.generation) || !offer.generation || !count(offer.downloadBytes) || !count(offer.expandedBytes)
      || !object(offer.components) || offer.restartRequired !== true) throw new Error("Invalid update status");
  }
  return value as unknown as UpdateStatus;
}

export async function updateRequest(bridge: UpdateBridge, action: UpdateAction, offerId?: string, signal?: AbortSignal): Promise<UpdateStatus> {
  const controller = new AbortController();
  const abort = () => controller.abort();
  signal?.addEventListener("abort", abort, { once: true });
  if (signal?.aborted) abort();
  const timer = setTimeout(abort, 10000);
  try {
    const body = action === "download" || action === "install" ? { offerId, approve: true }
      : action === "cancel" ? { offerId } : {};
    const result = await fetch(`${bridge.endpoint}/${action}`, {
      method: action === "status" ? "GET" : "POST", redirect: "error", credentials: "omit", cache: "no-store",
      headers: { Authorization: `Bearer ${bridge.token}`, "Content-Type": "application/json" },
      ...(action === "status" ? {} : { body: JSON.stringify(body) }), signal: controller.signal,
    });
    const reader = result.body?.getReader();
    if (!reader) throw new Error("Invalid update status");
    const chunks: Uint8Array[] = [];
    let size = 0;
    try {
      while (true) {
        const part = await reader.read();
        if (part.done) break;
        size += part.value.byteLength;
        if (size > 65536) throw new Error("Invalid update status");
        chunks.push(part.value);
      }
    } finally { await reader.cancel(); reader.releaseLock(); }
    const bytes = new Uint8Array(size);
    let offset = 0;
    for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.byteLength; }
    // Refusals return the same safe state contract; never print a raw HTTP body.
    const status = parseStatus(JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes)));
    if (!result.ok && !["error", "deferred", "unavailable"].includes(status.state)) {
      return { ...status, state: "error", message: "The update operation was refused. Check again before retrying." };
    }
    return status;
  } finally {
    clearTimeout(timer);
    signal?.removeEventListener("abort", abort);
  }
}
