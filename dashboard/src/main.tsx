import React from "react";
import { createRoot } from "react-dom/client";
import App, { ErrorBoundary } from "./App.tsx";

// Last resort: App itself scans every profile before it renders anything, so a
// malformed one can throw above the in-app boundaries. Better a readable panel
// than a white page.
createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <ErrorBoundary title="Numera Browser couldn't start">
      <App />
    </ErrorBoundary>
  </React.StrictMode>
);
