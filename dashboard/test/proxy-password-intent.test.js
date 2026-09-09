/* An untouched proxy password must not be sent back as a replacement.
 *
 * The dashboard loads profiles with their proxy password decrypted, and
 * `api.update` PUT the whole object - so an ordinary notes or name edit sent
 * that password again, and the server reads a password in a write as REPLACE.
 * On a profile whose two stored copies hold different passwords that silently
 * *chose* the dashboard's credential and converged both files, instead of the
 * 409 the server owes.
 *
 * The rule these pin down: the field is omitted unless the editor says the user
 * set one. Never inferred from the value - retyping the same password and never
 * touching the control produce identical values and mean opposite things. */

import { expect, test } from "vitest";
import { forWrite, withoutMarker } from "../src/api.js";

const loaded = () => ({
  id: "p1", name: "One", notes: "",
  proxy: { type: "HTTP", host: "h.example", port: "8080", user: "u", pass: "dashboard-secret" },
});

test("an untouched password is not transmitted at all", () => {
  const sent = forWrite({ ...loaded(), notes: "edited" });

  expect("pass" in sent.proxy).toBe(false);
  expect(sent.proxy).toEqual({ type: "HTTP", host: "h.example", port: "8080", user: "u" });
  expect(sent.notes).toBe("edited");
});

test("a password the user set is transmitted", () => {
  const p = { ...loaded(), _passwordSet: true };
  p.proxy = { ...p.proxy, pass: "chosen" };

  expect(forWrite(p).proxy.pass).toBe("chosen");
});

test("re-entering the password it already had is still a decision", () => {
  // The case value equality cannot see: same bytes, opposite meaning.
  const p = { ...loaded(), _passwordSet: true };

  expect(forWrite(p).proxy.pass).toBe("dashboard-secret");
});

test("an explicit clear survives as an empty string", () => {
  const p = { ...loaded(), _passwordSet: true };
  p.proxy = { ...p.proxy, pass: "" };

  expect(forWrite(p).proxy.pass).toBe("");
});

test("the marker itself never reaches the server", () => {
  expect("_passwordSet" in forWrite({ ...loaded(), _passwordSet: true })).toBe(false);
  expect("_passwordSet" in withoutMarker({ ...loaded(), _passwordSet: true })).toBe(false);
});

test("a profile with no proxy is passed through untouched", () => {
  const p = { id: "p1", name: "One", proxy: null };

  expect(forWrite(p)).toEqual(p);
});
