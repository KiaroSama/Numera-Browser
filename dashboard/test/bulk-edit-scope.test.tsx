import type { Profile, BulkPatch } from "../src/types";
import { response } from "./helpers";
/* "Edit all": the selection it shows, and the selection it sends.
 *
 * N16 gave every button in the bulk bar one selection scope - the global
 * selection, resolved against the profiles that still exist - and left "Edit
 * all" out of it. The toolbar counts the resolved selection inside
 * ProfilesView; the modal and the bulk request were handed the raw `sel` set
 * from App, and nothing prunes that set when a refresh removes a profile. So a
 * profile deleted from another client produced a toolbar reading "1 selected"
 * above a modal offering "Apply to 2", and the request carried the ghost id.
 *
 * Driven through the real App against a fake engine, because the disagreement
 * is *between* App and ProfilesView: a hand-rendered SyncModal is handed its
 * ids as a prop, so it can only ever agree with itself.
 */

import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import App from "../src/App.tsx";
import { auth } from "../src/api.ts";
import { newProfile } from "../src/ui.tsx";



const profile = (id: string, over: Partial<Profile> = {}) => ({
  ...newProfile("Facebook Ads", "playwright"),
  id, name: id, tags: [], status: "stopped", lastActive: "never", _new: false, ...over,
});

/* A fake engine that records the bulk edit it was asked to make.
 *
 * `/api/profiles/bulk` answers the way the real route does: `patch_many` skips
 * an id whose profile is gone and returns only what it saved, so the response
 * length is how the dashboard learns that part of its request missed. */
function engine(profiles: Profile[]) {
  const state: { profiles: Profile[]; bulk: { ids: string[]; patch: BulkPatch } | null } = { profiles, bulk: null };
  vi.stubGlobal("fetch", vi.fn(async (url: RequestInfo | URL, init: RequestInit = {}) => {
    const method = init.method || "GET";
    const path = String(url).split("?")[0]!;
    if (path === "/api/health") return response({ ok: true });
    if (path === "/api/engines") return response({ playwright: true });
    if (path === "/api/config") return response({ default_engine: "playwright", folders: [] });
    if (path === "/api/profiles" && method === "GET") return response(state.profiles.map(p => ({ ...p })));
    if (path === "/api/profiles/bulk" && method === "POST") {
      state.bulk = JSON.parse(String(init.body));
      const wanted = new Set(state.bulk!.ids);
      const saved = state.profiles.filter(p => wanted.has(p.id!));
      state.profiles = state.profiles.map(p => wanted.has(p.id!)
        ? { ...p, tags: [...new Set([...p.tags, ...(state.bulk!.patch.addTags || [])])] } : p);
      return response(saved.map(p => ({ ...p })));
    }
    return response({ detail: `Unexpected test route ${method} ${path}` }, 404);
  }));
  return state;
}

async function open(profiles: Profile[]) {
  const state = engine(profiles);
  render(<App />);
  await screen.findByText("Engine online");
  await screen.findByText(String(profiles[0]!.name));
  return state;
}

const selectAll = () => fireEvent.click(document.querySelector<HTMLInputElement>("thead input[type=checkbox]")!);
const counter = () => screen.getByText(/\d+ selected/);
const bulkBtn = (name: string | RegExp) => within(counter().closest(".ps-bulkbar")!).getByRole<HTMLButtonElement>("button", { name });
const modal = () => document.querySelector<HTMLElement>(".ps-modal")!;
const applyBtn = () => within(modal()).getByRole("button", { name: /Apply to/ });
const folderBtn = (name: string) =>
  [...document.querySelectorAll<HTMLElement>(".ps-folder")].find(b => b.textContent.startsWith(name));
const search = (text: string) =>
  fireEvent.change(screen.getByPlaceholderText(/Search profiles/), { target: { value: text } });

/* Open "Edit all", fill one field, apply. Returns the count the modal offered,
 * which is the number the user last saw before the request went out. */
async function editAll(tag = "audit") {
  fireEvent.click(bulkBtn(/Edit all/));
  await waitFor(() => expect(modal()).toBeTruthy());
  const offered = applyBtn().textContent.trim();
  fireEvent.change(within(modal()).getByLabelText(/Add tags/), { target: { value: tag } });
  fireEvent.click(applyBtn());
  return offered;
}

