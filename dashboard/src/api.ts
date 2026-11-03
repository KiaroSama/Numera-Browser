import * as contracts from "./apiContracts";
import type { BulkPatch, PoolProxy, ProfileWrite, ProfileId, Schedule } from "./types";
/* ------------------------------------------------------------------ *
 *  Tiny client for the Numera engine API (engine/numera/server.py).
 *
 *  The dashboard works in two modes:
 *   - LIVE  : the engine API is reachable, so launching actually opens a
 *             real Chromium window and profiles persist to disk.
 *   - DEMO  : the API is offline, so the UI runs on in-memory sample data
 *             (great for exploring the project without a backend).
 *
 *  Override the API location with a VITE_API_URL env var if needed.
 *
 *  The default is EMPTY, i.e. same-origin, and that matters. The engine now
 *  serves this bundle itself, and the desktop shell picks a free port at
 *  runtime - so a hardcoded "http://localhost:8787" is a compile-time constant
 *  that is wrong on every run but the first. Worse, it failed quietly: the app
 *  rendered, could not reach its own API, and fell back to demo mode showing
 *  fabricated profiles. Same-origin has no port to get wrong.
 * ------------------------------------------------------------------ */

const BASE = import.meta.env.VITE_API_URL || "";

/* The bearer token, when the engine is locked.
 *
 * Kept in localStorage because it belongs to this browser profile, not to the
 * document: a page reload must not throw the user back to a login they cannot
 * complete. It is scoped to the engine's own origin, which is where the token
 * is spendable anyway.
 *
 * Note this is a *credential in a browser*. It is only as safe as the machine,
 * which is the same trust boundary the whole app already sits inside. */
const TOKEN_KEY = "numera.token";

let token = "";
try { token = localStorage.getItem(TOKEN_KEY) || ""; } catch { /* private mode */ }

export const auth = {
  get: () => token,
  set(value: string) {
    token = (value || "").trim();
    try {
      if (token) localStorage.setItem(TOKEN_KEY, token);
      else localStorage.removeItem(TOKEN_KEY);
    } catch { /* private mode: it still works for this session */ }
  },
  clear() { this.set(""); },
};

/* Thrown for a 401 so callers can tell "you need a token" apart from "the
 * engine is not there". Conflating the two is what put fabricated demo
 * profiles on screen in front of a perfectly healthy locked engine. */
export class ApiError extends Error {
  readonly status: number;
  readonly kind: string | null;
  constructor(message: string, status: number, kind: string | null = null) {
    super(message);
    this.name = "ApiError";
    this.status = status;
    this.kind = kind;
  }
}

export class AuthError extends ApiError {
  constructor(detail?: string) {
    super(detail || "This engine requires an API token.", 401);
    this.name = "AuthError";
  }
}

/* Every 401 the app ever sees is reported through here.
 *
 * One handler instead of a catch at each call site, because the call sites are
 * exactly where this went wrong: the status poller's `refresh().catch(() => {})`
 * and the token list's own catch both swallowed a 401, so minting the first
 * named token - which locks this very server from the next request onwards -
 * left the dashboard looking connected while everything it asked for was being
 * rejected. Revoking the token in use did the same. `req` is the one place a 401
 * can be seen, so it is the one place that reports one. */
let onAuthLost: ((error: AuthError) => void) | null = null;
export function setAuthLostHandler(fn: ((error: AuthError) => void) | null) { onAuthLost = fn; }

function headers(body: unknown) {
  const h: Record<string, string> = {};
  if (body) h["Content-Type"] = "application/json";
  if (token) h["Authorization"] = `Bearer ${token}`;
  return Object.keys(h).length ? h : undefined;
}

// The editor's marker never goes to the server: it is a UI fact, not a field.
export const withoutMarker = <T extends ProfileWrite>({ _passwordSet, ...rest }: T): T => rest as T;

