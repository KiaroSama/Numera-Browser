import { afterEach, expect, test, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { api, auth } from "../src/api";
import { EditProxyModal } from "../src/views/ProxiesView";

afterEach(() => { auth.clear(); vi.unstubAllGlobals(); vi.restoreAllMocks(); });

test.each([
  ["clear", "fixture-password", "", ""],
  ["replace", "fixture-password", "replacement", "replacement"],
  ["preserve withheld", null, undefined, null],
])("the proxy pool editor transmits %s password intent", async (_intent, stored, edited, expected) => {
  auth.clear();
  const fetch = vi.fn(async () => ({ ok: true, status: 200, json: async () => ({ id: "pool-1" }) }));
  vi.stubGlobal("fetch", fetch);
  const closed = vi.fn();
  render(<EditProxyModal
    proxy={{ id: "pool-1", type: "HTTP", host: "127.0.0.1", port: "8080", pass: stored }}
    onClose={closed} onSave={patch => api.updateProxy("pool-1", patch)} />);
  if (edited !== undefined) fireEvent.change(screen.getByLabelText("Password"), { target: { value: edited } });
  fireEvent.click(screen.getByRole("button", { name: "Save", exact: true }));
  await waitFor(() => expect(closed).toHaveBeenCalledOnce());
  expect(fetch).toHaveBeenCalledOnce();
  const [url, request] = fetch.mock.calls[0];
  expect(url).toBe("/api/proxies/pool-1");
  expect(request.method).toBe("PUT");
  expect(JSON.parse(request.body).pass).toBe(expected);
});
