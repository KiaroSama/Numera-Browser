import { afterEach, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { response } from "./helpers";
import { updateRequest, readUpdateBridge } from "../src/updates";
import type { UpdateStatus } from "../src/updates";
import { useUpdates } from "../src/useUpdates";
import { UpdateNotification, UpdatePanel } from "../src/views/UpdatePanel";

const bridge = { endpoint: "http://127.0.0.1:19876", token: "a".repeat(64) };
const offered: UpdateStatus = {
  state: "available", message: "A verified update is available.",
  current: { version: "0.36.0", sourceSha: "1".repeat(40), generation: 10, platform: "windows-x64", portable: true },
  offer: { id: "offer-11", version: "0.36.1", sourceSha: "2".repeat(40), generation: 11,
    downloadBytes: 1048576, expandedBytes: 4194304, components: { sdk: { playwright: "1.62.0" } }, restartRequired: true },
  progress: { receivedBytes: 0, totalBytes: 1048576 }, canCancel: true,
};

function Updates() {
  const updates = useUpdates();
  return <><UpdateNotification updates={updates} onReview={() => undefined} /><UpdatePanel updates={updates} /></>;
}

afterEach(() => {
  cleanup();
  delete window.__NUMERA_UPDATES__;
  vi.restoreAllMocks();
});

test("source mode never probes or offers a pretend updater", () => {
  const fetch = vi.spyOn(globalThis, "fetch");
  render(<Updates />);
  expect(screen.getByText(/Source and server installations/)).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Check for updates" })).toBeNull();
  expect(fetch).not.toHaveBeenCalled();
});

test.each(["https://example.com", "http://localhost:19876", "http://127.0.0.1:19876/other", "http://127.0.0.1:0"])(
  "an invalid bridge cannot receive a capability: %s", endpoint => {
    window.__NUMERA_UPDATES__ = { ...bridge, endpoint };
    expect(readUpdateBridge()).toBeNull();
  },
);

test("an offer and Later never authorize a download; each exact target needs confirmation", async () => {
  window.__NUMERA_UPDATES__ = bridge;
  const requests: { path: string; body: unknown }[] = [];
  vi.spyOn(globalThis, "fetch").mockImplementation(async (url, init) => {
    const path = new URL(String(url)).pathname;
    requests.push({ path, body: init?.body ? JSON.parse(String(init.body)) : null });
    expect(new Headers(init?.headers).get("Authorization")).toBe(`Bearer ${bridge.token}`);
    expect(init?.redirect).toBe("error");
    return response(path === "/download" ? { ...offered, state: "ready" } : offered);
  });
  render(<Updates />);
  await screen.findByRole("button", { name: "Review update" });
  fireEvent.click(screen.getByRole("button", { name: "Download update" }));
  const first = screen.getByRole("dialog", { name: "Confirm update" });
  expect(within(first).getByText(/1.0 MiB/)).toBeTruthy();
  expect(requests.map(r => r.path)).toEqual(["/status"]);
  fireEvent.click(within(first).getByRole("button", { name: "Later" }));
  expect(requests.map(r => r.path)).toEqual(["/status"]);
  fireEvent.click(screen.getByRole("button", { name: "Download update" }));
  fireEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Approve download" }));
  await screen.findByRole("button", { name: "Install and restart" });
  expect(requests.find(r => r.path === "/download")?.body).toEqual({ offerId: "offer-11", approve: true });
  expect(requests.some(r => r.path === "/install")).toBe(false);
  fireEvent.click(screen.getByRole("button", { name: "Install and restart" }));
  fireEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Approve restart" }));
  await waitFor(() => expect(requests.find(r => r.path === "/install")?.body).toEqual({ offerId: "offer-11", approve: true }));
});

test("malformed bridge status is rejected rather than enabling an install", async () => {
  vi.spyOn(globalThis, "fetch").mockResolvedValue(response({ ...offered, offer: { ...offered.offer, downloadBytes: -1 } }));
  await expect(updateRequest(bridge, "status")).rejects.toThrow("Invalid update status");
});

test.each([...Array.from({ length: 32 }, (_, code) => code), 127])(
  "status text rejects control code %i before enabling an action", async code => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(response({ ...offered, message: `Update${String.fromCharCode(code)}ready` }));
    await expect(updateRequest(bridge, "status")).rejects.toThrow("Invalid update status");
  },
);

test("status text preserves Unicode and printable ASCII boundaries", async () => {
  const message = "Update ready ~ — فارسی 🚀";
  vi.spyOn(globalThis, "fetch").mockResolvedValue(response({ ...offered, message }));
  expect((await updateRequest(bridge, "status")).message).toBe(message);
});

test("HTTP refusal carries a safe deferred state and never starts an implicit retry", async () => {
  window.__NUMERA_UPDATES__ = bridge;
  const fetch = vi.spyOn(globalThis, "fetch").mockResolvedValue(response({ ...offered, state: "deferred", message: "Close active sessions before installing." }, 409));
  render(<Updates />);
  expect(await screen.findByText("Close active sessions before installing.")).toBeTruthy();
  expect(fetch).toHaveBeenCalledTimes(1);
});

test("unmount aborts the one outstanding status request", async () => {
  window.__NUMERA_UPDATES__ = bridge;
  let signal: AbortSignal | null = null;
  vi.spyOn(globalThis, "fetch").mockImplementation((_url, init) => new Promise((_resolve, reject) => {
    signal = init?.signal ?? null;
    signal?.addEventListener("abort", () => reject(new DOMException("Aborted", "AbortError")), { once: true });
  }));
  const view = render(<Updates />);
  view.unmount();
  expect(signal).not.toBeNull();
  expect((signal as AbortSignal | null)?.aborted).toBe(true);
});
