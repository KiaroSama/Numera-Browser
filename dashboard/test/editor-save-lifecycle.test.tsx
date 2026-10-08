import type { Profile, ProfileWrite } from "../src/types";
import { response } from "./helpers";
import { afterEach, expect, test, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import App from "../src/App.tsx";
import { newProfile } from "../src/ui.tsx";
import { auth } from "../src/api.ts";


const notes = () => screen.getByPlaceholderText<HTMLTextAreaElement>(/worth remembering/i);
const save = () => screen.getByRole<HTMLButtonElement>("button", { name: /Save changes/ });
const proxyTab = () => fireEvent.click(screen.getByRole<HTMLButtonElement>("button", { name: "proxy" }));
const password = () => document.querySelector<HTMLInputElement>('.ps-editor input[type="password"]')!;
const toggleProxy = () => fireEvent.click(document.querySelector<HTMLButtonElement>(".ps-proxy-toggle button")!);

async function open(secret: string | null = "original") {
  auth.clear();
  const state: { profile: Profile & { _rev: number }; writes: ProfileWrite[]; failure: number | string | null; failureKind: string | null; pending: boolean; finish: (() => void) | null; reads: number; failRefresh: boolean } = {
    profile: { ...newProfile("Facebook Ads", "playwright"), id: "p1", _new: false,
      name: "One", _rev: 5, notes: "", proxy: {
        type: "HTTP", host: "localhost", port: "8080", user: "u", pass: secret, country: "US",
      } },
    writes: [], failure: null, failureKind: null, pending: false, finish: null, reads: 0, failRefresh: false,
  };
  vi.stubGlobal("fetch", vi.fn(async (url: RequestInfo | URL, init: RequestInit = {}) => {
    const method = init.method || "GET";
    const path = String(url).split("?")[0]!;
    if (path === "/api/health") return response({ ok: true });
    if (path === "/api/engines") return response({ playwright: true });
    if (path === "/api/config") return response({ default_engine: "playwright", folders: [] });
    if (path === "/api/proxies") return response([
      { id: "pool1", type: "HTTP", host: "127.0.0.1", port: "8081", pass: "pooled" },
    ]);
    if (path === "/api/profiles" && method === "GET") {
      state.reads++;
      if (state.failRefresh) return response({ detail: "Refresh unavailable" }, 503);
      return response([{ ...state.profile }]);
    }
    if (path === "/api/profiles/p1" && method === "PUT") {
      const sent = JSON.parse(String(init.body));
      state.writes.push(sent);
      if (state.pending) await new Promise<void>(resolve => { state.finish = resolve; });
      if (state.failure === "network") throw new TypeError("Network unavailable");
      if (state.failure) return response({ detail: `Rejected ${state.failure}`, kind: state.failureKind }, Number(state.failure));
      if (sent._rev !== state.profile._rev) {
        return response({ detail: "Profile changed since it was read", kind: "StaleProfile" }, 409);
      }
      const proxy = !("proxy" in sent) ? state.profile.proxy
        : sent.proxy ? { ...state.profile.proxy!, ...sent.proxy } : null;
      state.profile = { ...state.profile, ...sent, proxy, _rev: state.profile._rev + 1 };
      return response(state.profile);
    }
    return response({ detail: "Unexpected test route" }, 404);
  }));
  render(<App />);
  await screen.findByText("Engine online");
  fireEvent.click(await screen.findByTitle("Edit"));
  await screen.findByPlaceholderText(/worth remembering/i);
  return state;
}

afterEach(() => { cleanup(); auth.clear(); vi.unstubAllGlobals(); vi.restoreAllMocks(); });

test.each([409, 423, 422, 500, "network"])("failed save (%s) keeps notes and permits retry", async failure => {
  const state = await open();
  state.failure = failure;
  fireEvent.change(notes(), { target: { value: "keep this exact draft" } });
  fireEvent.click(save());
  await screen.findByText(/Save failed:/);
  expect(notes().value).toBe("keep this exact draft");
  expect(save().disabled).toBe(false);
  state.failure = null;
  fireEvent.click(save());
  await waitFor(() => expect(screen.queryByPlaceholderText(/worth remembering/i)).toBeNull());
  expect(state.writes).toHaveLength(2);
  expect(state.profile.notes).toBe("keep this exact draft");
  expect(state.reads).toBeGreaterThan(1);
});

test("stale revision rebases local notes while preserving remote tags", async () => {
  const state = await open();
  state.profile = { ...state.profile, tags: ["remote"], _rev: 6 };
  fireEvent.change(notes(), { target: { value: "local notes" } });
  fireEvent.click(save());
  await screen.findByText(/rebased onto the latest profile/i);
  expect(notes().value).toBe("local notes");
  fireEvent.click(save());
  await waitFor(() => expect(document.querySelector<HTMLElement>(".ps-editor")!).toBeNull());
  expect(state.writes.map(p => p._rev)).toEqual([5, 6]);
  expect(state.profile).toMatchObject({ notes: "local notes", tags: ["remote"], _rev: 7 });
  expect(state.writes[1]!).not.toHaveProperty("tags");
});

test.each(["Keep my edit", "Use latest"])("stale revision resolves overlapping notes explicitly: %s", async choice => {
  const state = await open();
  state.profile = { ...state.profile, notes: "remote notes", _rev: 6 };
  fireEvent.change(notes(), { target: { value: "local notes" } });
  fireEvent.click(save());
  await screen.findByText(/both changed/i);
  expect(notes().value).toBe("local notes");
  expect(save().disabled).toBe(true);
  fireEvent.click(save());
  expect(state.writes).toHaveLength(1);
  fireEvent.click(screen.getByRole<HTMLButtonElement>("button", { name: choice }));
  fireEvent.click(save());
  await waitFor(() => expect(document.querySelector<HTMLElement>(".ps-editor")!).toBeNull());
  expect(state.profile.notes).toBe(choice === "Use latest" ? "remote notes" : "local notes");
  expect(state.writes[1]!._rev).toBe(6);
});

test.each(["clear", "replace", "remove", "preserve"])("stale revision preserves proxy %s intent", async intent => {
  const state = await open();
  state.profile = { ...state.profile, tags: ["remote"], _rev: 6 };
  if (intent === "preserve") {
    state.profile.proxy = { ...state.profile.proxy!, pass: "remote secret" };
    fireEvent.change(notes(), { target: { value: "local notes" } });
  } else {
    proxyTab();
    if (intent === "remove") toggleProxy();
    else fireEvent.change(password(), { target: { value: intent === "clear" ? "" : "replacement" } });
  }
  fireEvent.click(save());
  await screen.findByText(/rebased onto the latest profile/i);
  fireEvent.click(save());
  await waitFor(() => expect(document.querySelector<HTMLElement>(".ps-editor")!).toBeNull());
  expect(state.profile.tags).toEqual(["remote"]);
  expect(state.writes[1]!._rev).toBe(6);
  expect(state.writes[1]!).not.toHaveProperty("_passwordSet");
  if (intent === "remove") expect(state.profile.proxy).toBeNull();
  else expect(state.profile.proxy!.pass).toBe(intent === "clear" ? "" : intent === "replace" ? "replacement" : "remote secret");
  if (intent === "preserve") expect(state.writes[1]!).not.toHaveProperty("proxy");
});

test("a proxy conflict locks draft controls until its explicit resolution", async () => {
  const state = await open();
  state.profile = { ...state.profile, proxy: { ...state.profile.proxy!, pass: "remote secret" }, _rev: 6 };
  proxyTab();
  fireEvent.change(password(), { target: { value: "local secret" } });
  fireEvent.click(save());
  await screen.findByText(/both changed/i);
  expect(document.querySelector<HTMLElement>(".ps-editor-main")!.hasAttribute("inert")).toBe(true);
  expect(document.querySelector<HTMLElement>(".ps-editor")!.textContent).not.toContain("local secret");
  expect(document.querySelector<HTMLElement>(".ps-editor")!.textContent).not.toContain("remote secret");
  toggleProxy(); // Synthetic events also cannot mutate an unresolved draft.
  expect(password().value).toBe("local secret");
  fireEvent.click(screen.getByRole<HTMLButtonElement>("button", { name: "Use latest" }));
  expect(password().value).toBe("remote secret");
  expect(document.querySelector<HTMLElement>(".ps-editor-main")!.hasAttribute("inert")).toBe(false);
  fireEvent.click(save());
  await waitFor(() => expect(document.querySelector<HTMLElement>(".ps-editor")!).toBeNull());
  expect(state.profile.proxy!.pass).toBe("remote secret");
  expect(state.writes[1]!).not.toHaveProperty("proxy");
});

test.each([
  ["host", "host.proxy.io", "127.0.0.2", "original"],
  ["port", "8080", "8082", "original"],
  ["user", null, "other-user", "original"],
  ["host", "host.proxy.io", "127.0.0.2", ""],
])("keeping a remotely removed proxy restores its credential after editing %s", async (field, placeholder, value, secret) => {
  const state = await open(secret);
  proxyTab();
  const input = placeholder ? screen.getByPlaceholderText(placeholder) : screen.getByDisplayValue("u");
  fireEvent.change(input, { target: { value } });
  state.profile = { ...state.profile, proxy: null, _rev: 6 };
  fireEvent.click(save());
  await screen.findByText(/both changed/i);
  expect(document.querySelector<HTMLElement>(".ps-editor")!.textContent).not.toContain("original");
  fireEvent.click(screen.getByRole<HTMLButtonElement>("button", { name: "Keep my edit" }));
  fireEvent.click(save());
  await waitFor(() => expect(document.querySelector<HTMLElement>(".ps-editor")!).toBeNull());
  expect(state.writes.map(p => p._rev)).toEqual([5, 6]);
  expect(state.writes[1]!).not.toHaveProperty("_passwordSet");
  expect(state.writes[1]!.proxy).toMatchObject({ [field]: value, pass: secret });
  expect(state.profile.proxy!.pass).toBe(secret);
});

test.each(["original", null])("using the latest whole proxy accepts removal without replaying credentials (%s)", async secret => {
  const state = await open(secret);
  proxyTab();
  fireEvent.change(screen.getByPlaceholderText("host.proxy.io"), { target: { value: "127.0.0.2" } });
  state.profile = { ...state.profile, proxy: null, _rev: 6 };
  fireEvent.click(save());
  await screen.findByText(/both changed/i);
  fireEvent.click(screen.getByRole<HTMLButtonElement>("button", { name: "Use latest" }));
  fireEvent.click(save());
  await waitFor(() => expect(document.querySelector<HTMLElement>(".ps-editor")!).toBeNull());
  expect(state.writes[1]!).not.toHaveProperty("proxy");
  expect(state.profile.proxy).toBeNull();
});

test("keeping a whole proxy with an unavailable password refuses safely and retains the draft", async () => {
  const state = await open(null);
  proxyTab();
  fireEvent.change(screen.getByPlaceholderText("host.proxy.io"), { target: { value: "127.0.0.2" } });
  state.profile = { ...state.profile, proxy: null, _rev: 6 };
  fireEvent.click(save());
  await screen.findByText(/both changed/i);
  fireEvent.click(screen.getByRole<HTMLButtonElement>("button", { name: "Keep my edit" }));
  await screen.findByText(/password is unavailable/i);
  expect(save().disabled).toBe(true);
  expect(screen.getByPlaceholderText<HTMLInputElement>("host.proxy.io").value).toBe("127.0.0.2");
  expect(state.writes).toHaveLength(1);
  expect(state.profile.proxy).toBeNull();
  fireEvent.click(screen.getByRole<HTMLButtonElement>("button", { name: "Use latest" }));
  expect(save().disabled).toBe(false);
});

test("ProfileDiverged is not mistaken for a stale revision", async () => {
  const state = await open();
  state.failure = 409; state.failureKind = "ProfileDiverged";
  const readsBeforeSave = state.reads;
  fireEvent.change(notes(), { target: { value: "preserved draft" } });
  fireEvent.click(save());
  await screen.findByText(/Save failed:/);
  expect(notes().value).toBe("preserved draft");
  expect(state.reads).toBe(readsBeforeSave);
  expect(screen.queryByText(/rebased onto the latest profile/i)).toBeNull();
});

test("a failed stale refresh preserves the draft and can recover on a later attempt", async () => {
  const state = await open();
  state.profile = { ...state.profile, tags: ["remote"], _rev: 6 };
  state.failRefresh = true;
  fireEvent.change(notes(), { target: { value: "preserved draft" } });
  fireEvent.click(save());
  await screen.findByText(/Could not load the latest profile:/);
  expect(notes().value).toBe("preserved draft");
  state.failRefresh = false;
  fireEvent.click(save());
  await screen.findByText(/rebased onto the latest profile/i);
  fireEvent.click(save());
  await waitFor(() => expect(document.querySelector<HTMLElement>(".ps-editor")!).toBeNull());
  expect(state.writes.map(p => p._rev)).toEqual([5, 5, 6]);
  expect(state.profile).toMatchObject({ notes: "preserved draft", tags: ["remote"] });
});

test.each(["clear", "remove"])("423 retains proxy %s intent through retry", async intent => {
  const state = await open();
  state.failure = 423;
  proxyTab();
  if (intent === "clear") fireEvent.change(password(), { target: { value: "" } });
  else toggleProxy();
  fireEvent.click(save());
  await screen.findByText(/Save failed:/);
  if (intent === "clear") expect(password().value).toBe("");
  else expect(document.querySelector<HTMLButtonElement>(".ps-proxy-toggle button")!.textContent.trim()).toBe("Off");
  state.failure = null;
  fireEvent.click(save());
  await waitFor(() => expect(document.querySelector<HTMLElement>(".ps-editor")!).toBeNull());
  expect(state.writes).toHaveLength(2);
  expect(state.writes[1]!.proxy).toEqual(state.writes[0]!.proxy);
  if (intent === "clear") expect(state.writes[1]!.proxy!.pass).toBe("");
  else expect(state.writes[1]!.proxy).toBeNull();
});

test("pending save issues one mutation and retains the editor until commit", async () => {
  const state = await open();
  state.pending = true;
  const button = save();
  fireEvent.click(button);
  fireEvent.click(button);
  try {
    expect(state.writes).toHaveLength(1);
    expect(button.disabled).toBe(true);
    expect(notes()).toBeTruthy();
  } finally {
    await act(async () => state.finish!());
  }
  await waitFor(() => expect(document.querySelector<HTMLElement>(".ps-editor")!).toBeNull());
});

test("a committed save is not retried when the following list refresh fails", async () => {
  const state = await open();
  state.failRefresh = true;
  fireEvent.change(notes(), { target: { value: "committed draft" } });
  fireEvent.click(save());
  await waitFor(() => expect(document.querySelector<HTMLElement>(".ps-editor")!).toBeNull());
  expect(state.writes).toHaveLength(1);
  fireEvent.click(await screen.findByTitle("Edit"));
  expect(notes().value).toBe("committed draft");
});

test.each(["notes", "retype", "pool", "off-on"])("password intent follows editor actions: %s", async action => {
  const state = await open();
  if (action === "notes") fireEvent.change(notes(), { target: { value: "notes only" } });
  else {
    proxyTab();
    if (action === "retype") {
      fireEvent.change(password(), { target: { value: "" } });
      fireEvent.change(password(), { target: { value: "original" } });
    } else if (action === "pool") {
      await screen.findByText(/127.0.0.1:8081/);
      const picker = [...document.querySelectorAll<HTMLSelectElement>("select")].find(x => [...x.options].some(o => o.value === "pool1"));
      fireEvent.change(picker!, { target: { value: "pool1" } });
    } else {
      toggleProxy(); toggleProxy();
      fireEvent.change(screen.getByPlaceholderText("host.proxy.io"), { target: { value: "localhost" } });
      fireEvent.change(screen.getByPlaceholderText("8080"), { target: { value: "8080" } });
    }
  }
  fireEvent.click(save());
  await waitFor(() => expect(document.querySelector<HTMLElement>(".ps-editor")!).toBeNull());
  const sent = state.writes[0]!;
  expect(sent).not.toHaveProperty("_passwordSet");
  if (action === "notes") expect(sent).not.toHaveProperty("proxy");
  else expect(sent.proxy!.pass).toBe(action === "retype" ? "original" : action === "pool" ? "pooled" : "");
  fireEvent.click(await screen.findByTitle("Edit"));
  fireEvent.change(await screen.findByPlaceholderText(/worth remembering/i), { target: { value: "next draft" } });
  fireEvent.click(save());
  await waitFor(() => expect(state.writes).toHaveLength(2));
  expect(state.writes[1]!.proxy).toBeUndefined();
});