beforeEach(() => { auth.clear(); });
afterEach(() => { cleanup(); auth.clear(); vi.unstubAllGlobals(); vi.restoreAllMocks(); });

/* The scenario the finding names: A and B selected, B deleted from another
 * client, the poll brings the deletion in, "Edit all" pressed.
 *
 * The poll is waited for on its own interval rather than slept through, so the
 * test ends the moment the row goes rather than on a fixed delay. */
test("Edit all offers the count the toolbar shows, not the ids selected before the refresh", async () => {
  const state = await open([profile("alpha"), profile("bravo")]);

  selectAll();
  expect(counter().textContent).toBe("2 selected");

  // Another client deletes bravo. Nothing here knows yet.
  state.profiles = state.profiles.filter(p => p.id !== "bravo");
  await waitFor(() => expect(screen.queryByText("bravo")).toBeNull(), { timeout: 10000 });

  // The toolbar already counts one. The modal used to offer two.
  expect(counter().textContent).toBe("1 selected");
  expect(await editAll()).toBe("Apply to 1");
  await waitFor(() => expect(state.bulk?.ids).toEqual(["alpha"]));
}, 20000);

/* The half of N16 that already works and must keep working: a filter changes
 * what is shown, never what is selected. */
test("a filter hides a selected profile without shrinking what Edit all changes", async () => {
  const state = await open([profile("alpha"), profile("bravo")]);

  selectAll();
  search("alpha");
  await waitFor(() => expect(screen.queryByText("bravo")).toBeNull());

  expect(counter().textContent).toBe("2 selected");
  expect(await editAll()).toBe("Apply to 2");
  await waitFor(() => expect(state.bulk?.ids.slice().sort()).toEqual(["alpha", "bravo"]));
});

test("switching folders leaves the selection Edit all is about to change alone", async () => {
  const state = await open([profile("alpha"), profile("bravo", { folder: "Ops" })]);

  selectAll();
  expect(counter().textContent).toBe("2 selected");
  fireEvent.click(folderBtn("Ops")!);
  await waitFor(() => expect(screen.queryByText("alpha")).toBeNull());

  expect(counter().textContent).toBe("2 selected");
  expect(await editAll()).toBe("Apply to 2");
  await waitFor(() => expect(state.bulk?.ids.slice().sort()).toEqual(["alpha", "bravo"]));
});

test("a selection spanning pages is edited whole, from whichever page is on screen", async () => {
  // 101 profiles, 100 to a page: the last one is only reachable on page two.
  const many = Array.from({ length: 101 }, (_, i) => profile(`p-${String(i).padStart(3, "0")}`));
  const state = await open(many);

  selectAll();
  expect(counter().textContent).toBe("101 selected");
  fireEvent.click(screen.getByRole<HTMLButtonElement>("button", { name: /Next/ }));
  await screen.findByText("p-100");

  expect(counter().textContent).toBe("101 selected");
  expect(await editAll()).toBe("Apply to 101");
  await waitFor(() => expect(state.bulk?.ids).toHaveLength(101));
}, 30000);

/* The window the modal itself opens: polling is suspended while it is up, so a
 * profile deleted in that moment cannot be pruned before Apply is pressed. The
 * request is sent for both ids and the server saves one - which the dashboard
 * has to say, because the last thing the user read was "Apply to 2". */
test("a profile deleted while the modal is open is reported, not counted as edited", async () => {
  const state = await open([profile("alpha"), profile("bravo")]);

  selectAll();
  fireEvent.click(bulkBtn(/Edit all/));
  await waitFor(() => expect(modal()).toBeTruthy());
  expect(applyBtn().textContent.trim()).toBe("Apply to 2");

  state.profiles = state.profiles.filter(p => p.id !== "bravo");   // gone, mid-modal
  fireEvent.change(within(modal()).getByLabelText(/Add tags/), { target: { value: "audit" } });
  fireEvent.click(applyBtn());

  await waitFor(() => expect(state.bulk?.ids.slice().sort()).toEqual(["alpha", "bravo"]));
  // Said out loud, with the number that actually changed.
  await screen.findByText(/no longer existed/);
  expect(screen.queryByText(/Edited 2 profiles/)).toBeNull();
});
