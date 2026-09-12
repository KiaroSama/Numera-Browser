/* An extension's logo comes out of a protected endpoint.
 *
 * `<img src="/api/extensions/icon?path=…">` is a request the dashboard does not
 * make and cannot decorate: the browser issues it, and the browser knows nothing
 * about the bearer token this app keeps in localStorage. Against a locked engine
 * - which is every engine that has a token - the image came back 401 and every
 * row fell back to the grey puzzle glyph while all the other requests on the
 * same page authenticated perfectly.
 *
 * The fix has to go through the authenticated client and hand the <img> a blob
 * URL, and blob URLs are owned resources: one has to be revoked when the row
 * changes path or leaves the screen, or the page leaks a buffer per icon.
 *
 * The fake engine here refuses the icon route without an Authorization header,
 * so an icon that renders is proof the request carried the credential.
 */

import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import App from "../src/App.jsx";
import { auth } from "../src/api.js";

const TOKEN = "nmr_dashboard";
const PATH = "C:\\ext\\ublock";

const response = (body, status = 200) => ({ ok: status < 400, status, json: async () => body });
const bytes = (body) => ({
  ok: true, status: 200,
  json: async () => { throw new Error("icon is not JSON"); },
  blob: async () => new Blob([body], { type: "image/png" }),
});

function engine({ icon = "PNGDATA", meta = { path: PATH, name: "uBlock Origin", icon: "icons/128.png" } } = {}) {
  const state = { iconRequests: [] };
  vi.stubGlobal("fetch", vi.fn(async (url, init = {}) => {
    const full = String(url);
    const path = full.split("?")[0];
    const bearer = (init.headers?.Authorization || "").replace(/^Bearer /, "");
    if (path === "/api/health") return response({ ok: true });
    if (path === "/api/engines") return response({ playwright: true });
    if (path === "/api/profiles") return response([]);
    if (path === "/api/config")
      return response({ default_engine: "playwright", folders: [], default_extensions: [PATH] });
    if (path === "/api/extensions/installed")
      return response({ extensions: meta ? [meta] : [] });
    if (path === "/api/extensions/icon") {
      state.iconRequests.push({ url: full, bearer });
      // Exactly what the engine does: no credential, no icon.
      if (bearer !== TOKEN) return response({ detail: "invalid or missing API token" }, 401);
      if (!icon) return response({ detail: "no icon in this manifest" }, 404);
      return bytes(icon);
    }
    return response({ detail: `Unexpected test route ${path}` }, 404);
  }));
  return state;
}

/* Open the Extensions tab and wait for the row itself, which renders whether or
 * not its icon ever arrives - so the wait cannot mask a missing request. */
async function openExtensions() {
  render(<App />);
  await screen.findByText("Engine online");
  fireEvent.click(screen.getByRole("button", { name: "Extensions" }));
  await screen.findByText("uBlock Origin");
}

let revoked;
beforeEach(() => {
  auth.set(TOKEN);
  revoked = [];
  URL.createObjectURL = vi.fn(() => "blob:test/icon");
  URL.revokeObjectURL = vi.fn(url => revoked.push(url));
});
afterEach(() => { cleanup(); auth.clear(); vi.unstubAllGlobals(); vi.restoreAllMocks(); });

test("the icon is fetched with the app's credential, not by a bare <img>", async () => {
  const state = await engineAndOpen();

  // The request happens at all, and it carries the same bearer token every
  // other call on this page uses. A src= attribute produced neither.
  await waitFor(() => expect(state.iconRequests).toHaveLength(1));
  expect(state.iconRequests[0].bearer).toBe(TOKEN);
  expect(state.iconRequests[0].url).toContain(encodeURIComponent(PATH));
  // And no token may ride along in the URL, where it would land in logs.
  expect(state.iconRequests[0].url).not.toContain(TOKEN);

  const img = await waitFor(() => {
    const el = document.querySelector("img.ps-ext-logo");
    expect(el).toBeTruthy();
    return el;
  });
  expect(img.getAttribute("src")).toBe("blob:test/icon");
  expect(img.getAttribute("src")).not.toContain("/api/extensions/icon");
});

test("the object URL is released when the row goes away", async () => {
  await engineAndOpen();
  await waitFor(() => expect(document.querySelector("img.ps-ext-logo")).toBeTruthy());

  // Leaving the tab unmounts the row. Nothing else will ever free the buffer.
  fireEvent.click(screen.getByRole("button", { name: "Profiles" }));
  await waitFor(() => expect(revoked).toContain("blob:test/icon"));
});

test("an extension whose icon is missing falls back instead of breaking the row", async () => {
  const state = engine({ icon: "" });   // the engine has the manifest but no image
  await openExtensions();

  await waitFor(() => expect(state.iconRequests).toHaveLength(1));
  expect(document.querySelector("img.ps-ext-logo")).toBeNull();
  expect(document.querySelector(".ps-ext-ic svg")).toBeTruthy();   // the glyph
});

async function engineAndOpen() {
  const state = engine();
  await openExtensions();
  return state;
}
