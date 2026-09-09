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

/* The lifecycle, state transition by state transition. Each of these is a
 * thing the editor can do between loading a profile and saving it, and each
 * has to reach the server as the intent the user expressed - never as one
 * inferred from what the value happens to be. */

test("turning the proxy off sends the removal, not a password statement", () => {
  // `proxy: null` is REMOVE server-side: the endpoint, the username and the
  // password go together. It must survive whether or not the password control
  // was ever touched, so neither branch of the omission may swallow it.
  for (const marker of [{}, { _passwordSet: true }]) {
    const sent = forWrite({ ...loaded(), ...marker, proxy: null });

    expect(sent.proxy).toBe(null);
    expect("_passwordSet" in sent).toBe(false);
  }
});

test("turning it off and on again sends what the form now holds", () => {
  // The toggle builds a fresh proxy with an empty password and marks it, so
  // the new endpoint gets no credential rather than silently inheriting the
  // old one. Only the final state is sent; there is no history to replay.
  const on = { ...loaded(), proxy: { type: "HTTP", host: "", port: "", user: "", pass: "" }, _passwordSet: true };

  expect(forWrite(on).proxy.pass).toBe("");
});

test("picking a saved pooled proxy sends its password explicitly", () => {
  const picked = {
    ...loaded(), _passwordSet: true,
    proxy: { type: "SOCKS5", host: "pool.example", port: "1080", user: "pu", pass: "pool-secret" },
  };

  expect(forWrite(picked).proxy.pass).toBe("pool-secret");
});

test("a profile refetched after a save carries no dirty marker", () => {
  // The marker is stripped on the way out and never stored, so what the server
  // hands back on the next load has none - and the next unrelated edit is a
  // preserve again. A marker that survived one save would make every later
  // edit a replacement.
  const saved = forWrite({ ...loaded(), _passwordSet: true });
  const refetched = { ...saved, proxy: { ...saved.proxy, pass: "dashboard-secret" } };

  expect("_passwordSet" in refetched).toBe(false);
  expect("pass" in forWrite({ ...refetched, notes: "later" }).proxy).toBe(false);
});
