/* Every control in a field has its own name, and the field's label lands on
 * the control it was written for.
 *
 * What this pins: `Field` used to be a `<label>` wrapped around the whole
 * group. A `<label>` with no `for` names the FIRST labelable descendant in tree
 * order and nothing else, so a field that leads with a segmented control gave
 * the field's name to the first BUTTON - "Timezone" where the button says
 * "Auto" - and left the input the label was written for with no accessible name
 * at all. Two buttons a screen reader cannot tell apart, beside a nameless
 * text box.
 *
 * The numbers in these assertions are what the DOM actually reported before the
 * fix: `input.labels.length === 0`, `button.labels.length === 1`.
 */

import { afterEach, expect, test } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { Field } from "../src/ui.jsx";

afterEach(() => cleanup());

test("a button in a field keeps its own name, and never the field's", () => {
  render(
    <Field label="Timezone">
      <div className="ps-seg">
        <button type="button">Auto</button>
        <button type="button">Manual</button>
      </div>
      <input className="ps-in" defaultValue="Asia/Tehran" onChange={() => {}} />
    </Field>
  );

  // Each button answers to what it says, not to the field it sits in.
  expect(screen.getByRole("button", { name: "Auto" })).toBeTruthy();
  expect(screen.getByRole("button", { name: "Manual" })).toBeTruthy();
  expect(screen.queryAllByRole("button", { name: "Timezone" }),
    "a button is wearing the field's label").toHaveLength(0);

  // And the field's label reaches the control it was written for.
  expect(screen.getByRole("textbox", { name: "Timezone" }),
    "the input the label describes has no accessible name").toBeTruthy();
});

test("the plain case still works: one control, named by its field", () => {
  render(<Field label="Profile name"><input className="ps-in" defaultValue="x" onChange={() => {}} /></Field>);
  expect(screen.getByRole("textbox", { name: "Profile name" })).toBeTruthy();
});

test("a select and a textarea are named too, not only inputs", () => {
  render(
    <>
      <Field label="Operating system">
        <select defaultValue="Windows" onChange={() => {}}><option>Windows</option></select>
      </Field>
      <Field label="Notes"><textarea defaultValue="" onChange={() => {}} /></Field>
    </>
  );
  expect(screen.getByRole("combobox", { name: "Operating system" })).toBeTruthy();
  expect(screen.getByRole("textbox", { name: "Notes" })).toBeTruthy();
});

test("a control that brought its own name keeps it", () => {
  render(
    <Field label="Timezone">
      <input className="ps-in" aria-label="Timezone search" defaultValue="" onChange={() => {}} />
    </Field>
  );
  expect(screen.getByRole("textbox", { name: "Timezone search" })).toBeTruthy();
});

test("the group itself is still announced by its label", () => {
  render(<Field label="Screen"><div className="ps-seg"><button type="button">Auto</button></div></Field>);
  expect(screen.getByRole("group", { name: "Screen" })).toBeTruthy();
});
