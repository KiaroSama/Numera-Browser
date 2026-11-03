import { afterEach, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { response } from "./helpers";
import { CloakAcquisition } from "../src/views/CloakAcquisition";

const offer = { engine: "cloak", available: true, installed: false, offer_id: "f".repeat(64),
  sdk: "0.5.12", version: "146.0.7680.177.5", platform: "linux-x64", keyed: false, reason: null,
  notice: "Vendor download, not a redistributed Numera binary; exact SDK compatibility is not certified." };
afterEach(() => { cleanup(); vi.restoreAllMocks(); });

test("Cloak download requires reviewing local metadata and explicit exact offer consent", async () => {
  const requests: RequestInit[] = [];
  vi.spyOn(globalThis, "fetch").mockImplementation(async (_url, init = {}) => {
    requests.push(init);
    return response(init.method === "POST" ? { engine: "cloak", installed: true, version: offer.version, offer_id: offer.offer_id } : offer);
  });
  render(<CloakAcquisition live />);
  expect(requests).toHaveLength(0);
  fireEvent.click(screen.getByRole("button", { name: "Review Cloak download" }));
  await screen.findByText(/146.0.7680.177.5/);
  expect(requests.map(row => row.method)).toEqual(["GET"]);
  expect(screen.getByRole<HTMLButtonElement>("button", { name: "Approve vendor download" }).disabled).toBe(true);
  fireEvent.click(screen.getByRole("checkbox"));
  fireEvent.click(screen.getByRole("button", { name: "Approve vendor download" }));
  await waitFor(() => expect(screen.getByText(/Approved browser acquired/)).toBeTruthy());
  expect(JSON.parse(String(requests[1]?.body))).toEqual({ offer_id: offer.offer_id, approve: true });
});

test("a failed acquisition consumes consent and requires reviewing local state again", async () => {
  let posts = 0;
  vi.spyOn(globalThis, "fetch").mockImplementation(async (_url, init = {}) => {
    if (init.method === "POST") { ++posts; throw new Error("Vendor operation failed"); }
    return response(offer);
  });
  render(<CloakAcquisition live />);
  fireEvent.click(screen.getByRole("button", { name: "Review Cloak download" }));
  await screen.findByRole("checkbox");
  fireEvent.click(screen.getByRole("checkbox"));
  fireEvent.click(screen.getByRole("button", { name: "Approve vendor download" }));
  await screen.findByText("Vendor operation failed");
  expect(screen.queryByRole("button", { name: "Approve vendor download" })).toBeNull();
  expect(posts).toBe(1);
  fireEvent.click(screen.getByRole("button", { name: "Review Cloak download" }));
  await screen.findByRole("checkbox");
  expect(screen.getByRole<HTMLButtonElement>("button", { name: "Approve vendor download" }).disabled).toBe(true);
});

test("an unavailable vendor generation cannot be approved", async () => {
  vi.spyOn(globalThis, "fetch").mockResolvedValue(response({ ...offer, available: false, reason: "Exact vendor generation unavailable" }));
  render(<CloakAcquisition live />);
  fireEvent.click(screen.getByRole("button", { name: "Review Cloak download" }));
  await screen.findByText("Exact vendor generation unavailable");
  expect(screen.queryByRole("button", { name: "Approve vendor download" })).toBeNull();
});
