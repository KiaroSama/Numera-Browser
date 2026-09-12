/* The bulk bar: one press, applied to a whole selection.
 *
 * Three defects lived here, and they share a cause - the bar treats "the
 * selection" as something each button may interpret for itself.
 *
 *  - Launch called the row toggle for every selected id, and the row toggle
 *    *stops* a running profile. A mixed selection therefore launched some
 *    sessions and killed others, which is the opposite of what the button says.
 *  - Warm-up swallowed every rejection in an empty catch, cleared the selection
 *    and finished quietly, so a run in which nothing warmed up looked exactly
 *    like one in which everything did.
 *  - Export and Delete resolved ids against the *filtered* list while Launch and
 *    Warm-up used the whole selection, so "3 selected" could export two profiles
 *    and delete one - silently, and Delete has no undo.
 *
 * Driven through the real App against a fake engine, because the selection lives
 * in App and the filtering that hides part of it does too: a ProfilesView
 * rendered with hand-made props would be testing the props, not the disagreement.
 */

import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import App from "../src/App.jsx";
import { auth } from "../src/api.js";
import { newProfile } from "../src/ui.jsx";

const response = (body, status = 200) => ({ ok: status < 400, status, json: async () => body });

const profile = (id, over = {}) => ({
  ...newProfile("Facebook Ads", "playwright"),
  id, name: id, tags: [], status: "stopped", lastActive: "never", _new: false, ...over,
});

/* A fake engine that records every session call the dashboard asks it to make.
 * `fail` is keyed "action:id", so a partial failure is set up per profile. */
function engine(profiles, fail = {}) {
  const state = { profiles, calls: [], deleted: null, fail };
  vi.stubGlobal("fetch", vi.fn(async (url, init = {}) => {
    const method = init.method || "GET";
    const path = String(url).split("?")[0];
    if (path === "/api/health") return response({ ok: true });
    if (path === "/api/engines") return response({ playwright: true });
    if (path === "/api/config") return response({ default_engine: "playwright", folders: [] });
    if (path === "/api/profiles" && method === "GET") return response(state.profiles.map(p => ({ ...p })));
    if (path === "/api/profiles/delete" && method === "POST") {
      const sent = JSON.parse(init.body).ids;
      state.deleted = sent;
      state.profiles = state.profiles.filter(p => !sent.includes(p.id));
      return response({ removed: sent.map(id => ({ id })), failed: [], missing: [] });
    }
    const m = path.match(/^\/api\/profiles\/([^/]+)\/(launch|stop|warmup)$/);
    if (m && method === "POST") {
      const [, id, action] = m;
      state.calls.push(`${action}:${id}`);
      const why = state.fail[`${action}:${id}`];
      if (why) return response({ detail: why }, 423);
      if (action !== "warmup") {
        const status = action === "launch" ? "running" : "stopped";
        state.profiles = state.profiles.map(p => p.id === id ? { ...p, status } : p);
      }
      return response({ ok: true });
    }
    return response({ detail: `Unexpected test route ${method} ${path}` }, 404);
  }));
  return state;
}

const called = (state, action) =>
  state.calls.filter(c => c.startsWith(action + ":")).map(c => c.slice(action.length + 1)).sort();

async function open(profiles, fail) {
  const state = engine(profiles, fail);
  render(<App />);
  await screen.findByText("Engine online");
  await screen.findByText(profiles[0].name);
  return state;
}

const selectAll = () => fireEvent.click(document.querySelector("thead input[type=checkbox]"));
const counter = () => screen.getByText(/\d+ selected/);
const bulk = (name) => within(counter().closest(".ps-bulkbar")).getByRole("button", { name });
const search = (text) =>
  fireEvent.change(screen.getByPlaceholderText(/Search profiles/), { target: { value: text } });
/* One turn of the microtask queue, so a handler that fired its requests
 * synchronously has certainly recorded them before absence is asserted. */
const settle = () => act(async () => { await Promise.resolve(); await Promise.resolve(); });

let objectUrls;
beforeEach(() => {
  auth.clear();
  objectUrls = [];
  URL.createObjectURL = vi.fn(blob => {
    objectUrls.push(blob);
    return `blob:test/${objectUrls.length - 1}`;
  });
  URL.revokeObjectURL = vi.fn();
});
afterEach(() => { cleanup(); auth.clear(); vi.unstubAllGlobals(); vi.restoreAllMocks(); });

/* ---- N06: Launch is launch-only ---------------------------------- */

test("bulk Launch starts the stopped profiles and never stops a running one", async () => {
  const state = await open([
    profile("alpha"),
    profile("bravo", { status: "running" }),
    profile("charlie"),
  ]);

  selectAll();
  fireEvent.click(bulk(/Launch/));

  await waitFor(() => expect(called(state, "launch")).toEqual(["alpha", "charlie"]));
  await settle();
  // The whole point: a selection that mixes running and stopped profiles must
  // not terminate the running ones. `stop` is the call that kills a live
  // session with its cookies and logins still in memory.
  expect(called(state, "stop")).toEqual([]);
  // Partial results are reported rather than implied.
  await screen.findByText(/Launched 2/);
  expect(screen.getByText(/already running/)).toBeTruthy();
});

test("launching a selection that is already running issues no calls at all", async () => {
  const state = await open([
    profile("alpha", { status: "running" }),
    profile("bravo", { status: "running" }),
  ]);

  selectAll();
  fireEvent.click(bulk(/Launch/));
  await settle();

  expect(state.calls).toEqual([]);
  await screen.findByText(/already running/);
});

