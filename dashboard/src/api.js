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

async function req(method, path, body) {
  const res = await fetch(BASE + path, {
    method,
    headers: body ? { "Content-Type": "application/json" } : undefined,
    body: body ? JSON.stringify(body) : undefined,
  });
  if (!res.ok) {
    // Use the server's own explanation when it sent one. FastAPI puts it in
    // `detail`, and it is the only thing that tells the user *why* — dropping it
    // left the UI showing "PUT /api/config → 400", which is a status code, not a
    // reason someone can act on.
    let detail = "";
    try {
      const body = await res.json();
      detail = typeof body?.detail === "string" ? body.detail : "";
    } catch { /* not JSON, or empty — fall back to the status line */ }
    throw new Error(detail || `${method} ${path} → ${res.status}`);
  }
  return res.status === 204 ? null : res.json();
}

export const api = {
  async health() {
    try {
      const res = await fetch(BASE + "/api/health");
      return res.ok;
    } catch {
      return false;
    }
  },
  engines: () => req("GET", "/api/engines"),
  getConfig: () => req("GET", "/api/config"),
  setDefaultEngine: (name) => req("PUT", "/api/config", { default_engine: name }),
  setDefaultExtensions: (paths) => req("PUT", "/api/config", { default_extensions: paths }),
  // A Web Store link is downloaded and unpacked engine-side: Chromium cannot
  // install from the store itself once automation is enabled.
  installExtension: (url) => req("POST", "/api/extensions/install", { url }),
  // What has been fetched from the store, with each one's real name. A folder
  // path ending in a 32-character id says nothing about which extension it is.
  installedExtensions: () => req("GET", "/api/extensions/installed"),
  // The extension's own icon, straight out of its manifest. A URL rather than a
  // fetch: <img> handles the loading and the failure, and the browser caches it.
  extensionIcon: (path) => `${BASE}/api/extensions/icon?path=${encodeURIComponent(path)}`,
  // Folders: a persisted list the sidebar and editor share. Rename/delete also
  // reassign the profiles that live in the folder.
  createFolder: (name) => req("POST", "/api/folders", { name }),
  renameFolder: (from, to) => req("POST", "/api/folders/rename", { from, to }),
  deleteFolder: (name, reassignTo) => req("POST", "/api/folders/delete", { name, reassignTo }),
  list: () => req("GET", "/api/profiles"),
  create: (p) => req("POST", "/api/profiles", p),
  update: (p) => req("PUT", `/api/profiles/${p.id}`, p),
  remove: (id) => req("DELETE", `/api/profiles/${id}`),
  launch: (id) => req("POST", `/api/profiles/${id}/launch`),
  stop: (id) => req("POST", `/api/profiles/${id}/stop`),
  // Cookies are read/written through the profile's real browser session, so
  // these calls open it headlessly and can take a couple of seconds.
  exportCookies: (id) => req("GET", `/api/profiles/${id}/cookies`),
  importCookies: (id, text, clear) => req("POST", `/api/profiles/${id}/cookies`, { text, clear }),
  // These open the profile headlessly too, so they're slow by nature.
  proxyTest: (id) => req("POST", `/api/profiles/${id}/proxy-test`),
  trust: (id) => req("POST", `/api/profiles/${id}/trust`),
  tls: (id) => req("POST", `/api/profiles/${id}/tls`),
  // DNS leak check: do name lookups escape the proxy to your real ISP?
  dns: (id) => req("POST", `/api/profiles/${id}/dns`),
  // Auto-adjust: align the profile to what its browser actually presents.
  align: (id) => req("POST", `/api/profiles/${id}/align`),
  // Automation: open the profile with a CDP endpoint your scripts can attach to.
  // (Detach with stop(id) — the attached browser closes when the session ends.)
  attach: (id, headless) => req("POST", `/api/profiles/${id}/attach`, { headless }),
  warmup: (id, minutes, preset) => req("POST", `/api/profiles/${id}/warmup`, { minutes, preset }),
  bulkUpdate: (ids, patch) => req("POST", "/api/profiles/bulk", { ids, patch }),
  // Bulk create at scale: save a chunk of generated profiles in one request.
  batchCreate: (profiles) => req("POST", "/api/profiles/batch", { profiles }),
  // Import profiles exported from GoLogin / AdsPower / Multilogin.
  importProfiles: (text) => req("POST", "/api/profiles/import", { text }),
  // Proxy pool: import, list, test and assign proxies across profiles.
  listProxies: () => req("GET", "/api/proxies"),
  addProxies: (text) => req("POST", "/api/proxies", { text }),
  deleteProxy: (id) => req("DELETE", `/api/proxies/${id}`),
  // Bulk forms so a 40-proxy sweep is one request, not forty in a row.
  deleteProxies: (ids) => req("POST", "/api/proxies/delete", { ids }),
  // Rendered engine-side: the line format is the parser's inverse, and a second
  // copy of it here would drift until an export stopped importing.
  exportProxies: (ids) => req("POST", "/api/proxies/export", { ids }),
  testProxies: (ids) => req("POST", "/api/proxies/test", { ids }),
  updateProxy: (id, patch) => req("PUT", `/api/proxies/${id}`, patch),
  testProxy: (id) => req("POST", `/api/proxies/${id}/test`),
  assignProxies: (profileIds, proxyIds) => req("POST", "/api/proxies/assign", { profileIds, proxyIds }),
  // Scheduled warm-up: recurring, unattended aging of a profile's session.
  listSchedules: () => req("GET", "/api/schedules"),
  addSchedule: (s) => req("POST", "/api/schedules", s),
  updateSchedule: (id, patch) => req("PUT", `/api/schedules/${id}`, patch),
  deleteSchedule: (id) => req("DELETE", `/api/schedules/${id}`),
  // Health overview: coherence, proxy, cached trust grade and schedule per profile.
  // Distinct from health() above — that one probes whether the engine is up at
  // all and must never throw; this one fetches per-profile data and does.
  profileHealth: () => req("GET", "/api/profiles/health"),
  // Fingerprint collisions: which profiles look like the same device.
  collisions: () => req("GET", "/api/profiles/collisions"),
};
