import type { Profile } from "../src/types";
/* The timezone control: two named buttons, and every zone there is.
 *
 * What it replaced: a single toggle whose label flipped between "IP" and
 * "Manual" - one control with two meanings - beside a bare `<input>` that
 * accepted any string and offered no list at all. The dashboard's only
 * timezone source was `LOCALES`, ten pairs, so `Asia/Tehran` was typed from
 * memory and a typo reached the engine unchallenged.
 *
 * These tests pin the three things that were asked for and the one that must
 * not quietly regress: Auto and Manual are separate, the list is complete and
 * comes from the platform rather than a written table, the input is a
 * searchable datalist and not a `<select>` (a select cannot show a stored zone
 * the runtime has since renamed), and Auto only locks the field when a proxy
 * exists to resolve from.
 */

import { afterEach, expect, test } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { TimezoneField, LIST_ID } from "../src/views/TimezoneField.tsx";
import { LOCALES, TIMEZONES, validTimezone } from "../src/deviceData.ts";

const proxy = { type: "HTTP", host: "h", port: "1", server: "http://h:1", country: "US" };

function field(profile: Partial<Profile> = {}, onSet: (patch: Partial<Profile>) => void = () => {}) {
  const p = { timezone: "America/New_York", ipBased: [], proxy: null, ...profile };
  render(<TimezoneField p={p} set={onSet} />);
  return p;
}

const input = () => document.querySelector<HTMLInputElement>(`input[list="${LIST_ID}"]`)!;
/* Queried by accessible NAME on purpose. `Field` wraps its children in a
 * <label>, and a <button> is a labelable element, so without an explicit
 * aria-label both of these announce as "Timezone" - two controls a screen
 * reader cannot tell apart. Asking by name is what keeps that fixed. */
const button = (name: string) => screen.getByRole<HTMLButtonElement>("button", { name });
const zones = () => [...document.querySelectorAll<HTMLOptionElement>(`#${LIST_ID} option`)].map(o => o.value);

afterEach(() => cleanup());

test("the list is the platform's, not the ten pairs the dashboard had", () => {
  expect(TIMEZONES.length,
    "the zone list collapsed back to the locale table").toBeGreaterThan(50);
  // Every default this project already creates profiles with must be offerable,
  // including `Asia/Kolkata`, which the platform list spells `Asia/Calcutta`.
  for (const tz of Object.values(LOCALES)) {
    expect(TIMEZONES, `the picker cannot offer ${tz}`).toContain(tz);
    expect(validTimezone(tz), `${tz} would be flagged as unknown`).toBe(true);
  }
  expect(TIMEZONES).toContain("Asia/Tehran");
  expect(TIMEZONES).toContain("America/Los_Angeles");
});

test("every zone is offered, and through a datalist so it stays searchable", () => {
  field();
  expect(input(), "the control is no longer an input with a list").toBeTruthy();
  expect(document.querySelector<HTMLSelectElement>("select[name='timezone']")!,
    "a select cannot show a zone the runtime has renamed").toBeNull();
  expect(zones()).toEqual(TIMEZONES);
});

test("Auto and Manual are two buttons, not one toggle with two labels", () => {
  field({ proxy });
  const auto = button("Auto");
  const manual = button("Manual");
  expect(auto).not.toBe(manual);
  expect(manual.className, "Manual is the mode when nothing follows the IP").toContain("on");
  expect(auto.className).not.toContain("on");
});

test("Auto asks to follow the exit IP; Manual stops following it", () => {
  const calls: Partial<Profile>[] = [];
  field({ proxy }, patch => calls.push(patch));
  fireEvent.click(button("Auto"));
  expect(calls.at(-1)).toEqual({ ipBased: ["timezone"] });

  cleanup();
  const back: Partial<Profile>[] = [];
  field({ proxy, ipBased: ["timezone", "languages"] }, patch => back.push(patch));
  fireEvent.click(button("Manual"));
  expect(back.at(-1), "Manual must not clear the other IP-based signals")
    .toEqual({ ipBased: ["languages"] });
});

test("Auto needs a proxy, and never locks a field nothing can fill", () => {
  field({ ipBased: ["timezone"] });          // auto wanted, but no proxy
  expect(button("Auto").disabled,
    "Auto offered with no exit IP to read").toBe(true);
  expect(input().disabled,
    "the field is locked with nothing able to fill it").toBe(false);

  cleanup();
  field({ ipBased: ["timezone"], proxy });   // auto, and an IP to read
  expect(input().disabled).toBe(true);
});

test("a zone the runtime does not know is flagged, not silently accepted", () => {
  field({ timezone: "Mars/Olympus" });
  expect(input().getAttribute("aria-invalid")).toBe("true");
  expect(screen.getByText(/Not a zone this browser knows/)).toBeTruthy();

  cleanup();
  field({ timezone: "Asia/Tehran" });
  expect(input().getAttribute("aria-invalid")).toBeNull();
});