/** What a profile write may say about the proxy password.
 *
 * The dashboard loads profiles with their passwords decrypted, so PUTting the
 * whole object back sent the stored password again - and the server reads a
 * password in a write as REPLACE. An ordinary notes or name edit therefore
 * *chose* the dashboard's credential, which on a profile whose two stored
 * copies disagree converged them silently instead of answering 409.
 *
 * Omitting the field is how a caller says "leave it alone", so that is what an
 * untouched password control sends. `_passwordSet` is set by the editor when
 * the user actually enters one (see setProxy in Editor.jsx) - never inferred
 * from the value, because retyping the same password and never touching the
 * field look identical and mean opposite things.
 */
export const forWrite = (p: ProfileWrite) => {
  const rest = withoutMarker(p);
  if (p._passwordSet || !rest.proxy || !("pass" in rest.proxy)) return rest;
  const { pass, ...proxy } = rest.proxy;
  return { ...rest, proxy };
};

/* The request itself, with this app's credential and this app's error
 * reporting, returning the raw Response.
 *
 * Split out of `req` because not every protected route answers with JSON: an
 * extension icon is image bytes. A second fetch written next to that one caller
 * would be a second place for the token, the 401 handler and the `detail`
 * unwrapping to drift out of step with this one. */
async function send(method: string, path: string, body?: unknown, signal?: AbortSignal): Promise<Response> {
  const res = await fetch(BASE + path, {
    method,
    ...(headers(body) ? { headers: headers(body)! } : {}),
    ...(body ? { body: JSON.stringify(body) } : {}),
    ...(signal ? { signal } : {}),
  });
  if (!res.ok) {
    // Use the server's own explanation when it sent one. FastAPI puts it in
    // `detail`, and it is the only thing that tells the user *why* — dropping it
    // left the UI showing "PUT /api/config → 400", which is a status code, not a
    // reason someone can act on.
    let detail = "";
    let kind = null;
    try {
      const body: unknown = await res.json();
      const payload = contracts.record(body);
      detail = typeof payload.detail === "string" ? payload.detail : "";
      kind = typeof payload.kind === "string" ? payload.kind : null;
    } catch { /* not JSON, or empty — fall back to the status line */ }
    if (res.status === 401) {
      const lost = new AuthError(detail);
      onAuthLost?.(lost);
      throw lost;
    }
    throw new ApiError(detail || `${method} ${path} → ${res.status}`, res.status, kind);
  }
  return res;
}

async function req(method: string, path: string, body?: unknown, signal?: AbortSignal): Promise<unknown> {
  const res = await send(method, path, body, signal);
  return res.status === 204 ? null : res.json();
}

