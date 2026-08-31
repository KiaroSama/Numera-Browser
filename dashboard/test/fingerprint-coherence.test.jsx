/* The editor's derived fields, which have to move together.
 *
 * This is an anti-detect tool, so a fingerprint fails on contradictions rather
 * than on unusual values. Picking macOS has to rewrite the user-agent, the
 * screen, the core count, the memory, the GPU and the GPU's vendor in one
 * step - leave any one of them behind and the profile ships a macOS platform
 * under a Windows user-agent, or an Apple renderer reporting an NVIDIA vendor,
 * which is precisely the tell the product exists to avoid.
 *
 * The user-agent is asserted by hand rather than through `coherence()` on
 * purpose: coherence never inspects the user-agent, so a profile with a stale
 * one still scores 100% and the meter says every signal agrees. */

import { afterEach, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { Editor } from "../src/views/Editor.jsx";
import { coherence } from "../src/deviceData.js";
import { newProfile } from "../src/ui.jsx";

afterEach(cleanup);

/* Built from the app's own factory, not a hand-written literal, so the fixture
 * cannot drift away from the shape a real new profile has. It comes back as
 * Windows / en-US / America/New_York with `_new: true`. */
function open(overrides = {}) {
  const onSave = vi.fn();
  render(<Editor profile={{ ...newProfile("Facebook Ads", "playwright"), ...overrides }}
                 live={false} folders={[]} onClose={() => {}} onSave={onSave} />);
  return onSave;
}

const fingerprintTab = () => fireEvent.click(screen.getByRole("button", { name: "fingerprint" }));
const saveButton = () => document.querySelector(".ps-drawer-foot .ps-btn.primary");

/* What the profile would actually be written as. Asserting the payload rather
 * than the form controls is the point: the payload is what reaches the engine,
 * and a field that renders correctly but is dropped on the way out is still a
 * broken profile. */
function save(onSave) {
  fireEvent.click(saveButton());
  expect(onSave).toHaveBeenCalledTimes(1);
  return onSave.mock.calls[0][0];
}

test("choosing macOS rewrites the user-agent, hardware and GPU as one device", () => {
  const onSave = open({ name: "Ad account 12" });
  fingerprintTab();
  fireEvent.click(screen.getByRole("button", { name: "macOS" }));
  const p = save(onSave);

  expect(p.os).toBe("macOS");
  // The headline contradiction: a macOS profile still announcing Windows NT.
  expect(p.userAgent).toContain("Macintosh; Intel Mac OS X 10_15_7");
  expect(p.userAgent).not.toContain("Windows NT");
  // 1920x1080 is a Windows resolution and 6 threads a Windows core count; a
  // Mac reporting either is a machine that does not exist.
  expect(p.screen).toBe("2560x1600");
  expect(p.cores).toBe(10);
  expect(p.webglRenderer).toBe("ANGLE (Apple M1, OpenGL 4.1)");
  // The seeded vendor is "Google Inc. (NVIDIA)". An Apple M1 that reports an
  // NVIDIA vendor is a contradiction the coherence meter does not score, so
  // nothing else in the app would catch this one.
  expect(p.webglVendor).toBe("Google Inc. (Apple)");
  expect(coherence(p)).toEqual([]);
});

test("choosing Android moves the memory and the mobile token too", () => {
  const onSave = open({ name: "Ad account 12" });
  fingerprintTab();
  fireEvent.click(screen.getByRole("button", { name: "Android" }));
  const p = save(onSave);

  // A phone with a desktop RAM figure. The Windows fixture starts at 16 GB and
  // every desktop archetype offers 8/16/32, so a derivation that forgot memory
  // survives the macOS case above and only shows up here.
  expect(p.memory).toBe(6);
  expect(p.cores).toBe(8);
  expect(p.screen).toBe("360x800");
  expect(p.userAgent).toContain("Android 13; Pixel 7");
  expect(p.userAgent).toContain(" Mobile Safari");
  expect(p.webglVendor).toBe("Google Inc. (Qualcomm)");
  expect(coherence(p)).toEqual([]);
});

test("choosing a locale moves the timezone with it", () => {
  const onSave = open({ name: "Ad account 12" });
  fingerprintTab();

  // The control currently showing en-US is the locale picker; the timezone
  // input beside it is showing the timezone that locale implies.
  expect(screen.getByDisplayValue("America/New_York")).toBeTruthy();
  fireEvent.change(screen.getByDisplayValue("en-US"), { target: { value: "de-DE" } });

  // Visible first: a German profile still holding a New York clock is the
  // cheapest tell there is, and the user has to be able to see it moved.
  expect(screen.getByDisplayValue("Europe/Berlin")).toBeTruthy();
  expect(screen.queryByDisplayValue("America/New_York")).toBeNull();

  const p = save(onSave);
  expect(p.locale).toBe("de-DE");
  expect(p.timezone).toBe("Europe/Berlin");
  expect(coherence(p)).toEqual([]);
});

test("the coherence meter reports the contradiction a stored profile arrives with", () => {
  // Not every profile is built here - one can be imported, or written by an
  // older version. The meter has to score what it is given, not assume the
  // editor produced it.
  open({ name: "Imported", locale: "de-DE", timezone: "America/New_York" });
  expect(document.querySelector(".ps-coh-iss").textContent).toContain("Timezone doesn't match locale");
  expect(document.querySelector(".ps-coh-ok")).toBeNull();
  expect(document.querySelector(".ps-coh-score").textContent).toBe("78%");
});

test("a profile cannot be saved without a name", () => {
  // `newProfile` starts with an empty name and App.save keys off the id, so an
  // unnamed profile would land in the list as an untitled row nobody can find.
  const onSave = open();
  expect(saveButton().disabled).toBe(true);
  fireEvent.click(saveButton());
  expect(onSave).not.toHaveBeenCalled();

  fireEvent.change(screen.getByLabelText("Profile name"), { target: { value: "Ad account 12" } });
  expect(saveButton().disabled).toBe(false);
  expect(save(onSave).name).toBe("Ad account 12");
});

test("saving clears the new-profile flag, so a second save updates instead of duplicating", () => {
  // App.save reads `!p._new` to choose between api.update and api.create. Leave
  // the flag set and reopening a just-created profile and saving again POSTs a
  // second copy of it every time instead of updating the first.
  const onSave = open({ name: "Ad account 12" });
  expect(save(onSave)._new).toBe(false);
});
