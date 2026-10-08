import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    open: true,
    // `npm run dev` serves this app on 5173 while `numera serve` listens on
    // 8787, and api.js deliberately defaults to same-origin because in
    // production the engine serves the built bundle itself. Without this proxy
    // the documented two-terminal workflow sent every /api call to Vite, which
    // has no such route, and the dashboard came up in demo mode showing sample
    // profiles on top of a perfectly healthy engine. Dev server only: the build
    // output never sees this, so same-origin and the engine's narrow CORS
    // default both stay as they are.
    proxy: { "/api": "http://127.0.0.1:8787" },
  },
  // The regression tests render the real app against a fake engine, so they
  // need a DOM.
  test: {
    environment: "jsdom",
    setupFiles: ["./test/setup.ts"],
    // A ceiling, not an expected duration. Warm, every test in this suite
    // finishes in 0.4-1.7 s; the first one to render `<App/>` in a cold worker
    // pays the module graph and the first jsdom layout inside its own measured
    // window, and vitest's 5 s default left that no margin. Measured 2026-09-13:
    // the same two tests were 5103 ms (timeout) and 1632 ms in one scheduling,
    // and 1451 ms / 1632 ms in another where the cost landed elsewhere - so the
    // suite was passing on how the run happened to be scheduled. It failed in
    // CI on a fresh `npm ci`, which is the coldest case there is.
    //
    // 15 s is ~9x the warm median and still far inside the job's own 15 min
    // bound, so a genuine hang still fails rather than hiding here.
    testTimeout: 15000,
  },
});
