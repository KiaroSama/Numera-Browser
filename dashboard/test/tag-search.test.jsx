/* Search matches names case-insensitively and tags case-sensitively.
 *
 * `p.name.toLowerCase().includes(q) || p.tags.some(t => t.includes(q))` - the
 * query is lowered for both, but the tag is not, so a profile tagged "Work" is
 * unreachable by typing "work", "Work" or "WORK": the first two miss on case
 * and the third misses because the query itself was lowered. The tag chips in
 * the row are clickable and do match, which is what kept this quiet.
 *
 * The same expression is the app's only guard around legacy tag data: `t` is
 * assumed to be a string, and a profile whose stored tags came from an older
 * import can hold a number. `t.includes` then throws inside App's `filtered`,
 * which is computed above the error boundary - so one bad record does not
 * degrade a view, it blanks the application.
 */

import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import App from "../src/App.jsx";
import { auth } from "../src/api.js";
import { newProfile } from "../src/ui.jsx";

const response = (body, status = 200) => ({ ok: status < 400, status, json: async () => body });

const profile = (name, tags) => ({
  ...newProfile("Facebook Ads", "playwright"),
  id: name, name, tags, status: "stopped", lastActive: "never", _new: false,
});

function engine(profiles) {
  vi.stubGlobal("fetch", vi.fn(async (url) => {
    const path = String(url).split("?")[0];
    if (path === "/api/health") return response({ ok: true });
    if (path === "/api/engines") return response({ playwright: true });
    if (path === "/api/config") return response({ default_engine: "playwright", folders: [] });
    if (path === "/api/profiles") return response(profiles);
    return response({ detail: `Unexpected test route ${path}` }, 404);
  }));
}

const search = (text) =>
  fireEvent.change(screen.getByPlaceholderText(/Search profiles/), { target: { value: text } });
const shown = () => [...document.querySelectorAll("tbody .ps-pname")].map(el => el.textContent.trim());

beforeEach(() => { auth.clear(); });
afterEach(() => { cleanup(); auth.clear(); vi.unstubAllGlobals(); vi.restoreAllMocks(); });

async function open(profiles) {
  engine(profiles);
  render(<App />);
  await screen.findByText("Engine online");
  await screen.findByText(profiles[0].name);
}

test.each([["work"], ["Work"], ["WORK"], ["WoRk"]])(
  "searching %s finds the profile tagged Work", async (query) => {
    await open([profile("tagged", ["Work"]), profile("other", ["personal"])]);

    search(query);
    // Both operands are normalised for the comparison, so the case someone
    // types and the case someone stored are independent.
    await waitFor(() => expect(shown()).toEqual(["tagged"]));
  });

test("the stored tag keeps its own capitalisation", async () => {
  await open([profile("tagged", ["Work"])]);

  search("work");
  await waitFor(() => expect(shown()).toEqual(["tagged"]));
  // Matching lowercases a copy; it must never write the lowered form back onto
  // the profile, where the next save would persist it.
  expect(screen.getByText("Work")).toBeTruthy();
  expect(screen.queryByText("work")).toBeNull();
});

test("a legacy non-string tag does not take the application down", async () => {
  // A number, not an object: an object tag additionally breaks *rendering*,
  // which is a different defect. This isolates the search expression.
  await open([profile("legacy", [42]), profile("normal", ["work"])]);

  // Reaching this line at all is the assertion - `filtered` runs on every
  // render, so an unguarded `t.includes` throws before anything is drawn.
  expect(shown()).toEqual(["legacy", "normal"]);

  search("work");
  await waitFor(() => expect(shown()).toEqual(["normal"]));
  search("legacy");
  await waitFor(() => expect(shown()).toEqual(["legacy"]));   // still findable by name
});
