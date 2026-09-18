/* Every sidebar button lands on a real screen.
 *
 * `App.jsx` holds the nav as one array of `[key, Icon, label]` and renders a
 * view with `view === "<key>" && <SomeView/>`. Those are two lists, edited
 * separately, and nothing tied them together - so a key could sit in the nav
 * with no view behind it and the only sign was a "Soon / On the roadmap" panel
 * that read like a feature in progress rather than a dead button.
 *
 * `team` was exactly that: in the nav, absent from the render list, backed by
 * no endpoint, no identity and no role anywhere in the engine - and team/members
 * is outside this product's scope by an explicit decision (`.ai/GOLOGIN_PARITY`).
 * It is gone now. This test is what stops the next one appearing: it does not
 * know the nav's contents, it reads the buttons out of the rendered sidebar and
 * presses each of them.
 *
 * What it asserts is deliberately narrow - that pressing a nav button produces a
 * screen, not the unknown-view fallback and not the error boundary. It is NOT a
 * check that each view's data loads, which is why the fake below answers any
 * unrecognised GET with something empty and harmless: a view failing to fetch is
 * a different test's subject, and letting it fail here would turn this one into a
 * flaky proxy for all of them.
 */

import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import App from "../src/App.jsx";
import { auth } from "../src/api.js";

const response = (body, status = 200) => ({ ok: status < 400, status, json: async () => body });

/* Permissive on purpose - see the header. The four routes App needs to finish
 * booting are answered exactly; everything else gets an empty shape that both
 * a list-reader and an object-reader can survive. */
function engine() {
  vi.stubGlobal("fetch", vi.fn(async (url) => {
    const path = String(url).split("?")[0];
    if (path === "/api/health") return response({ ok: true });
    if (path === "/api/engines") return response({ playwright: true });
    if (path === "/api/config") return response({ default_engine: "playwright", folders: [] });
    if (path === "/api/profiles") return response([]);
    return response([]);
  }));
}

beforeEach(() => { auth.clear(); });
afterEach(() => { cleanup(); auth.clear(); vi.unstubAllGlobals(); vi.restoreAllMocks(); });

const navButtons = () => [...document.querySelectorAll(".ps-navitem")];

test("every nav button opens a screen, none falls through to the fallback", async () => {
  engine();
  render(<App />);
  await screen.findByText("Engine online");

  const labels = navButtons().map(b => b.textContent.trim());
  expect(labels.length, "the sidebar rendered no nav buttons").toBeGreaterThan(0);

  for (const label of labels) {
    const button = navButtons().find(b => b.textContent.trim() === label);
    expect(button, `nav button ${label} disappeared between presses`).toBeTruthy();
    fireEvent.click(button);

    // The pressed item is the active one: proves the click was taken, so a
    // "no fallback" pass below cannot come from nothing having happened.
    await waitFor(() => expect(
      navButtons().find(b => b.textContent.trim() === label).className,
    ).toContain("active"));

    expect(screen.queryByText("Nothing here"),
      `"${label}" is in the sidebar with no screen behind it`).toBeNull();
    expect(screen.queryByText("This view couldn't be drawn"),
      `"${label}" threw while rendering`).toBeNull();
  }
});

test("the fallback still answers a view name nothing renders", async () => {
  /* The guard above is only worth having if the fallback it looks for really
   * appears when a key has no screen - otherwise both assertions would pass on
   * an app that had quietly stopped rendering the fallback at all. The Folders
   * sidebar sets `view` through the same setter, so pressing "Manage" and then
   * a nav button exercises the same path; here the unknown key is reached the
   * only way left, through the component itself. */
  const { Placeholder } = await import("../src/ui.jsx");
  render(<Placeholder view="team" />);
  expect(screen.getByText("Nothing here")).toBeTruthy();
  expect(screen.getByText(/asked for "team"/)).toBeTruthy();
  expect(screen.queryByText(/roadmap/i), "the fallback still promises the feature").toBeNull();
});
