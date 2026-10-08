import { response } from "./helpers";
/* A profile name an older build stored must not blank the application.
 *
 * `App.jsx` computes `filtered` above the error boundary, and its search
 * expression is `p.name.toLowerCase().includes(q)`. The tag half of that line
 * is guarded (`typeof t === "string"`); the name half is not - so a record
 * whose stored name is a number, a list or absent throws inside a value the
 * whole page depends on, and one bad profile blanks every profile.
 *
 * The engine now refuses to *write* such a name (N04) and repairs the *view*
 * of one already stored: `write_contract.readable` turns `123` into `"123"`,
 * `null` and a missing key into `""`, and a list into `""`. So the shapes in
 * the first test are what `GET /api/profiles` actually answers for the damaged
 * records in `engine/tests/test_write_contract_boundaries.py::DAMAGED` - this
 * is the other end of that assertion, and it is the one that says the repair
 * chose shapes the UI can really render.
 *
 * The last test feeds the raw damage instead, on the principle that a client
 * which blanks when a server contract is broken is one deploy-skew away from
 * blanking for real.
 */

import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import App from "../src/App.tsx";
import { auth } from "../src/api.ts";
import { newProfile } from "../src/ui.tsx";



const profile = (id: string, name: string | number | null) => ({
  ...newProfile("Facebook Ads", "playwright"),
  id, name, tags: [], status: "stopped", lastActive: "never", _new: false,
});

function engine(profiles: ReturnType<typeof profile>[]) {
  vi.stubGlobal("fetch", vi.fn(async (url: RequestInfo | URL) => {
    const path = String(url).split("?")[0]!;
    if (path === "/api/health") return response({ ok: true });
    if (path === "/api/engines") return response({ playwright: true });
    if (path === "/api/config") return response({ default_engine: "playwright", folders: [] });
    if (path === "/api/profiles") return response(profiles);
    return response({ detail: `Unexpected test route ${path}` }, 404);
  }));
}

const search = (text: string) =>
  fireEvent.change(screen.getByPlaceholderText(/Search profiles/), { target: { value: text } });
const rows = () => [...document.querySelectorAll<HTMLElement>("tbody .ps-pname")].map(el => (el.textContent ?? "").trim());

beforeEach(() => { auth.clear(); });
afterEach(() => { cleanup(); auth.clear(); vi.unstubAllGlobals(); vi.restoreAllMocks(); });

async function open(profiles: ReturnType<typeof profile>[]) {
  engine(profiles);
  render(<App />);
  await screen.findByText("Engine online");
  await waitFor(() => expect(document.querySelectorAll<HTMLElement>("tbody .ps-pname").length)
    .toBe(profiles.length));
}

test("the repaired view of damaged records renders every row", async () => {
  // Exactly what the API answers for records stored as 123, null, absent and
  // ["a"] respectively - plus one that was never damaged.
  await open([
    profile("aaaaaaaaaaaa", "123"),
    profile("bbbbbbbbbbbb", ""),
    profile("cccccccccccc", ""),
    profile("dddddddddddd", ""),
    profile("eeeeeeeeeeee", "healthy"),
  ]);

  expect(rows()).toEqual(["123", "", "", "", "healthy"]);
});

test("search still works across the repaired names", async () => {
  await open([
    profile("aaaaaaaaaaaa", "123"),
    profile("bbbbbbbbbbbb", ""),
    profile("eeeeeeeeeeee", "healthy"),
  ]);

  search("healthy");
  await waitFor(() => expect(rows()).toEqual(["healthy"]));

  // A repaired numeric name is still findable by the text it now shows, which
  // is the point of repairing to `"123"` rather than dropping the field.
  search("123");
  await waitFor(() => expect(rows()).toEqual(["123"]));
});

test("an unrepaired name does not take the application down", async () => {
  // The server contract says this cannot arrive. A client that blanks when it
  // does is one deploy-skew - an older engine, a hand-edited file served by a
  // proxy, a future field - away from blanking for real, and the row it cannot
  // render is not a reason to render none of the others.
  await open([
    profile("aaaaaaaaaaaa", 123),
    profile("bbbbbbbbbbbb", null),
    profile("eeeeeeeeeeee", "healthy"),
  ]);

  // Reaching this line at all is the assertion: `filtered` runs on every
  // render, above the error boundary.
  expect(rows()).toContain("healthy");

  search("healthy");
  await waitFor(() => expect(rows()).toEqual(["healthy"]));
});
