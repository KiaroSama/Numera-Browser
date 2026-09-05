"""What every probe needs: the check shape, and a page that costs no network.

`_check` is the one record type the CLI, the API and the dashboard all render,
so it lives here rather than in whichever probe happened to define it first.
"""

from __future__ import annotations

PROBE_URL = "https://numera.probe/"
PROBE_HTML = "<!doctype html><meta charset=utf-8><title>numera probe</title>"


def _check(name: str, ok: bool, detail: str = "") -> dict:
    return {"name": name, "ok": bool(ok), "detail": detail}


def _serve_probe_page(context):
    """Give the context a page on a real https origin without touching the network."""
    page = context.pages[0] if context.pages else context.new_page()
    page.route("**/*", lambda route: route.fulfill(
        status=200, content_type="text/html", body=PROBE_HTML))
    page.goto(PROBE_URL, wait_until="domcontentloaded", timeout=15000)
    return page