export const api = {
  /* Three outcomes, not two.
   *
   * "locked" is an engine that is running perfectly and wants a token. It used
   * to return false here, indistinguishable from no engine at all, and the app
   * answered by showing sample data as though it were the user's own. */
  async health(): Promise<"ok" | "locked" | "off"> {
    try {
      const res = await fetch(BASE + "/api/health", { ...(headers(null) ? { headers: headers(null)! } : {}) });
      if (res.ok) return "ok";
      return res.status === 401 ? "locked" : "off";
    } catch {
      return "off";
    }
  },
  engines: () => req("GET", "/api/engines").then(contracts.engines),
  cloakOffer: (signal: AbortSignal) => req("GET", "/api/engines/cloak/acquisition", undefined, signal).then(contracts.cloakOffer),
  acquireCloak: (offer_id: string, signal: AbortSignal) => req("POST", "/api/engines/cloak/acquisition", { offer_id, approve: true }, signal)
    .then(contracts.shape({ engine: contracts.text, installed: contracts.boolean, version: contracts.text, offer_id: contracts.text })),
  getConfig: () => req("GET", "/api/config").then(contracts.config),
  setDefaultEngine: (name: string) => req("PUT", "/api/config", { default_engine: name }).then(contracts.config),
  setDefaultExtensions: (paths: string[]) => req("PUT", "/api/config", { default_extensions: paths }).then(contracts.config),
  // A Web Store link is downloaded and unpacked engine-side: Chromium cannot
  // install from the store itself once automation is enabled.
  installExtension: (url: string) => req("POST", "/api/extensions/install", { url }).then(contracts.shape({ default_extensions: contracts.array(contracts.text), reused: contracts.boolean, name: contracts.text })),
  // What has been fetched from the store, with each one's real name. A folder
  // path ending in a 32-character id says nothing about which extension it is.
  installedExtensions: () => req("GET", "/api/extensions/installed").then(contracts.extensions),
  /* The extension's own icon, straight out of its manifest.
   *
   * Fetched, not handed to `<img src>`. That endpoint is protected, and the
   * request an `<img>` makes is the browser's own - it carries no Authorization
   * header and cannot be given one, so against any engine holding a token every
   * icon came back 401 and fell back to the grey glyph while every other
   * request on the same page authenticated. Putting the token in the query
   * string would "work" and write the credential into every access log on the
   * way, so it is a blob instead. The caller owns the object URL and revokes
   * it; there is nothing else that can. */
  extensionIcon: async (path: string) => {
    const res = await send("GET", `/api/extensions/icon?path=${encodeURIComponent(path)}`);
    return URL.createObjectURL(await res.blob());
  },
  // Folders: a persisted list the sidebar and editor share. Rename/delete also
  // reassign the profiles that live in the folder.
  createFolder: (name: string) => req("POST", "/api/folders", { name }).then(contracts.config),
  renameFolder: (from: string, to: string) => req("POST", "/api/folders/rename", { from, to }).then(contracts.config),
  deleteFolder: (name: string, reassignTo?: string) => req("POST", "/api/folders/delete", { name, reassignTo }).then(contracts.config),
  list: () => req("GET", "/api/profiles").then(contracts.profiles),
  create: (p: ProfileWrite) => req("POST", "/api/profiles", withoutMarker(p)).then(contracts.profile),
  update: (p: ProfileWrite) => req("PUT", `/api/profiles/${p.id}`, forWrite(p)).then(contracts.profile),
  remove: (id: ProfileId) => req("DELETE", `/api/profiles/${id}`),
  // One request for a selection. Deleting M profiles used to be M round trips,
  // while creating and editing in bulk were both batched.
  removeMany: (ids: ProfileId[]) => req("POST", "/api/profiles/delete", { ids }).then(contracts.shape({ removed: contracts.array(value => typeof value === "string" ? contracts.text(value) : contracts.shape({ id: contracts.text })(value)), missing: contracts.array(contracts.text), failed: contracts.array(contracts.shape({ error: contracts.text })) })),
  launch: (id: ProfileId) => req("POST", `/api/profiles/${id}/launch`),
  stop: (id: ProfileId) => req("POST", `/api/profiles/${id}/stop`),
  // Cookies are read/written through the profile's real browser session, so
  // these calls open it headlessly and can take a couple of seconds.
  exportCookies: (id: ProfileId) => req("GET", `/api/profiles/${id}/cookies`).then(contracts.shape({ cookies: contracts.array(contracts.json) })),
  importCookies: (id: ProfileId, text: string, clear: boolean) => req("POST", `/api/profiles/${id}/cookies`, { text, clear }).then(contracts.shape({ imported: contracts.number })),
  // These open the profile headlessly too, so they're slow by nature.
  proxyTest: (id: ProfileId) => req("POST", `/api/profiles/${id}/proxy-test`).then(contracts.probe),
  trust: (id: ProfileId) => req("POST", `/api/profiles/${id}/trust`).then(contracts.probe),
  tls: (id: ProfileId) => req("POST", `/api/profiles/${id}/tls`).then(contracts.probe),
  // DNS leak check: do name lookups escape the proxy to your real ISP?
  dns: (id: ProfileId) => req("POST", `/api/profiles/${id}/dns`).then(contracts.probe),
  // Auto-adjust: align the profile to what its browser actually presents.
  align: (id: ProfileId) => req("POST", `/api/profiles/${id}/align`).then(contracts.probe),
  // Automation: open the profile with a CDP endpoint your scripts can attach to.
  // (Detach with stop(id) — the attached browser closes when the session ends.)
  attach: (id: ProfileId, headless: boolean) => req("POST", `/api/profiles/${id}/attach`, { headless }).then(contracts.attachment),
  warmup: (id: ProfileId, minutes: number, preset: string) => req("POST", `/api/profiles/${id}/warmup`, { minutes, preset }),
  bulkUpdate: (ids: ProfileId[], patch: BulkPatch) => req("POST", "/api/profiles/bulk", { ids, patch }).then(contracts.profiles),
  // Bulk create at scale: save a chunk of generated profiles in one request.
  batchCreate: (profiles: ProfileWrite[]) => req("POST", "/api/profiles/batch", { profiles }),
  // Import profiles exported from GoLogin / AdsPower / Multilogin.
  importProfiles: (text: string) => req("POST", "/api/profiles/import", { text }).then(contracts.shape({ imported: contracts.number, format: contracts.text, results: contracts.array(contracts.shape({ issues: contracts.array(contracts.text) })) })),
  // Proxy pool: import, list, test and assign proxies across profiles.
  listProxies: () => req("GET", "/api/proxies").then(contracts.proxies),
  addProxies: (text: string) => req("POST", "/api/proxies", { text }).then(contracts.shape({ added: contracts.number, total: contracts.number })),
  deleteProxy: (id: ProfileId) => req("DELETE", `/api/proxies/${id}`),
  // Bulk forms so a 40-proxy sweep is one request, not forty in a row.
  deleteProxies: (ids: ProfileId[]) => req("POST", "/api/proxies/delete", { ids }).then(contracts.shape({ deleted: contracts.number })),
  // Rendered engine-side: the line format is the parser's inverse, and a second
  // copy of it here would drift until an export stopped importing.
  exportProxies: (ids: ProfileId[]) => req("POST", "/api/proxies/export", { ids }).then(contracts.shape({ text: contracts.text, count: contracts.number })),
  testProxies: (ids: ProfileId[]) => req("POST", "/api/proxies/test", { ids }).then(contracts.shape({ tested: contracts.number, ok: contracts.number })),
  updateProxy: (id: ProfileId, patch: Partial<PoolProxy>) => req("PUT", `/api/proxies/${id}`, patch),
  testProxy: (id: ProfileId) => req("POST", `/api/proxies/${id}/test`),
  assignProxies: (profileIds?: string[], proxyIds?: string[]) => req("POST", "/api/proxies/assign", { profileIds, proxyIds }).then(contracts.shape({ assigned: contracts.number })),
  // Rotating proxies: ask the provider's change-IP endpoint for a new exit IP.
  // The engine re-checks afterwards, so `rotated` means the address really moved.
  rotateProxy: (id: ProfileId) => req("POST", `/api/proxies/${id}/rotate`).then(contracts.shape({ rotated: contracts.boolean, previousIp: contracts.optional(contracts.nullable(contracts.text)), ip: contracts.optional(contracts.text), error: contracts.optional(contracts.text) })),
  rotateProxies: (ids: ProfileId[]) => req("POST", "/api/proxies/rotate", { ids }).then(contracts.shape({ rotated: contracts.number, tried: contracts.number })),
  // Archive retires a proxy without losing it or which profiles used it.
  archiveProxies: (ids: ProfileId[], archived = true) => req("POST", "/api/proxies/archive", { ids, archived }),
  // Named API tokens. The value comes back from create() once and never again.
  listTokens: () => req("GET", "/api/tokens").then(contracts.tokens),
  createToken: (name: string) => req("POST", "/api/tokens", { name }).then(contracts.tokenCreated),
  revokeToken: (id: ProfileId) => req("DELETE", `/api/tokens/${id}`),
  // Scheduled warm-up: recurring, unattended aging of a profile's session.
  listSchedules: () => req("GET", "/api/schedules").then(contracts.schedules),
  addSchedule: (s: Omit<Schedule, "id">) => req("POST", "/api/schedules", s).then(contracts.schedule),
  updateSchedule: (id: ProfileId, patch: Partial<Schedule>) => req("PUT", `/api/schedules/${id}`, patch).then(contracts.schedule),
  deleteSchedule: (id: ProfileId) => req("DELETE", `/api/schedules/${id}`),
  // Health overview: coherence, proxy, cached trust grade and schedule per profile.
  // Distinct from health() above — that one probes whether the engine is up at
  // all and must never throw; this one fetches per-profile data and does.
  profileHealth: () => req("GET", "/api/profiles/health").then(contracts.health),
  // Fingerprint collisions: which profiles look like the same device.
  collisions: () => req("GET", "/api/profiles/collisions"),
};
