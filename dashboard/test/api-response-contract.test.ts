import { afterEach, expect, test, vi } from "vitest";
import { api } from "../src/api";
import { response } from "./helpers";

// Literal wire shapes from routes_profiles.profiles_health and proxypool.rotate,
// not mocks of the decoder. No browser/provider request is made by these cases.
const healthRow = {
  id: "fixture", name: "Fixture", engine: "playwright", status: "stopped",
  coherent: true, issues: [], proxy: { set: false, country: null, type: null },
  trust: null, schedule: null,
};

afterEach(() => vi.unstubAllGlobals());

function answer(body: unknown) {
  vi.stubGlobal("fetch", vi.fn(async () => response(body)));
}

test.each([
  healthRow,
  { ...healthRow, schedule: { enabled: true, everyHours: 24, preset: "general" } },
])("health accepts its nullable proxy and summary-only schedule", async row => {
  const body = { profiles: [row], summary: { total: 1 } };
  answer(body);
  expect(await api.profileHealth()).toEqual(body);
});

test.each([
  { ok: null, rotated: false, error: "no change-IP URL is set for this proxy" },
  { ok: true, rotated: true, previousIp: null, ip: "192.0.2.1", country: null, city: null },
])("rotation preserves valid outcome-dependent fields", async body => {
  answer(body);
  expect(await api.rotateProxy("fixture")).toEqual(body);
});

test("a proxy check can return unknown location without invalidating its IP", async () => {
  const body = { ok: true, ip: "192.0.2.1", country: null, city: null, latencyMs: 1 };
  answer(body);
  expect(await api.proxyTest("fixture")).toEqual(body);
});

test.each([
  { ja3Hash: null, ja4: "fixture-ja4" },
  { ja3Hash: "fixture-ja3", ja4: null },
  { ja3Hash: null, ja4: null },
])("TLS and profile proxy checks preserve absent fingerprint observations", async fingerprints => {
  const body = { ok: true, checks: [], ...fingerprints };
  answer(body);
  expect(await api.tls("fixture")).toEqual(body);
  expect(await api.proxyTest("fixture")).toEqual(body);
});

test("TLS still refuses malformed fingerprint values", async () => {
  answer({ ok: true, checks: [], ja3Hash: 12, ja4: null });
  await expect(api.tls("fixture")).rejects.toThrow("invalid string");
});

test("pool listing retains a successful check with unknown country", async () => {
  const body = [{ id: "fixture", type: "HTTP", host: "127.0.0.1", port: "8080",
    lastCheck: { ok: true, ip: "192.0.2.1", country: null, city: null, latencyMs: 1 } }];
  answer(body);
  expect(await api.listProxies()).toEqual(body);
});

test("malformed health scalars are still refused", async () => {
  answer({ profiles: [{ ...healthRow, coherent: "true" }], summary: { total: 1 } });
  await expect(api.profileHealth()).rejects.toThrow("invalid boolean");
});
