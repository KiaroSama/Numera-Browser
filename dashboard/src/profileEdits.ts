import type { FieldPath, JsonObject, JsonValue, Profile, ProfileWrite } from "./types";

const record = (value: unknown): value is JsonObject => value !== null && typeof value === "object" && !Array.isArray(value);
const same = (a: JsonValue | undefined, b: JsonValue | undefined): boolean => {
  if (Object.is(a, b)) return true;
  if (Array.isArray(a) && Array.isArray(b)) return a.length === b.length && a.every((v, i) => same(v, b[i]));
  if (!record(a) || !record(b)) return false;
  const keys = Object.keys(a);
  return keys.length === Object.keys(b).length && keys.every(k => Object.hasOwn(b, k) && same(a[k], b[k]));
};

export const fieldValue = (profile: JsonValue | undefined, path: FieldPath): JsonValue | undefined => path.reduce<JsonValue | undefined>((value, key) => record(value) ? value[key] : undefined, profile);
function replaceField(profile: JsonObject | undefined, path: FieldPath, value: JsonValue | undefined): JsonObject {
  const [key, ...rest] = path;
  if (key === undefined) return { ...profile };
  const nested = profile?.[key];
  return { ...profile, [key]: rest.length ? replaceField(record(nested) ? nested : undefined, rest, value) : value };
}
// Paths come only from changedFields or its whole-proxy fallback. Replacing
// their original field values preserves the profile shape and unknown fields.
export const withField = (profile: Profile, path: FieldPath, value: JsonValue | undefined): Profile => replaceField(profile, path, value) as Profile;

function changedFields(base: Profile, draft: Profile): FieldPath[] {
  const changes: FieldPath[] = [];
  const visit = (before: JsonValue | undefined, after: JsonValue | undefined, path: FieldPath): void => {
    if (same(before, after)) return;
    if (record(before) && record(after)) {
      for (const key of new Set([...Object.keys(before), ...Object.keys(after)])) visit(before[key], after[key], [...path, key]);
    } else changes.push(path);
  };
  for (const key of Object.keys(draft)) {
    if (key.startsWith("_") || ["id", "status", "lastActive"].includes(key)) continue;
    visit(base[key], draft[key], [key]);
  }
  if (draft._passwordSet && draft.proxy && !changes.some(p => p[0] === "proxy" && (p.length === 1 || p[1] === "pass"))) changes.push(["proxy", "pass"]);
  return changes;
}

export function profileWrite(base: Profile, draft: Profile): ProfileWrite {
  const patch: ProfileWrite = { id: draft.id, _rev: base._rev };
  for (const [key] of changedFields(base, draft)) if (key !== undefined) patch[key] = draft[key];
  if (draft._passwordSet && patch.proxy) patch._passwordSet = true;
  return patch;
}

export function rebaseProfile(base: Profile, draft: Profile, latest: Profile): { profile: Profile; conflicts: FieldPath[] } {
  let profile: Profile = { ...latest, _new: draft._new, _passwordSet: !!draft._passwordSet };
  const conflicts: FieldPath[] = [];
  for (const path of changedFields(base, draft)) {
    const target = path[0] === "proxy" && path.length > 1 && !latest.proxy ? ["proxy"] : path;
    if (!same(fieldValue(base, target), fieldValue(latest, target)) && !same(fieldValue(draft, target), fieldValue(latest, target))) {
      if (!conflicts.some(p => p.join(".") === target.join("."))) conflicts.push(target);
    }
    profile = withField(profile, target, fieldValue(draft, target));
  }
  return { profile, conflicts };
}
