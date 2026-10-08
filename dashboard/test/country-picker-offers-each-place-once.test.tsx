import { response } from "./helpers";
/* A picker must not offer an alias as if it were a second place.
 *
 * `COUNTRY_LOCALE` maps a country code to a locale and deliberately accepts
 * `UK` as well as `GB`: ISO 3166-1 alpha-2 spells the United Kingdom `GB`, but
 * proxy lists and CSV exports write `UK`, so the lookup tolerates both and the
 * engine's `devices.py` carries the same pair.
 *
 * Bulk create built its country chips from `Object.keys` of that table, so the
 * alias became a chip: `GB` and `UK` stood side by side as two choices for one
 * country. Selecting both spread a single place across two stored values, and
 * everything that later compares `proxy.country` - `coherence()` among them -
 * reads those as different. The chips now come from `COUNTRIES`, the canonical
 * list; this pins both halves, because dropping `UK` from the LOOKUP would be
 * the opposite bug and silently break every profile that already stores it.
 */

import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import App from "../src/App.tsx";
import { auth } from "../src/api.ts";
import { seed } from "../src/demoData";
import { COUNTRIES, COUNTRY_LOCALE } from "../src/deviceData.ts";



function engine() {
  vi.stubGlobal("fetch", vi.fn(async (url: RequestInfo | URL) => {
    const path = String(url).split("?")[0]!;
    if (path === "/api/health") return response({ ok: true });
    if (path === "/api/engines") return response({ playwright: true });
    if (path === "/api/config") return response({ default_engine: "playwright", folders: [] });
    if (path === "/api/profiles") return response([]);
    return response([]);
  }));
}

beforeEach(() => { auth.clear(); });
afterEach(() => { cleanup(); auth.clear(); vi.unstubAllGlobals(); vi.restoreAllMocks(); });

test("the lookup still accepts the alias, and it resolves where GB does", () => {
  expect(COUNTRY_LOCALE.UK, "dropping the alias breaks stored profiles").toBe("en-GB");
  expect(COUNTRY_LOCALE.UK).toBe(COUNTRY_LOCALE.GB);
});

test("no two offered countries share a locale, and the alias is not offered", () => {
  expect(COUNTRIES).not.toContain("UK");
  expect(COUNTRIES).toContain("GB");
  const locales = COUNTRIES.map(c => COUNTRY_LOCALE[c]);
  expect(new Set(locales).size, `two countries offer one locale: ${locales.join(", ")}`)
    .toBe(COUNTRIES.length);
});

/* The same table, the same mistake, a second time.
 *
 * Found by opening the editor against a live engine: the proxy tab's Country
 * select was still built from `Object.keys(COUNTRY_LOCALE)`, so `GB` and `UK`
 * sat one above the other there long after the bulk dialog was fixed. One
 * picker being right is not the rule holding. */
test("the editor's proxy country select offers each country once", async () => {
  const stored = {
    ...seed[0],
    id: "p1", name: "Has proxy", folder: "All profiles", status: "stopped",
    os: "Windows", tags: [], engine: "playwright", browser: "Chrome",
    browserVersion: "128.0.0.0", userAgent: "UA", screen: "1920x1080",
    cores: 8, memory: 16, locale: "en-US", timezone: "America/New_York",
    proxy: { type: "HTTP", host: "h", port: "1", user: "", pass: "", country: "US" },
    ipBased: [], clearOnStart: [], switchedOff: [], lastActive: "never",
  };
  vi.stubGlobal("fetch", vi.fn(async (url: RequestInfo | URL) => {
    const path = String(url).split("?")[0]!;
    if (path === "/api/health") return response({ ok: true });
    if (path === "/api/engines") return response({ playwright: true });
    if (path === "/api/config") return response({ default_engine: "playwright", folders: [] });
    if (path === "/api/profiles") return response([stored]);
    return response([]);
  }));
  render(<App />);
  await screen.findByText("Engine online");

  fireEvent.click(await screen.findByTitle("Edit"));
  fireEvent.click(await screen.findByText("proxy"));

  const select = await screen.findByRole<HTMLSelectElement>("combobox", { name: "Country" });
  const offered = [...select.options].map(o => o.value);
  expect(offered, "the alias is offered as a country").not.toContain("UK");
  expect(offered).toContain("GB");
  expect(new Set(offered).size, `a country is offered twice: ${offered.join(", ")}`)
    .toBe(offered.length);
});

test("a profile that already stores the alias still shows it", async () => {
  // Dropping `UK` from the LIST must not blank the field of every profile that
  // was created back when the list offered it.
  const stored = {
    ...seed[0],
    id: "p2", name: "Stored UK", folder: "All profiles", status: "stopped",
    os: "Windows", tags: [], engine: "playwright", browser: "Chrome",
    browserVersion: "128.0.0.0", userAgent: "UA", screen: "1920x1080",
    cores: 8, memory: 16, locale: "en-GB", timezone: "Europe/London",
    proxy: { type: "HTTP", host: "h", port: "1", user: "", pass: "", country: "UK" },
    ipBased: [], clearOnStart: [], switchedOff: [], lastActive: "never",
  };
  vi.stubGlobal("fetch", vi.fn(async (url: RequestInfo | URL) => {
    const path = String(url).split("?")[0]!;
    if (path === "/api/health") return response({ ok: true });
    if (path === "/api/engines") return response({ playwright: true });
    if (path === "/api/config") return response({ default_engine: "playwright", folders: [] });
    if (path === "/api/profiles") return response([stored]);
    return response([]);
  }));
  render(<App />);
  await screen.findByText("Engine online");
  fireEvent.click(await screen.findByTitle("Edit"));
  fireEvent.click(await screen.findByText("proxy"));

  const select = await screen.findByRole<HTMLSelectElement>("combobox", { name: "Country" });
  expect(select.value, "the stored country silently changed").toBe("UK");
});

test("the bulk dialog offers each country once", async () => {
  engine();
  render(<App />);
  await screen.findByText("Engine online");
  fireEvent.click(screen.getByText("Bulk create"));

  const field = await screen.findByText(/Countries \/ locales/i);
  const chips = within(field.closest(".ps-field")! || field.parentElement)
    .getAllByRole("button").map(b => b.textContent.trim());

  expect(chips, "the alias is offered as a country").not.toContain("UK");
  expect(chips).toContain("GB");
  expect(new Set(chips).size, `a country is offered twice: ${chips.join(", ")}`)
    .toBe(chips.length);
  await waitFor(() => expect(chips.length).toBe(COUNTRIES.length));
});
