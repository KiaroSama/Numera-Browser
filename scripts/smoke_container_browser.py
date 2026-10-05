"""Exercise the final image's Chromium offline as its configured non-root user.

Run inside the built image, for example by feeding this file to ``python -``.
This does not install browsers, use a host cache or contact an external site.
The CI caller additionally enforces an outer deadline and disables networking.
"""
from __future__ import annotations

import argparse
import logging
import os
import re
from importlib import metadata
from pathlib import Path

LOG = logging.getLogger("numera.container-smoke")
VERSION_ENV = "NUMERA_CONTAINER_PLAYWRIGHT_VERSION"
BROWSER_ROOT = Path("/ms-playwright")
PAGE_TEXT = "numera-container-ready"


class SmokeFailure(RuntimeError):
    """The installed image failed its browser compatibility contract."""


def _close_resources(page, context, browser) -> None:
    """Attempt every cleanup even when an earlier close fails."""
    failures = []
    for name, resource in (("page", page), ("context", context), ("browser", browser)):
        if resource is None:
            continue
        try:
            resource.close()
            LOG.debug("Closed %s", name)
        except Exception as error:
            failures.append(name)
            LOG.warning("Failed to close %s (%s)", name, type(error).__name__)
    try:
        if browser.is_connected():
            failures.append("browser still connected")
    except Exception as error:
        failures.append("browser connection unverified")
        LOG.warning("Cannot verify browser disconnect (%s)", type(error).__name__)
    if failures:
        raise SmokeFailure("Incomplete browser cleanup: " + ", ".join(failures))


def check_browser(expected: str) -> None:
    """Check the client, selected image binary, real page and orderly closure."""
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", expected):
        raise SmokeFailure("Image must declare an exact Playwright release")
    installed = metadata.version("playwright")
    if installed != expected:
        raise SmokeFailure(f"Playwright client {installed} does not match image pin {expected}")
    if not hasattr(os, "geteuid") or os.geteuid() == 0:
        raise SmokeFailure("Run this Linux image smoke as its configured non-root user")
    LOG.info("Verified installed Playwright client %s", installed)

    # Import only after version validation; mismatches must not launch a driver.
    from playwright.sync_api import sync_playwright

    with sync_playwright() as playwright:
        executable = Path(playwright.chromium.executable_path).resolve(strict=True)
        if not executable.is_relative_to(BROWSER_ROOT.resolve(strict=True)):
            raise SmokeFailure("Chromium resolves outside the image browser directory")
        if not executable.is_file() or not os.access(executable, os.X_OK):
            raise SmokeFailure("Image Chromium executable is absent or not executable")
        LOG.debug("Selected bundled Chromium: %s", executable)
        browser = playwright.chromium.launch(
            executable_path=str(executable), headless=True, timeout=10000,
        )
        page = context = None
        try:
            context = browser.new_context()
            page = context.new_page()
            page.set_content(
                f'<main id="numera-smoke">{PAGE_TEXT}</main>',
                wait_until="load", timeout=5000,
            )
            actual = page.locator("#numera-smoke").inner_text(timeout=5000)
            if actual != PAGE_TEXT or page.evaluate("() => 6 * 7") != 42:
                raise SmokeFailure("Bundled Chromium returned an unexpected page result")
            LOG.info("Bundled Chromium rendered the local page and executed JavaScript")
        finally:
            _close_resources(page, context, browser)
    # The driver context has exited too; returning from the page is insufficient.
    LOG.info("Browser and driver closed; container browser smoke passed")


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--debug", action="store_true", help="Enable diagnostic logging")
    args = parser.parse_args(argv)
    # Standard logging supports DEBUG, INFO, WARNING and ERROR. No environment
    # dump, credential, token or profile data is included in these diagnostics.
    logging.basicConfig(
        level=logging.DEBUG if args.debug else logging.INFO,
        format="%(asctime)s [%(levelname)s] %(message)s",
    )
    try:
        check_browser(os.environ.get(VERSION_ENV, ""))
    except Exception as error:
        LOG.error("Container browser smoke failed (%s): %s", type(error).__name__, error)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
