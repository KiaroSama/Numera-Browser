import type { Engines, JsonValue } from "./types";

type Decoder<T> = (value: unknown) => T;
export function record(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error("The engine returned an invalid object.");
  return value as Record<string, unknown>;
}
export const text: Decoder<string> = value => {
  if (typeof value !== "string") throw new Error("The engine returned an invalid string.");
  return value;
};
export const number: Decoder<number> = value => {
  if (typeof value !== "number" || !Number.isFinite(value)) throw new Error("The engine returned an invalid number.");
  return value;
};
export const boolean: Decoder<boolean> = value => {
  if (typeof value !== "boolean") throw new Error("The engine returned an invalid boolean.");
  return value;
};
export const nullable = <T>(decode: Decoder<T>): Decoder<T | null> => value => value === null ? null : decode(value);
export const array = <T>(decode: Decoder<T>): Decoder<T[]> => value => {
  if (!Array.isArray(value)) throw new Error("The engine returned an invalid array.");
  return value.map(decode);
};
export const json: Decoder<JsonValue> = value => {
  if (value === null || typeof value === "string" || typeof value === "boolean") return value;
  if (typeof value === "number") return number(value);
  if (Array.isArray(value)) return value.map(json);
  const object = record(value);
  return Object.fromEntries(Object.entries(object).map(([key, item]) => [key, json(item)]));
};
// Every declared property is checked, including absent required fields. Extra fields are
// retained for full-fidelity reads/exports, not silently stripped by decoding.
const optional = <T>(decode: Decoder<T>): Decoder<T | undefined> => value => value === undefined ? undefined : decode(value);
type Output<D> = D extends Decoder<infer T> ? T : never;
type Decoded<S> = { [K in keyof S as undefined extends Output<S[K]> ? never : K]: Output<S[K]> }
  & { [K in keyof S as undefined extends Output<S[K]> ? K : never]?: Exclude<Output<S[K]>, undefined> };
function shape<S extends Record<string, Decoder<unknown>>>(fields: S): Decoder<Decoded<S>> {
  return value => {
    const object = record(value);
    for (const key in fields) fields[key]!(object[key]);
    // The schema determines every property, and every one was checked above.
    // The assertion preserves extra fields rather than inventing a DTO shape.
    return object as Decoded<S>;
  };
}
const port: Decoder<string | number> = value => typeof value === "number" ? number(value) : text(value);
const strings = array(text);
const proxyFields = { id: optional(text), type: text, host: text, port, user: optional(nullable(text)), pass: optional(nullable(text)), country: optional(nullable(text)), label: optional(nullable(text)), rotateUrl: optional(nullable(text)), archived: optional(boolean), profiles: optional(number), status: optional(text), lastCheck: optional(shape({ ip: optional(text), country: optional(nullable(text)), latencyMs: optional(number), error: optional(text) })) };
const proxy = shape(proxyFields);
const profileFields = {
  id: text, name: (value: unknown) => value === null ? null : typeof value === "number" ? number(value) : text(value), folder: text, status: text, os: (value: unknown) => {
    if (value !== "Windows" && value !== "macOS" && value !== "Linux" && value !== "Android") throw new Error("The engine returned an invalid operating system.");
    return value;
  }, tags: array(value => typeof value === "number" ? number(value) : text(value)), engine: nullable(text), browser: text, browserVersion: text, userAgent: text,
  screen: text, cores: number, memory: number, webglVendor: text, webglRenderer: text,
  canvas: text, webrtc: text, audio: text, fonts: text, locale: text, timezone: text,
  geo: text, mediaVideo: number, mediaAudio: number, dnt: boolean, proxy: nullable(proxy),
  notes: text, startupUrls: text, extensions: text, lastActive: text,
  clientRects: optional(text), webglImage: optional(text), mediaDevices: optional(text), mediaSpeakers: optional(number), fontList: optional(text),
  bookmarks: optional(text), launchArgs: optional(text), dns: optional(text), ipBased: optional(strings), clearOnStart: optional(strings),
  switchedOff: optional(strings), _new: optional(boolean), _passwordSet: optional(boolean), _rev: optional(json),
};
export const profile = shape(profileFields);
export const draftProfile = shape({ ...profileFields, id: optional(text) });
export const profiles = array(profile);
export const proxies = array(shape({ ...proxyFields, id: text }));
export const config = shape({ root: optional(text), version: optional(text), folders: optional(strings), default_engine: optional(text), default_extensions: optional(strings), proxyCount: optional(number) });
export const engines: Decoder<Engines> = value => Object.fromEntries(Object.entries(record(value)).map(([key, installed]) => [key, boolean(installed)]));
const grade = shape({ grade: optional(text), score: number, passed: optional(number), total: optional(number) });
export const probe = shape({ before: optional(grade), after: optional(grade), grade: optional(text), score: optional(number), passed: optional(number), total: optional(number), checks: optional(array(shape({ name: text, ok: boolean, detail: optional(text) }))), changed: optional(strings), os: optional(text), ip: optional(text), city: optional(nullable(text)), country: optional(nullable(text)), latencyMs: optional(number), ja3Hash: optional(nullable(text)), ja4: optional(nullable(text)), countries: optional(strings), resolvers: optional(array(json)), error: optional(text), profile: optional(profile) });
export const schedule = shape({ id: text, profileId: text, everyHours: number, minutes: number, preset: text, enabled: optional(boolean) });
export const schedules = array(schedule);
const token = shape({ id: text, name: text, created: number, lastUsed: optional(nullable(number)) });
export const tokens = shape({ tokens: array(token), envToken: boolean });
export const tokenCreated = shape({ id: text, name: text, created: number, token: text });
export const attachment = shape({ port: number, headless: optional(boolean), wsUrl: optional(text) });
export const health = shape({ profiles: array(shape({ id: text, name: text, engine: text, status: text, coherent: boolean, issues: strings, collides: optional(boolean), needsAttention: optional(boolean), lastActive: optional(text), proxy: optional(nullable(shape({ set: optional(boolean), country: optional(nullable(text)), type: optional(nullable(text)) }))), trust: optional(nullable(grade)), schedule: optional(nullable(shape({ enabled: optional(boolean), everyHours: optional(number), preset: optional(text) }))) })), summary: shape({ total: optional(number), coherent: optional(number), withProxy: optional(number), scheduled: optional(number), colliding: optional(number), needsAttention: optional(number) }) });
export const extensions = shape({ extensions: array(shape({ path: text, name: optional(text), icon: optional(nullable(text)) })) });
export { shape, optional };
