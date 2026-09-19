/* The two Health row actions have to report what they did.
 *
 * Found by pressing them against a live engine. Both open a real browser and
 * write to the profile - the trust check re-graded a profile from B to C, and
 * auto-adjust rewrote its OS from macOS to Windows, its screen, its memory and
 * its WebGL renderer to the host machine's real GPU - and the dashboard said
 * nothing at all either time. `act()` toasted on failure and returned in
 * silence on success, so the only feedback was a table row quietly redrawing.
 *
 * That silence is indistinguishable from a broken button, which is exactly how
 * it was reported. What is pinned here is that each action states its result,
 * that auto-adjust names the fields it rewrote (the user is entitled to know
 * their disguise was replaced with the real machine), and that "nothing needed
 * changing" is said out loud rather than looking identical to a no-op.
 */

import { afterEach, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { HealthView } from "../src/views/HealthView.jsx";
import { Toaster } from "../src/toast.jsx";
import { api } from "../src/api.js";

const profile = {
  id: "p1", name: "Ads 01", os: "Windows", engine: "cloak", status: "stopped",
  proxy: null, tags: [], locale: "en-US", timezone: "America/New_York",
  screen: "1920x1080", cores: 8, memory: 16, lastActive: "never",
};

const health = {
  summary: { total: 1, coherent: 1, withProxy: 0, scheduled: 0, colliding: 0, needsAttention: 0 },
  profiles: [{ ...profile, coherent: true, issues: [], trust: { grade: "B", score: 94 } }],
};

function show() {
  render(<><HealthView profiles={[profile]} live refresh={async () => {}} setEditor={() => {}} /><Toaster /></>);
}

afterEach(() => { cleanup(); vi.restoreAllMocks(); });

test("a trust check reports the grade it measured", async () => {
  vi.spyOn(api, "profileHealth").mockResolvedValue(health);
  vi.spyOn(api, "trust").mockResolvedValue({ grade: "C", score: 81, passed: 13, total: 16 });
  show();

  fireEvent.click(await screen.findByTitle("Run trust check"));

  await waitFor(() => expect(screen.getByText(/trust C \(81\/100\), 13 of 16 checks passed/)).toBeTruthy());
});

test("auto-adjust names what it rewrote, and the grade it moved to", async () => {
  vi.spyOn(api, "profileHealth").mockResolvedValue(health);
  vi.spyOn(api, "align").mockResolvedValue({
    before: { grade: "C", score: 81 }, after: { grade: "B", score: 94 },
    changed: ["os", "screen", "webglRenderer"],
  });
  show();

  fireEvent.click(await screen.findByTitle("Auto-adjust to grade A"));

  await waitFor(() => {
    const said = screen.getByText(/aligned/);
    expect(said.textContent).toMatch(/C → B/);
    expect(said.textContent, "the rewritten fields were not named")
      .toMatch(/os, screen, webglRenderer/);
  });
});

test("nothing to change is said, not left looking like a dead button", async () => {
  vi.spyOn(api, "profileHealth").mockResolvedValue(health);
  vi.spyOn(api, "align").mockResolvedValue({
    before: { grade: "A", score: 100 }, after: { grade: "A", score: 100 }, changed: [],
  });
  show();

  fireEvent.click(await screen.findByTitle("Auto-adjust to grade A"));

  await waitFor(() => expect(screen.getByText(/nothing needed changing/)).toBeTruthy());
});

test("a failure is still reported, and not as a success", async () => {
  vi.spyOn(api, "profileHealth").mockResolvedValue(health);
  vi.spyOn(api, "trust").mockRejectedValue(new Error("the engine is not installed"));
  show();

  fireEvent.click(await screen.findByTitle("Run trust check"));

  await waitFor(() => expect(screen.getByText(/Trust check failed: the engine is not installed/)).toBeTruthy());
  expect(screen.queryByText(/trust [A-F] \(/), "a failure reported a grade").toBeNull();
});
