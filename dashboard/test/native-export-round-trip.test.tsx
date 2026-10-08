type ImportAnswer = { imported?: number; format?: string; results?: { issues?: string[]; name?: string }[]; status?: number; body?: unknown };
import type { Profile } from "../src/types";
import { response } from "./helpers";
/* Export and Import, as one contract.
 *
 * Export wrote a bare array of dashboard profiles and Import handed whatever
 * file it was given to the engine's *vendor* importer - the tolerant one, whose
 * job is to guess at a schema it has never seen. Fed Numera's own file that
 * tolerance is destruction: it invents a new fingerprint seed and drops the
 * signal modes, startup URLs, extensions and folder, then reports "Imported 1
 * profile". The file a user exported was not a round-trip of their own
 * configuration, and nothing anywhere said so.
 *
 * So the file names its format, and the dashboard says what actually happened:
 * which format the engine read, that this is configuration and not a browser
 * session, and which proxy passwords the vault was holding when the file was
 * written and therefore are not in it.
 *
 * Driven through the real App, because the export button reads the selection
 * that lives in App and the import handler lives there too.
 */

import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import App from "../src/App.tsx";
import { auth } from "../src/api.ts";
import { newProfile } from "../src/ui.tsx";



/* A stored profile as `GET /api/profiles` returns it: a sentinel in every
 * editable field, so a field the export drops is visible as a missing value. */
const profile = (id: string, over: Partial<Profile> = {}): Profile => ({
  ...newProfile("Client work", "camoufox"),
  id, name: id, _new: false, _rev: 3, _created: 1700000000,
  status: "stopped", lastActive: "never",
  seed: 424242, fontList: "Sentinel Sans", osVersion: "10.0.19045",
  deviceModel: "", maxTouchPoints: 0, devicePixelRatio: 1,
  clientRects: "Off", webglImage: "Off", mediaDevices: "Real", mediaSpeakers: 4,
  startupUrls: "https://example.test/search?q=red,blue",
  extensions: "C:\\ext\\sentinel", launchArgs: "--sentinel",
  tags: ["sentinel-tag"], ipBased: ["timezone"], clearOnStart: ["cookies"],
  switchedOff: ["googleServices"],
  proxy: { type: "SOCKS5", host: "gw.sentinel.test", port: "1080",
           user: "u", pass: "sentinel-pass", country: "DE" },
  ...over,
});

/* A fake engine that records what the dashboard sent to the import route. */
function engine(profiles: Profile[], importAnswer: ImportAnswer = { imported: 1, format: "numera", results: [] }) {
  const state: { profiles: Profile[]; imported: string | null } = { profiles, imported: null };
  vi.stubGlobal("fetch", vi.fn(async (url: RequestInfo | URL, init: RequestInit = {}) => {
    const method = init.method || "GET";
    const path = String(url).split("?")[0]!;
    if (path === "/api/health") return response({ ok: true });
    if (path === "/api/engines") return response({ playwright: true, camoufox: true });
    if (path === "/api/config") return response({ default_engine: "camoufox", folders: [] });
    if (path === "/api/profiles" && method === "GET") return response(state.profiles.map(p => ({ ...p })));
    if (path === "/api/profiles/import" && method === "POST") {
      state.imported = JSON.parse(String(init.body)).text;
      if ((importAnswer.status ?? 200) >= 400) return response(importAnswer.body, importAnswer.status);
      return response(importAnswer);
    }
    return response({ detail: `Unexpected test route ${method} ${path}` }, 404);
  }));
  return state;
}

async function open(profiles: Profile[], importAnswer?: ImportAnswer) {
  const state = engine(profiles, importAnswer);
  render(<App />);
  await screen.findByText("Engine online");
  await screen.findByText(String(profiles[0]!.name));
  return state;
}

const counter = () => screen.getByText(/\d+ selected/);
const bulk = (name: string | RegExp) => within(counter().closest(".ps-bulkbar")!).getByRole<HTMLButtonElement>("button", { name });
const selectAll = () => fireEvent.click(document.querySelector<HTMLInputElement>("thead input[type=checkbox]")!);
const settle = () => act(async () => { await Promise.resolve(); await Promise.resolve(); });

let objectUrls: Blob[];
let pickedFile: string | null;

beforeEach(() => {
  auth.clear();
  objectUrls = [];
  pickedFile = null;
  URL.createObjectURL = vi.fn(blob => { objectUrls.push(blob); return `blob:test/${objectUrls.length - 1}`; });
  URL.revokeObjectURL = vi.fn();
  /* The import handler builds its own <input type=file> and clicks it, so the
   * file arrives through a picker no test can operate. Hand that one element a
   * chosen file and let `click()` be the moment it is chosen; everything after
   * that is the real handler. */
  const realCreate = document.createElement.bind(document);
  vi.spyOn(document, "createElement").mockImplementation((tag, ...rest) => {
    const el = realCreate(tag, ...rest);
    if (tag !== "input") return el;
    el.click = () => {
      if (pickedFile === null) return;
      Object.defineProperty(el, "files", {
        configurable: true,
        value: [{ text: async () => pickedFile }],
      });
      el.dispatchEvent(new Event("change", { bubbles: true }));
    };
    return el;
  });
});
afterEach(() => { cleanup(); auth.clear(); vi.unstubAllGlobals(); vi.restoreAllMocks(); });

