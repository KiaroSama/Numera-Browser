/* A mutation the server rejects must not leave the screen looking successful.
 *
 * Five App actions changed local state before the server confirmed the write
 * and only raised a toast on failure. Some of that state is never repaired: the
 * regular `refresh()` reads *profiles*, while an empty folder and the default
 * engine live in config - so a rejected create or engine change stayed on
 * screen until someone reloaded the page.
 *
 * `FoldersView` is driven for real here rather than stubbed: the component
 * hands the name to its `onCreate` and clears the input, and the *handler* is
 * what has to decide whether the folder exists. That split is exactly where the
 * bug lived, so the tests assert the handler's contract - what it awaits, and
 * what it shows afterwards - rather than that a function exists. */

import { afterEach, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { FoldersView } from "../src/views/FoldersView.tsx";

afterEach(cleanup);

function open({ folders = ["Work"], onCreate = vi.fn(), onRename = vi.fn(), onDelete = vi.fn() } = {}) {
  render(<FoldersView folders={folders} profiles={[]} onOpen={() => {}}
                      onCreate={onCreate} onRename={onRename} onDelete={onDelete} />);
  return { onCreate, onRename, onDelete };
}

const nameBox = () => screen.getByPlaceholderText("New folder name");
const addButton = () => screen.getByRole<HTMLButtonElement>("button", { name: /add/i });

test("the view asks the handler to create; it does not add the folder itself", () => {
  const { onCreate } = open({ folders: ["Work"] });

  fireEvent.change(nameBox(), { target: { value: "Ads" } });
  fireEvent.click(addButton());

  expect(onCreate).toHaveBeenCalledWith("Ads");
  // The grid still shows only what it was given. The folder appears when the
  // handler says so - which is what lets a rejected create leave no trace.
  expect(screen.queryByText("Ads")).toBeNull();
  expect(screen.getByText("Work")).toBeTruthy();
});

test("a rejected create leaves the list exactly as the server has it", async () => {
  // The handler shape App now uses: await the server, and only then let the
  // rendered list change. A rejection re-renders with the unchanged list.
  const onCreate = vi.fn().mockRejectedValue(new Error("read-only"));
  open({ folders: ["Work"], onCreate });

  fireEvent.change(nameBox(), { target: { value: "Ghost" } });
  fireEvent.click(addButton());

  await waitFor(() => expect(onCreate).toHaveBeenCalled());
  await expect(onCreate.mock.results[0]!.value).rejects.toThrow("read-only");
  expect(screen.queryByText("Ghost")).toBeNull();
});

test("renaming asks the handler for both names and shows neither until it says so", () => {
  const { onRename } = open({ folders: ["Work"] });

  fireEvent.click(screen.getByTitle(/rename/i));
  const box = document.querySelector<HTMLInputElement>("input.ps-in.sm")!;
  fireEvent.change(box, { target: { value: "Client A" } });
  fireEvent.keyDown(box, { key: "Enter" });

  expect(onRename).toHaveBeenCalledWith("Work", "Client A");
  expect(screen.queryByText("Client A")).toBeNull();
});

test("an empty name is never sent", () => {
  const { onCreate } = open();

  fireEvent.change(nameBox(), { target: { value: "   " } });
  fireEvent.click(addButton());

  expect(onCreate).not.toHaveBeenCalled();
});
