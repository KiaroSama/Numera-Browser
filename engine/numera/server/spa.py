"""Serving the built dashboard from the engine itself.

One process, one port: `numera serve` is the whole application. The
catch-all route must refuse `/api/*` - without that a mistyped endpoint
returns the HTML page with status 200 and the dashboard tries to parse it
as JSON."""

from __future__ import annotations

from pathlib import Path
from typing import Optional


def dashboard_dir() -> Optional[Path]:
    """The built dashboard, or None when it hasn't been built.

    Two layouts to find: a packaged app, where the build sits beside the
    `numera` package as `web/`, and a source checkout, where it lands in
    `dashboard/dist` at the repo root.

    The checkout case *searches upward* rather than counting parent levels. It
    used to be `here.parents[2]`, which was right while this lived in
    `numera/server.py` and silently wrong the moment it moved one directory
    deeper - the API still started, the tests still passed, and every page
    404'd. A path expressed as "N levels up" is a hidden dependency on the
    file's own location.
    """
    here = Path(__file__).resolve()
    packaged = here.parents[1] / "web"          # numera/web, next to the package
    if (packaged / "index.html").is_file():
        return packaged
    for parent in here.parents:
        candidate = parent / "dashboard" / "dist"
        if (candidate / "index.html").is_file():
            return candidate
    return None


def _mount_dashboard(app) -> None:
    """Serve the dashboard from the API itself, so one process on one port is
    the whole application - no second dev server, no CORS, nothing for a user to
    start by hand.

    The SPA needs any unknown path to return index.html so client-side routing
    works on a refresh. That catch-all must never swallow the API, so it is
    registered last and explicitly refuses /api/*: without that, a typo'd
    endpoint would return the HTML page with status 200 and the dashboard would
    try to JSON.parse it."""
    root = dashboard_dir()
    if root is None:
        return

    # Imported here, not at module scope: fastapi is an optional extra, and the
    # engine must stay importable without it.
    from fastapi import HTTPException
    from fastapi.responses import FileResponse
    from fastapi.staticfiles import StaticFiles

    app.mount("/assets", StaticFiles(directory=str(root / "assets")), name="assets")

    index = root / "index.html"

    @app.get("/{path:path}", include_in_schema=False)
    def _spa(path: str):
        if path.startswith("api/"):
            raise HTTPException(404, "no such API endpoint")
        # A real file (favicon, icon, manifest) wins over the SPA fallback.
        candidate = (root / path).resolve()
        if path and candidate.is_file() and candidate.is_relative_to(root):
            return FileResponse(candidate)
        return FileResponse(index)
