/* A camoufox profile must not be presented as a Chrome.
 *
 * `drivers.py:_camoufox_kwargs` passes Camoufox four things - `os`, `locale`,
 * the proxy, and `geoip` when the profile asked to follow its exit IP - and
 * generates the browser, its version, the user agent, the screen, the cores,
 * the memory and every WebGL/canvas/audio/font signal itself, as a Firefox.
 *
 * The dashboard printed "Chrome 128" in the profile row and a settled
 * user agent, resolution and WebGL vendor in the summary anyway, and graded the
 * profile "Coherent" on fields the engine never receives. Camoufox is the
 * recommended engine, so that was the common reading of the product, not a
 * corner case.
 *
 * What is pinned here: the claim is gone, the values are still there (switching
 * to a Chromium engine makes them live again), and the coherence check stops
 * scoring signals the engine is not given while still scoring the ones it is.
 */

import { afterEach, expect, test } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { ProfileSummary } from "../src/views/ProfileSummary.jsx";
import { coherence, generatesOwnIdentity } from "../src/deviceData.js";

const base = {
  name: "Ads 01", folder: "Ads", os: "Windows", browser: "Chrome", browserVersion: "128.0.0.0",
  userAgent: "Mozilla/5.0 … Chrome/128.0.0.0", screen: "1920x1080", cores: 8, memory: 16,
  locale: "en-US", timezone: "America/New_York", geo: "Prompt",
  webglVendor: "Google Inc. (NVIDIA)", webglRenderer: "ANGLE (NVIDIA GeForce RTX 3060 Direct3D11)",
  canvas: "Noise", webrtc: "Altered", audio: "Noise", fonts: "Masked", proxy: null,
};

afterEach(() => cleanup());

test("camoufox is the engine that builds its own identity; chromium ones do not", () => {
  expect(generatesOwnIdentity("camoufox")).toBe(true);
  for (const e of ["cloak", "patchright", "playwright", null, undefined])
    expect(generatesOwnIdentity(e), `${e} was treated as self-identifying`).toBe(false);
});

test("the summary stops claiming Chrome, and says who generates the rest", () => {
  render(<ProfileSummary p={{ ...base, engine: "camoufox" }} issues={[]} score={100} />);
  expect(screen.queryByText(/Chrome 128/), "the profile is still presented as a Chrome").toBeNull();
  expect(screen.getByText(/Firefox/)).toBeTruthy();
  expect(screen.getByText(/builds its own fingerprint/)).toBeTruthy();
  // The stored values are marked, not hidden - a Chromium engine still uses them.
  expect(screen.getAllByText(/· generated/).length).toBeGreaterThan(5);
});

test("a chromium engine is presented exactly as before", () => {
  render(<ProfileSummary p={{ ...base, engine: "cloak" }} issues={[]} score={100} />);
  expect(screen.getByText("Chrome 128.0.0.0")).toBeTruthy();
  expect(screen.queryByText(/builds its own fingerprint/)).toBeNull();
  expect(screen.queryAllByText(/· generated/)).toHaveLength(0);
});

test("coherence grades only the signals the engine is actually given", () => {
  // A GPU that belongs to no Windows machine, and a screen that belongs to none.
  const wrong = { ...base, webglRenderer: "ANGLE (Apple M1, OpenGL 4.1)", screen: "1x1", cores: 999 };

  expect(coherence({ ...wrong, engine: "cloak" }).length,
    "a Chromium engine must still be graded on hardware").toBeGreaterThan(0);
  expect(coherence({ ...wrong, engine: "camoufox" }),
    "camoufox was graded on a GPU, screen and core count it never receives").toEqual([]);
});

test("locale, timezone and proxy country are still graded on camoufox", () => {
  // These three ARE passed through, so a contradiction here is a real leak.
  const mismatched = { ...base, engine: "camoufox", locale: "en-US", timezone: "Europe/Berlin" };
  expect(coherence(mismatched)).toContain("Timezone doesn't match locale");

  const wrongCountry = { ...base, engine: "camoufox", locale: "en-US", proxy: { country: "DE", type: "http", host: "h", port: 1 } };
  expect(coherence(wrongCountry).some(i => i.includes("proxy country")),
    "a proxy country that disagrees with the locale went unflagged").toBe(true);
});
