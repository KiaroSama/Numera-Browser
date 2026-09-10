// Three-way profile edits. Arrays are one field; proxy members can rebase
// independently. Password intent is explicit even when its value is unchanged.
const record = value => value !== null && typeof value === "object" && !Array.isArray(value);
const same = (a, b) => {
  if (Object.is(a, b)) return true;
  if (Array.isArray(a) && Array.isArray(b)) return a.length === b.length && a.every((v, i) => same(v, b[i]));
  if (!record(a) || !record(b)) return false;
  const keys = Object.keys(a);
  return keys.length === Object.keys(b).length && keys.every(k => Object.hasOwn(b, k) && same(a[k], b[k]));
};

export const fieldValue = (profile, path) => path.reduce((value, key) => value?.[key], profile);
export const withField = (profile, [key, ...rest], value) => ({
  ...profile, [key]: rest.length ? withField(profile?.[key], rest, value) : value,
});

function changedFields(base, draft) {
  const changes = [];
  const visit = (before, after, path) => {
    if (same(before, after)) return;
    if (record(before) && record(after)) {
      for (const key of new Set([...Object.keys(before), ...Object.keys(after)])) {
        visit(before[key], after[key], [...path, key]);
      }
    } else changes.push(path);
  };
  for (const key of Object.keys(draft)) {
    if (key.startsWith("_") || ["id", "status", "lastActive"].includes(key)) continue;
    visit(base[key], draft[key], [key]);
  }
  if (draft._passwordSet && draft.proxy && !changes.some(p => p[0] === "proxy" && (p.length === 1 || p[1] === "pass"))) {
    changes.push(["proxy", "pass"]);
  }
  return changes;
}

export function profileWrite(base, draft) {
  const patch = { id: draft.id, _rev: base._rev };
  for (const [key] of changedFields(base, draft)) patch[key] = draft[key];
  if (draft._passwordSet && patch.proxy) patch._passwordSet = true;
  return patch;
}

export function rebaseProfile(base, draft, latest) {
  let profile = { ...latest, _new: draft._new, _passwordSet: !!draft._passwordSet };
  const conflicts = [];
  for (const path of changedFields(base, draft)) {
    // Rebuilding just one member of a remotely removed proxy would create an
    // incomplete object, so that removal conflicts with the whole local proxy.
    const target = path[0] === "proxy" && path.length > 1 && !latest.proxy ? ["proxy"] : path;
    if (!same(fieldValue(base, target), fieldValue(latest, target)) && !same(fieldValue(draft, target), fieldValue(latest, target))) {
      if (!conflicts.some(p => p.join(".") === target.join("."))) conflicts.push(target);
    }
    profile = withField(profile, target, fieldValue(draft, target));
  }
  return { profile, conflicts };
}
