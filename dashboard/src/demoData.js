/* Sample profiles for demo mode - what the UI shows when no engine answers.
 *
 * Deliberately separate from deviceData.js: these are fabricated rows, and
 * mixing them with the real device tables is how fake data ends up on screen
 * in live mode. If you see 'Acme' anywhere with an engine running, this file
 * leaked. */

import { CORES, COUNTRY_LOCALE, GPUS, LOCALES, RAM, SCREENS, nid, uaFor } from "./deviceData";

const FOLDERS = ["All profiles", "Facebook Ads", "Amazon Stores", "Crypto", "QA Testing"];

// Collision-resistant local id: a counter (unique within a page load) plus a
// random suffix (so ids don't repeat across reloads / other sessions). In live
// mode the server assigns its own id anyway; this is for demo/local state.

function mk(name, folder, status, os, tags, country) {
  const locale = country && COUNTRY_LOCALE[country] ? COUNTRY_LOCALE[country] : "en-US";
  return {
    id: nid(), name, folder, status, os, tags, engine: "playwright",
    browser: "Chrome", browserVersion: "128.0.0.0", userAgent: uaFor(os),
    screen: SCREENS[os][0], cores: CORES[os][1], memory: RAM[os][1],
    webglVendor: os === "macOS" ? "Google Inc. (Apple)" : "Google Inc. (NVIDIA)",
    webglRenderer: GPUS[os][0],
    canvas: "Noise", webrtc: "Altered", audio: "Noise", fonts: "Masked",
    locale, timezone: LOCALES[locale], geo: "Prompt",
    mediaVideo: 1, mediaAudio: 1, dnt: false,
    proxy: country ? { type: "HTTP", host: "res-" + country.toLowerCase() + ".proxy.io", port: "8080", user: "u" + name.length, pass: "•••••", country } : null,
    notes: "", startupUrls: "", extensions: "",
    lastActive: status === "running" ? "now" : ["2h ago", "yesterday", "3d ago"][name.length % 3],
  };
}

const seed = [
  mk("FB-Ads-01", "Facebook Ads", "running", "Windows", ["client-a"], "US"),
  mk("FB-Ads-02", "Facebook Ads", "stopped", "Windows", ["client-a"], "US"),
  mk("AMZ-Store-EU", "Amazon Stores", "running", "macOS", ["eu"], "DE"),
  mk("Crypto-Air-07", "Crypto", "stopped", "Linux", ["airdrop"], null),
  mk("PK-Mobile-01", "QA Testing", "running", "Android", ["mobile", "urdu"], "PK"),
  mk("QA-Sandbox", "QA Testing", "stopped", "Windows", ["internal"], "GB"),
];

export { FOLDERS, mk, seed };
