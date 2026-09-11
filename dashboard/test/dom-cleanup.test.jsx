import { expect, test } from "vitest";
import { render } from "@testing-library/react";
import { cleanupTestDom } from "./setup";

test("nested stylesheet cleanup does not accumulate sheets between renders", () => {
  const count = document.styleSheets.length;
  for (let i = 0; i < 3; i++) {
    const { getByText } = render(<div><style>{".cleanup-probe { color: red; }"}</style><p className="cleanup-probe">Probe</p></div>);
    expect(getComputedStyle(getByText("Probe")).color).toBe("rgb(255, 0, 0)");
    expect(document.styleSheets.length).toBe(count + 1);
    cleanupTestDom();
    expect(document.styleSheets.length).toBe(count);
  }
});

test("cleanup preserves stylesheets whose owner is still connected", () => {
  const style = document.createElement("style");
  style.textContent = ".live-probe { color: blue; }";
  document.head.append(style);
  try {
    const sheet = style.sheet;
    cleanupTestDom();
    expect(style.sheet).toBe(sheet);
    expect(style.textContent).toContain("color: blue");
  } finally {
    style.remove();
  }
});
