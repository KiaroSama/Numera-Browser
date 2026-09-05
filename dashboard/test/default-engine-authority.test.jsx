/* The default launch engine belongs to the server, not to the profile you saved.
 *
 * `save()` ended with `if (p.engine) setDefaultEngine(p.engine)`, so every
 * successful save moved the local default to whatever engine that profile
 * happened to use. The backend is stricter - it changes the default only when
 * the profile's engine actually changed - so editing the notes of a Playwright
 * profile under a Camoufox default left the UI showing Playwright over a server
 * that had correctly left it alone. The next new profile then sent Playwright
 * explicitly, and the server's default was wrong too.
 *
 * The fake engine here enforces the real rule: a PUT that does not change the
 * engine does not touch config. If the app infers the default from the profile
 * again, Settings shows Playwright and this fails. */

import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import App from "../src/App.jsx";
import { auth } from "../src/api.js";

function engine() {
  const state = {
    config: { root: "/tmp/numera", version: "0.19.0", folders: [], default_engine: "camoufox" },
    profile: {
      id: "p1", name: "One", engine: "playwright", notes: "", tags: [],
      folder: "Facebook Ads", status: "stopped", lastActive: "never", _rev: 1,
      os: "Windows", browser: "Chrome", browserVersion: "128.0.0.0",
      userAgent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 "
                 + "(KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36",
      screen: "1920x1080", cores: 8, memory: 8,
      webglVendor: "Google Inc. (NVIDIA)", webglRenderer: "ANGLE (NVIDIA GeForce RTX 3060)",
      canvas: "Noise", webrtc: "Altered", audio: "Noise", fonts: "Masked",
      locale: "en-US", timezone: "America/New_York", geo: "Prompt",
      mediaVideo: 1, mediaAudio: 1, dnt: false, proxy: null,
      startupUrls: "", extensions: "", bookmarks: "", launchArgs: "", dns: "",
      ipBased: [], clearOnStart: [], switchedOff: [],
    },
    configWrites: [],
  };
  const send = (body, status = 200) => ({ ok: status < 400, status, json: async () => body });

  globalThis.fetch = vi.fn(async (url, init = {}) => {
    const path = String(url).split("?")[0];
    const method = init.method || "GET";

    if (path === "/api/health") return send({ ok: true });
    if (path === "/api/engines") return send({ playwright: true, camoufox: true });
    if (path === "/api/profiles" && method === "GET") return send([state.profile]);
    if (path === "/api/config" && method === "GET") return send(state.config);
    if (path === "/api/config" && method === "PUT") {
      const patch = JSON.parse(init.body);
      state.configWrites.push(patch);
      state.config = { ...state.config, ...patch };
      return send(state.config);
    }
    if (path === "/api/profiles/p1" && method === "PUT") {
      const sent = JSON.parse(init.body);
      // The server's rule: the global default follows the profile only when
      // the profile's engine actually changed.
      if (sent.engine && sent.engine !== state.profile.engine)
        state.config = { ...state.config, default_engine: sent.engine };
      state.profile = { ...state.profile, ...sent, _rev: state.profile._rev + 1 };
      return send(state.profile);
    }
    return send({ detail: "no such route: " + method + " " + path }, 404);
  });
  return state;
}

const online = () => screen.findByText("Engine online");
const settings = () => fireEvent.click(screen.getByRole("button", { name: "Settings" }));
const currentDefault = () =>
  document.querySelector(".ps-seg.wrap button.on")?.textContent?.trim();

async function editNotesOnly() {
  fireEvent.click(await screen.findByTitle("Edit"));
  const notes = await screen.findByPlaceholderText(/worth remembering/i);
  fireEvent.change(notes, { target: { value: "just a note" } });
  fireEvent.click(screen.getByRole("button", { name: /Save changes/ }));
}

beforeEach(() => { auth.clear(); });
afterEach(() => { cleanup(); auth.clear(); vi.restoreAllMocks(); });

test("editing notes does not move the default engine to the profile's engine", async () => {
  const state = engine();
  render(<App />);
  await online();

  settings();
  expect(currentDefault()).toMatch(/Camoufox/);

  fireEvent.click(screen.getByRole("button", { name: "Profiles" }));
  await editNotesOnly();

  await waitFor(() => expect(state.profile.notes).toBe("just a note"));
  expect(state.configWrites).toEqual([]);          // the server was never asked to change it

  settings();
  await waitFor(() => expect(currentDefault()).toMatch(/Camoufox/));
});

test("changing a profile's engine does move the default, because the server did", async () => {
  const state = engine();
  render(<App />);
  await online();

  fireEvent.click(await screen.findByTitle("Edit"));
  await screen.findByPlaceholderText(/worth remembering/i);
  const picker = [...document.querySelectorAll("select")]
    .find(el => el.value === "playwright");
  expect(picker, "the editor's engine picker").toBeTruthy();
  fireEvent.change(picker, { target: { value: "camoufox" } });
  fireEvent.click(screen.getByRole("button", { name: /Save changes/ }));

  await waitFor(() => expect(state.profile.engine).toBe("camoufox"));
  settings();
  await waitFor(() => expect(currentDefault()).toMatch(/Camoufox/));
  expect(state.config.default_engine).toBe("camoufox");
});
