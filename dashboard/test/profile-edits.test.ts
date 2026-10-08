import { newProfile } from "../src/ui";
import { expect, test } from "vitest";
import { forWrite } from "../src/api.ts";
import { profileWrite, rebaseProfile } from "../src/profileEdits.ts";

const base = { ...newProfile("", "playwright"), id: "p", _rev: 5, tags: ["old"], notes: "", proxy: {
  type: "HTTP", host: "old.host", port: "80", pass: "original",
} };

test("nested proxy edits retain a remote password without replacing its intent", () => {
  const draft = { ...base, proxy: { ...base.proxy, host: "new.host" } };
  const latest = { ...base, _rev: 6, proxy: { ...base.proxy, pass: "remote" } };
  const rebased = rebaseProfile(base, draft, latest);
  expect(rebased.conflicts).toEqual([]);
  expect(rebased.profile.proxy).toMatchObject({ host: "new.host", pass: "remote" });
  expect(forWrite(profileWrite(latest, rebased.profile))).toEqual({
    id: "p", _rev: 6, proxy: { type: "HTTP", host: "new.host", port: "80" },
  });
});

test("retyping the old password remains an explicit conflict after remote replacement", () => {
  const latest = { ...base, _rev: 6, proxy: { ...base.proxy, pass: "remote" } };
  const rebased = rebaseProfile(base, { ...base, _passwordSet: true }, latest);
  expect(rebased.conflicts).toEqual([["proxy", "pass"]]);
  expect(forWrite(profileWrite(latest, rebased.profile)).proxy!.pass).toBe("original");
});

test("remote proxy removal conflicts with a local member edit without building an invalid proxy", () => {
  const draft = { ...base, proxy: { ...base.proxy, host: "new.host" } };
  const rebased = rebaseProfile(base, draft, { ...base, _rev: 6, proxy: null });
  expect(rebased.conflicts).toEqual([["proxy"]]);
  expect(rebased.profile.proxy).toEqual(draft.proxy);
});

test("equal concurrent edits need no conflict and later revisions rebase from the new base", () => {
  const latest = { ...base, _rev: 6, notes: "same" };
  const first = rebaseProfile(base, { ...base, notes: "same" }, latest);
  expect(first.conflicts).toEqual([]);
  const next = { ...latest, _rev: 7, tags: ["new"] };
  const second = rebaseProfile(latest, { ...first.profile, notes: "next edit" }, next);
  expect(second.profile).toMatchObject({ notes: "next edit", tags: ["new"], _rev: 7 });
  expect(forWrite(profileWrite(next, second.profile))).toEqual({ id: "p", _rev: 7, notes: "next edit" });
  expect(base.proxy.host).toBe("old.host");
});