test("a profile that fails to launch stays selected and is named", async () => {
  const state = await open(
    [profile("alpha"), profile("bravo")],
    { "launch:bravo": "Proxy credential is locked" },
  );

  selectAll();
  fireEvent.click(bulk(/Launch/));

  await screen.findByText(/Proxy credential is locked/);
  expect(called(state, "launch")).toEqual(["alpha", "bravo"]);
  // Recoverable: the one that failed is still selected, so Launch can be
  // pressed again without hunting for it, and the one that started is not.
  await waitFor(() => expect(counter().textContent).toBe("1 selected"));
  expect(document.querySelector("tr.sel .ps-pname").textContent).toContain("bravo");
});

test("Launch is launch-only in demo mode too", async () => {
  // Demo mode keeps the same behaviour locally instead of calling the engine,
  // and it toggled exactly the same way: half the sample profiles are running,
  // so pressing Launch on all six stopped three of them.
  vi.stubGlobal("fetch", vi.fn(async () => { throw new TypeError("offline"); }));
  render(<App />);
  await screen.findByText("Demo mode");

  selectAll();
  fireEvent.click(bulk(/Launch/));

  await waitFor(() => {
    const state = [...document.querySelectorAll("tbody .ps-status")].map(el => el.className);
    expect(state.filter(c => c.includes("stopped"))).toEqual([]);
    expect(state.filter(c => c.includes("running"))).toHaveLength(6);
  });
});

/* ---- N12: warm-up reports what happened --------------------------- */

test("bulk warm-up reports failures instead of clearing the selection", async () => {
  const state = await open(
    [profile("alpha"), profile("bravo"), profile("charlie")],
    { "warmup:bravo": "Session is busy", "warmup:charlie": "Session is busy" },
  );

  selectAll();
  fireEvent.click(bulk(/Warm up/));

  await waitFor(() => expect(called(state, "warmup")).toEqual(["alpha", "bravo", "charlie"]));
  // Visible, countable and actionable - the server's own reason, not "failed".
  const report = await screen.findByText(/Session is busy/);
  expect(report.textContent).toMatch(/2/);
  // Nothing anywhere may claim the whole batch warmed up.
  expect(screen.queryByText(/^Warmed up 3/)).toBeNull();
  // The two that failed are still selected; the one that worked is not.
  await waitFor(() => expect(counter().textContent).toBe("2 selected"));
});

test("a warm-up that fails outright never reports success", async () => {
  await open([profile("alpha")], { "warmup:alpha": "No engine installed" });

  selectAll();
  fireEvent.click(bulk(/Warm up/));

  await screen.findByText(/No engine installed/);
  expect(document.querySelector(".ps-toast.success")).toBeNull();
  expect(counter().textContent).toBe("1 selected");
});

test("warm-up cannot be fired twice while it is still running", async () => {
  const state = await open([profile("alpha")]);
  let release;
  const held = new Promise(r => { release = r; });
  const real = globalThis.fetch;
  globalThis.fetch = vi.fn(async (url, init) => {
    if (String(url).endsWith("/warmup")) await held;
    return real(url, init);
  });

  selectAll();
  fireEvent.click(bulk(/Warm up/));
  await waitFor(() => expect(bulk(/Warm up/).disabled).toBe(true));
  fireEvent.click(bulk(/Warm up/));
  release();

  await waitFor(() => expect(called(state, "warmup")).toEqual(["alpha"]));
  await settle();
  expect(called(state, "warmup")).toEqual(["alpha"]);   // one warm-up, not two
});

/* ---- N16: one selection scope for every bulk action --------------- */
/* Policy: the selection is global. Filtering changes what is *shown*, never
 * what is selected, and every bulk action resolves the full selection. */

test("a filter hides rows without shrinking what Delete is about to destroy", async () => {
  const state = await open([profile("alpha"), profile("bravo")]);

  selectAll();
  expect(counter().textContent).toBe("2 selected");
  search("alpha");                       // bravo is now off screen, still selected
  await waitFor(() => expect(screen.queryByText("bravo")).toBeNull());

  expect(counter().textContent).toBe("2 selected");
  fireEvent.click(bulk(/Delete/));
  // The confirmation counts every profile it is about to delete, including the
  // one the filter is hiding. It used to say "Delete this profile?".
  await screen.findByText("Delete 2 profiles?");
  fireEvent.click(within(document.querySelector(".ps-modal")).getByRole("button", { name: /Delete/ }));
  await waitFor(() => expect(state.deleted).toEqual(["alpha", "bravo"]));
});

test("export writes every selected profile, not only the visible ones", async () => {
  await open([profile("alpha"), profile("bravo")]);

  selectAll();
  search("alpha");
  await waitFor(() => expect(screen.queryByText("bravo")).toBeNull());

  const shown = Number(counter().textContent.split(" ")[0]);
  fireEvent.click(bulk(/Export/));

  expect(objectUrls).toHaveLength(1);
  const written = JSON.parse(await objectUrls[0].text());
  // The counter and the file have to agree; they used to differ by however many
  // selected profiles the current filter happened to hide.
  expect(written.map(p => p.id)).toEqual(["alpha", "bravo"]);
  expect(written).toHaveLength(shown);
});

test("Launch and Export resolve the identical set of ids", async () => {
  // The finding itself: two buttons in the same bar, one reading the selection
  // and one reading the filtered list, disagreeing about the same press.
  const state = await open([profile("alpha"), profile("bravo")]);

  selectAll();
  search("alpha");
  await waitFor(() => expect(screen.queryByText("bravo")).toBeNull());

  fireEvent.click(bulk(/Export/));
  const exported = JSON.parse(await objectUrls[0].text()).map(p => p.id);
  fireEvent.click(bulk(/Launch/));
  await waitFor(() => expect(called(state, "launch")).toHaveLength(2));

  expect(called(state, "launch")).toEqual(exported.sort());
});
