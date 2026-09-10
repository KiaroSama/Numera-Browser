import { afterEach, expect, test, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import App from "../src/App.jsx";
import { newProfile } from "../src/ui.jsx";
import { auth } from "../src/api.js";

const response = (body, status = 200) => ({ ok: status < 400, status, json: async () => body });
const notes = () => screen.getByPlaceholderText(/worth remembering/i);
const save = () => screen.getByRole("button", { name: /Save changes/ });
const proxyTab = () => fireEvent.click(screen.getByRole("button", { name: "proxy", exact: true }));
const password = () => document.querySelector('.ps-editor input[type="password"]');
const toggleProxy = () => fireEvent.click(document.querySelector(".ps-proxy-toggle button"));

async function open() {
  auth.clear();
  const state = {
    profile: { ...newProfile("Facebook Ads", "playwright"), id: "p1", _new: false,
      name: "One", _rev: 1, notes: "", proxy: {
        type: "HTTP", host: "localhost", port: "8080", user: "u", pass: "original", country: "US",
      } },
    writes: [], failure: null, pending: false, finish: null, reads: 0, failRefresh: false,
  };
  vi.stubGlobal("fetch", vi.fn(async (url, init = {}) => {
    const method = init.method || "GET";
    const path = String(url).split("?")[0];
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
      const sent = JSON.parse(init.body);
      state.writes.push(sent);
      if (state.pending) await new Promise(resolve => { state.finish = resolve; });
      if (state.failure === "network") throw new TypeError("Network unavailable");
      if (state.failure) return response({ detail: `Rejected ${state.failure}` }, state.failure);
      const proxy = sent.proxy ? { ...state.profile.proxy, ...sent.proxy } : null;
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

test.each(["clear", "remove"])("423 retains proxy %s intent through retry", async intent => {
  const state = await open();
  state.failure = 423;
  proxyTab();
  if (intent === "clear") fireEvent.change(password(), { target: { value: "" } });
  else toggleProxy();
  fireEvent.click(save());
  await screen.findByText(/Save failed:/);
  if (intent === "clear") expect(password().value).toBe("");
  else expect(document.querySelector(".ps-proxy-toggle button").textContent.trim()).toBe("Off");
  state.failure = null;
  fireEvent.click(save());
  await waitFor(() => expect(document.querySelector(".ps-editor")).toBeNull());
  expect(state.writes).toHaveLength(2);
  expect(state.writes[1].proxy).toEqual(state.writes[0].proxy);
  if (intent === "clear") expect(state.writes[1].proxy.pass).toBe("");
  else expect(state.writes[1].proxy).toBeNull();
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
    await act(async () => state.finish());
  }
  await waitFor(() => expect(document.querySelector(".ps-editor")).toBeNull());
});

test("a committed save is not retried when the following list refresh fails", async () => {
  const state = await open();
  state.failRefresh = true;
  fireEvent.change(notes(), { target: { value: "committed draft" } });
  fireEvent.click(save());
  await waitFor(() => expect(document.querySelector(".ps-editor")).toBeNull());
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
      const picker = [...document.querySelectorAll("select")].find(x => [...x.options].some(o => o.value === "pool1"));
      fireEvent.change(picker, { target: { value: "pool1" } });
    } else {
      toggleProxy(); toggleProxy();
      fireEvent.change(screen.getByPlaceholderText("host.proxy.io"), { target: { value: "localhost" } });
      fireEvent.change(screen.getByPlaceholderText("8080"), { target: { value: "8080" } });
    }
  }
  fireEvent.click(save());
  await waitFor(() => expect(document.querySelector(".ps-editor")).toBeNull());
  const sent = state.writes[0];
  expect(sent).not.toHaveProperty("_passwordSet");
  if (action === "notes") expect(sent.proxy).not.toHaveProperty("pass");
  else expect(sent.proxy.pass).toBe(action === "retype" ? "original" : action === "pool" ? "pooled" : "");
  fireEvent.click(await screen.findByTitle("Edit"));
  fireEvent.change(await screen.findByPlaceholderText(/worth remembering/i), { target: { value: "next draft" } });
  fireEvent.click(save());
  await waitFor(() => expect(state.writes).toHaveLength(2));
  expect(state.writes[1].proxy).not.toHaveProperty("pass");
});
