/* The dashboard locking itself out of a running engine.
 *
 * Auth is evaluated per request, so the API can change its mind underneath a
 * session that is already open: minting the first named token locks it, and
 * revoking the token in use invalidates the credential this tab is holding.
 * Both used to be swallowed - one by the token list's own catch, one by the
 * poller's `refresh().catch(() => {})` - leaving the app visibly "Engine
 * online" while every request it made came back 401. */

import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import App from "../src/App.jsx";
import { auth } from "../src/api.js";

/* A fake engine that enforces auth the way the real one does: per request, with
 * the token store readable and writable through /api/tokens. A mock that always
 * answered 200 would pass whether or not the app kept a usable credential. */
function engine(tokens, envToken = false) {
  const state = { tokens, envToken };
  const send = (body, status = 200) => ({
    ok: status < 400, status, json: async () => body,
  });

  globalThis.fetch = vi.fn(async (url, init = {}) => {
    const path = String(url).split("?")[0];
    const method = init.method || "GET";
    const sent = (init.headers?.Authorization || "").replace(/^Bearer /, "");

    if ((state.tokens.length > 0 || state.envToken) && !state.tokens.some(t => t.value === sent))
      return send({ detail: "invalid or missing API token" }, 401);

    if (path === "/api/health") return send({ ok: true });
    if (path === "/api/profiles") return send([]);
    if (path === "/api/engines") return send({ playwright: true });
    if (path === "/api/config") return send({ root: "/tmp/numera", version: "0.11.1", folders: [] });
    if (path === "/api/tokens" && method === "GET")
      return send({ tokens: state.tokens.map(({ value, ...t }) => t), envToken: state.envToken });
    if (path === "/api/tokens" && method === "POST") {
      const name = JSON.parse(init.body).name;
      const made = { id: "t" + (state.tokens.length + 1), name, value: "nmr_" + name, created: 1, lastUsed: null };
      state.tokens.push(made);
      return send({ id: made.id, name, token: made.value, created: made.created });
    }
    if (path.startsWith("/api/tokens/") && method === "DELETE") {
      state.tokens = state.tokens.filter(t => t.id !== path.split("/").pop());
      return send(null, 204);
    }
    return send({ detail: "no such route" }, 404);
  });
  return state;
}

const openSettings = async () => {
  await screen.findByText("Engine online");
  fireEvent.click(screen.getByRole("button", { name: "Settings" }));
};

beforeEach(() => { auth.clear(); });
afterEach(() => { cleanup(); auth.clear(); });

test("minting the first token keeps this session authorised to its own engine", async () => {
  engine([]);                                  // open API: no tokens, no env token
  render(<App />);
  await openSettings();

  fireEvent.change(await screen.findByPlaceholderText(/What is it for/),
                   { target: { value: "dashboard" } });
  fireEvent.click(screen.getByRole("button", { name: "Create" }));

  // Shown once and only once, so being thrown to the locked screen here loses it.
  expect(await screen.findByText("nmr_dashboard")).toBeTruthy();
  // The reload after minting needs the new token: this line only renders when
  // the list came back, and the app is still live rather than locked out.
  expect(await screen.findByText("The API now requires one of these on every request.")).toBeTruthy();
  expect(screen.queryByText("This engine needs a token")).toBeNull();
  expect(auth.get()).toBe("nmr_dashboard");
});

test("minting a second token leaves the credential this session already has", async () => {
  engine([{ id: "t1", name: "dashboard", value: "nmr_dashboard", created: 1, lastUsed: null }]);
  auth.set("nmr_dashboard");
  render(<App />);
  await openSettings();

  fireEvent.change(await screen.findByPlaceholderText(/What is it for/),
                   { target: { value: "ci" } });
  fireEvent.click(screen.getByRole("button", { name: "Create" }));
  await screen.findByText("nmr_ci");

  // Adopting it would hand the dashboard a token minted for something else, so
  // revoking the CI runner would lock the dashboard out - the same defect again.
  expect(auth.get()).toBe("nmr_dashboard");
});

test("revoking the token in use shows the locked screen instead of staying live", async () => {
  engine([
    { id: "t1", name: "dashboard", value: "nmr_dashboard", created: 1, lastUsed: null },
    { id: "t2", name: "ci", value: "nmr_ci", created: 1, lastUsed: null },
  ]);
  auth.set("nmr_dashboard");
  render(<App />);
  await openSettings();

  fireEvent.click((await screen.findAllByTitle("Revoke"))[0]);
  await screen.findByText("Revoke this token?");
  fireEvent.click(document.querySelector(".ps-overlay .ps-btn.danger"));

  expect(await screen.findByText("This engine needs a token")).toBeTruthy();
});
