import { defineConfig } from "vite";
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
  test: { environment: "jsdom", setupFiles: ["./test/setup.js"] },
});
