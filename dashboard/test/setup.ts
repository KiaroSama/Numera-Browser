import { afterEach } from "vitest";
import { cleanup } from "@testing-library/react";

export function cleanupTestDom() {
  cleanup();
  // jsdom 30 can retain a sheet when its ancestor is removed: descendant
  // connection caches are invalidated after the style's detach callback.
  // Invalidate only orphan sheets, after assertions; keep live CSS intact.
  for (const sheet of Array.from(document.styleSheets)) {
    const owner = sheet.ownerNode;
    if (owner && !owner.isConnected) owner.textContent = "";
  }
}

afterEach(cleanupTestDom);
