"""Can a real browser start here at all?

Every `*_live.py` suite guarded on `pytest.importorskip("playwright")`, which
answers a different question: whether the *package* is importable. A machine
with the package and without the browser - a fresh CI image, a WSL install
missing `libnspr4`, a container without `playwright install-deps` - produced a
page of red that said nothing about the code, and red that is not about the
code is worse than no signal, because it trains people to ignore the file.

So the guard asks the real question once per session: start a browser, close
it, remember the answer. A browser that cannot start is a skip *with its
reason*; a browser that starts and then misbehaves is still a failure, which is
what these suites are for.
"""

import pytest

_ANSWER: list = []


def _try_launch() -> str:
    """"" if a browser starts here, else why it does not."""
    try:
        from playwright.sync_api import sync_playwright
    except Exception as err:            # noqa: BLE001 - reported, not raised
        return f"playwright is not importable: {err}"
    try:
        with sync_playwright() as pw:
            browser = pw.chromium.launch()
            browser.close()
        return ""
    except Exception as err:            # noqa: BLE001
        first = str(err).strip().splitlines()[0] if str(err).strip() else repr(err)
        return f"chromium will not start here: {first[:200]}"


def requires_browser() -> None:
    """Skip the calling module unless a real browser can be started."""
    if not _ANSWER:
        _ANSWER.append(_try_launch())
    if _ANSWER[0]:
        pytest.skip(_ANSWER[0], allow_module_level=True)
