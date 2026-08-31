/* The two bulk operations, where a small mistake is multiplied by the batch.
 *
 * BulkModal writes up to 10,000 profiles in one press and nothing reviews them
 * afterwards, so a wrong spread formula or a malformed field is not one bad
 * profile - it is the whole batch, discovered later by whichever account gets
 * flagged. SyncModal has the mirror-image risk: it applies one patch to every
 * selected profile, so a field that stops being omitted when blank is not a
 * no-op, it is bulk data loss across the selection.
 *
 * Both are driven through their real controls rather than by calling build()
 * directly - build() is a closure over component state, and the state is half
 * of what is being tested. */

import { afterEach, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { BulkModal, SyncModal } from "../src/views/bulk.jsx";
import { coherence } from "../src/deviceData.js";

afterEach(cleanup);

const primary = () => document.querySelector(".ps-drawer-foot .ps-btn.primary");

/* Open the generator, adjust it, press Create, and return the batch it built.
 * Cleans up on the way out so one test can generate more than one batch. */
async function generate(adjust = () => {}) {
  const onCreate = vi.fn(async () => {});
  render(<BulkModal onClose={() => {}} onCreate={onCreate} folder="Facebook Ads" />);
  adjust();
  fireEvent.click(primary());
  await waitFor(() => expect(onCreate).toHaveBeenCalled());
  const list = onCreate.mock.calls[0][0];
  cleanup();
  return list;
}

const setNumber = (label, value) =>
  fireEvent.change(screen.getByLabelText(label), { target: { value: String(value) } });

test("the batch pairs every OS with every country rather than locking them together", async () => {
  const list = await generate(() => {
    setNumber(/How many/, 4);
    fireEvent.click(screen.getByRole("button", { name: "macOS" }));   // OS chips: Windows + macOS
    fireEvent.click(screen.getByRole("button", { name: "GB" }));      // country chips: US + GB
  });

  /* The OS cycles every profile and the country only advances once the OS list
   * has been walked, which is what makes a batch a spread. Flatten the country
   * to `countries[i % countries.length]` - the obvious-looking simplification -
   * and the two move in lockstep instead: every Windows profile is American and
   * every Mac is British, which is a pattern rather than a population. */
  expect(list.map(p => [p.os, p.locale])).toEqual([
    ["Windows", "en-US"],
    ["macOS", "en-US"],
    ["Windows", "en-GB"],
    ["macOS", "en-GB"],
  ]);
  // The locale is the country's, and the timezone is the locale's.
  expect(list.map(p => p.timezone)).toEqual([
    "America/New_York", "America/New_York", "Europe/London", "Europe/London",
  ]);
});

test("names are padded to one width across the batch, counted from the last number", async () => {
  // 98..102 crosses from two digits to three. The width has to come from the
  // last name, not the first: pad from `start` and the batch reads Batch-98,
  // Batch-99, Batch-100 - which sorts 100 before 98 everywhere the list is
  // ordered as text, so the batch no longer reads as a run.
  const crossing = await generate(() => { setNumber(/How many/, 5); setNumber("Start number", 98); });
  expect(crossing.map(p => p.name)).toEqual([
    "Batch-098", "Batch-099", "Batch-100", "Batch-101", "Batch-102",
  ]);

  // Two digits is the floor, so a small batch is Batch-01 and not Batch-1.
  const small = await generate(() => { setNumber(/How many/, 3); });
  expect(small.map(p => p.name)).toEqual(["Batch-01", "Batch-02", "Batch-03"]);
});

test("an unset launch engine is sent as null, not an empty string", async () => {
  // The server rejects an engine it does not recognise but reads null as "you
  // decide". Send "" instead and every profile in the batch is refused, so the
  // press that was meant to create 10,000 creates none.
  const unset = await generate(() => setNumber(/How many/, 2));
  for (const p of unset) {
    expect(p.engine).toBeNull();
    expect(p.engine).not.toBe("");   // toBeFalsy() would pass on the broken value
  }

  const chosen = await generate(() => {
    setNumber(/How many/, 2);
    fireEvent.change(screen.getByLabelText(/Launch engine/), { target: { value: "camoufox" } });
  });
  expect(chosen.map(p => p.engine)).toEqual(["camoufox", "camoufox"]);
});

test("every generated profile is internally coherent", async () => {
  // Hardware is randomised per profile, so this is the assertion that covers
  // the combinations no fixed example reaches. `coherence` is the same check
  // the editor's meter runs, so anything it flags here is a profile the app
  // itself would mark as contradictory the moment someone opened it.
  const list = await generate(() => {
    setNumber(/How many/, 24);
    for (const os of ["macOS", "Linux", "Android"]) fireEvent.click(screen.getByRole("button", { name: os }));
    for (const c of ["DE", "TR", "IN"]) fireEvent.click(screen.getByRole("button", { name: c }));
  });
  expect(list).toHaveLength(24);

  // "Google Inc. (AMD)" describes a Radeon, and so on: the vendor names a GPU
  // family, and it has to be the family the renderer string actually belongs
  // to. Checked against the renderer rather than by re-running vendorFor, so
  // the test does not agree with a broken derivation just because it shares it.
  const ALIAS = { AMD: "radeon", Qualcomm: "adreno", ARM: "mali" };

  for (const p of list) {
    expect(coherence(p), `${p.name} ${p.os} ${p.locale}`).toEqual([]);
    // coherence() never reads the user-agent, so the OS token and the Chrome
    // version in it are only ever checked here.
    expect(p.userAgent).toContain(`Chrome/${p.browserVersion}`);
    expect(p.userAgent).toContain({
      Windows: "Windows NT", macOS: "Mac OS X", Linux: "X11; Linux", Android: "Android 13",
    }[p.os]);

    const named = p.webglVendor.match(/\((.+)\)/);
    expect(named, `no GPU family in vendor "${p.webglVendor}"`).not.toBeNull();
    const family = ALIAS[named[1]] || named[1];
    expect(p.webglRenderer.toLowerCase()).toContain(family.toLowerCase());
  }
});

/* ---- SyncModal: one patch, applied to a whole selection ------------ */

function openSync() {
  const onApply = vi.fn();
  render(<SyncModal ids={["a", "b", "c"]} folders={["All profiles", "Facebook Ads"]}
                    onClose={() => {}} onApply={onApply} />);
  return onApply;
}

test("a bulk edit sends only the fields that were filled in", () => {
  const onApply = openSync();
  fireEvent.change(screen.getByLabelText(/Locale/), { target: { value: "en-GB" } });
  fireEvent.click(primary());

  const patch = onApply.mock.calls[0][0];
  expect(patch).toEqual({ locale: "en-GB" });
  /* Membership, not value: a patch carrying `engine: ""` or `folder: ""` is
   * the failure this guards against, and both are falsy, so checking that the
   * value is empty would pass on exactly the broken payload. Applied to a
   * selection, each blank field would clear that setting on every profile in
   * it - a bulk edit turned into bulk erasure. */
  for (const key of ["engine", "folder", "addTags"]) expect(key in patch).toBe(false);
});

test("whitespace is not a tag, and an untouched form cannot be applied at all", () => {
  const onApply = openSync();
  // Nothing filled in: there is no change to make, so the button does not offer
  // to write one to all three profiles.
  expect(primary().disabled).toBe(true);
  fireEvent.click(primary());
  expect(onApply).not.toHaveBeenCalled();

  fireEvent.change(screen.getByLabelText(/Add tags/), { target: { value: "   " } });
  expect(primary().disabled).toBe(true);

  fireEvent.change(screen.getByLabelText(/Add tags/), { target: { value: " q3-campaign , , ads " } });
  fireEvent.click(primary());
  // Trimmed, and the empty entry between the commas is dropped rather than
  // tagging every selected profile with "".
  expect(onApply.mock.calls[0][0]).toEqual({ addTags: ["q3-campaign", "ads"] });
});