const exportFile = async () => JSON.parse(await objectUrls[0]!.text());
const importButton = () => screen.getByRole<HTMLButtonElement>("button", { name: /Import/ });

/* ---- the file says what it is ------------------------------------- */

test("export writes a versioned Numera file, not a bare array", async () => {
  await open([profile("alpha")]);

  selectAll();
  fireEvent.click(bulk(/Export/));

  const file = await exportFile();
  // Without this the engine cannot tell its own configuration from a vendor
  // export, and the tolerant importer is the wrong reader for one of them.
  expect(file.numeraProfiles).toBe(1);
  expect(Array.isArray(file.profiles)).toBe(true);
  expect(file.profiles.map((p: Profile) => p.id)).toEqual(["alpha"]);
});

test("every editable field is in the file, including the ones the vendor importer never read", async () => {
  const source = profile("alpha");
  await open([source]);

  selectAll();
  fireEvent.click(bulk(/Export/));

  const [written] = (await exportFile()).profiles;
  for (const key of ["seed", "webglVendor", "webglRenderer", "canvas", "webrtc",
                     "audio", "clientRects", "webglImage", "mediaDevices",
                     "startupUrls", "extensions", "launchArgs", "folder",
                     "engine", "ipBased", "clearOnStart", "switchedOff", "proxy"]) {
    expect({ [key]: written[key] }).toEqual({ [key]: source[key] });
  }
});

test("the export says it is configuration and not a browser session", async () => {
  await open([profile("alpha")]);

  selectAll();
  fireEvent.click(bulk(/Export/));

  // Cookies, logins and local storage are not in this file; a user who reads
  // "Exported 1 profile" and deletes the original has lost the session.
  const said = await screen.findByText(/cookies|session/i);
  expect(said.textContent).toMatch(/not included|numera backup/i);
});

test("a proxy password the vault is holding is named, not quietly omitted", async () => {
  // `pass: null` is exactly how the listing reports a withheld secret; an
  // absent password comes back as "".
  await open([
    profile("alpha", { proxy: { ...profile("alpha").proxy!, pass: null } }),
    profile("bravo"),
  ]);

  selectAll();
  fireEvent.click(bulk(/Export/));

  const warning = await screen.findByText(/password/i);
  expect(warning.textContent).toMatch(/1 proxy password/i);
  // The file is still written - the rest of the configuration is worth having.
  const file = await exportFile();
  expect(file.profiles).toHaveLength(2);
  expect(file.profiles[0].proxy.host).toBe("gw.sentinel.test");
});

/* ---- and the import says what was done with it --------------------- */

test("the chosen file reaches the engine exactly as written", async () => {
  const state = await open([profile("alpha")]);
  pickedFile = JSON.stringify({ numeraProfiles: 1, profiles: [profile("beta")] });

  fireEvent.click(importButton());
  await waitFor(() => expect(state.imported).not.toBeNull());

  expect(JSON.parse(state.imported!).numeraProfiles).toBe(1);
});

test("a file read as a vendor export says so instead of implying a faithful import", async () => {
  const state = await open([profile("alpha")],
    { imported: 1, format: "vendor", results: [{ name: "from gologin", issues: [] }] });
  pickedFile = JSON.stringify([{ name: "from gologin" }]);

  fireEvent.click(importButton());
  await waitFor(() => expect(state.imported).not.toBeNull());

  // The vendor importer regenerates the fingerprint it could not read. Saying
  // only "Imported 1 profile" is how that became invisible.
  const said = await screen.findByText(/vendor/i);
  expect(said.textContent).toMatch(/fingerprint/i);
});

test("a native import reports plainly and claims nothing about vendor heuristics", async () => {
  const state = await open([profile("alpha")],
    { imported: 2, format: "numera", results: [{ name: "one", issues: [] }, { name: "two", issues: [] }] });
  pickedFile = JSON.stringify({ numeraProfiles: 1, profiles: [] });

  fireEvent.click(importButton());
  await waitFor(() => expect(state.imported).not.toBeNull());

  await screen.findByText(/Imported 2 profiles/);
  expect(screen.queryByText(/vendor/i)).toBeNull();
});

test("a profile whose password could not come back is reported", async () => {
  const state = await open([profile("alpha")], {
    imported: 1, format: "numera",
    results: [{ name: "one", issues: ["the proxy password was not in this file"] }],
  });
  pickedFile = JSON.stringify({ numeraProfiles: 1, profiles: [] });

  fireEvent.click(importButton());
  await waitFor(() => expect(state.imported).not.toBeNull());

  const said = await screen.findByText(/flagged/i);
  expect(said.textContent).toMatch(/1/);
});

test("a file the engine refuses shows the engine's reason", async () => {
  const state = await open([profile("alpha")],
    { status: 400, body: { detail: "this looks like a Numera profile export written before the format carried a version" } });
  pickedFile = JSON.stringify([profile("beta")]);

  fireEvent.click(importButton());
  await waitFor(() => expect(state.imported).not.toBeNull());

  // The unversioned-export refusal is the one the user most needs to read.
  await screen.findByText(/written before the format carried a version/);
  expect(document.querySelector<HTMLElement>(".ps-toast.success")!).toBeNull();
});

test("choosing no file asks the engine for nothing", async () => {
  const state = await open([profile("alpha")]);

  fireEvent.click(importButton());
  await settle();

  expect(state.imported).toBeNull();
});
