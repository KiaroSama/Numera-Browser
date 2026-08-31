/* The two halves of "where does /api live?", which have to disagree.
 *
 * In production the engine serves this bundle, so the client must ask its own
 * origin - the desktop shell picks a free port at runtime and any baked-in one
 * is wrong on every run but the first. In development Vite serves it on 5173
 * while `numera serve` listens on 8787, so the same relative URL has to be
 * forwarded. Getting either wrong is silent: the app renders, cannot reach the
 * engine, and falls back to sample data as though that were the user's own. */

import { expect, test, vi } from "vitest";
import config from "../vite.config.js";
import { api } from "../src/api.js";

test("the dev server forwards /api to the engine, so npm run dev is not demo mode", () => {
  expect(config.server?.proxy?.["/api"]).toBe("http://127.0.0.1:8787");
});

test("the client still asks its own origin, so the built dashboard is unaffected", async () => {
  globalThis.fetch = vi.fn(async () => ({ ok: true, status: 200, json: async () => ({ ok: true }) }));
  await api.health();
  expect(globalThis.fetch.mock.calls[0][0]).toBe("/api/health");
});
