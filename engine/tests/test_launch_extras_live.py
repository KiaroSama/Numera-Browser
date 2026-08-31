"""Real-browser proof that the bookmarks and extra flags survive a launch.

The unit tests show the file is written and the flags are assembled. Neither
answers the question that decides whether the feature exists: does *Chromium*
accept them? A `Bookmarks` file it considers malformed is discarded and rebuilt
empty, and an unrecognised flag is ignored — in both cases the user gets a
control that stored their input and changed nothing, which is the bug class this
project keeps hitting.
"""
import json

import pytest

from numera import generate
from numera.models import Profile
from numera.store import ProfileStore

pytest.importorskip("playwright")

from numera import drivers  # noqa: E402  - only meaningful once playwright is here

pytestmark = pytest.mark.slow


def make(tmp_path, **kw):
    store = ProfileStore(tmp_path)
    p = Profile(name="extras", fingerprint=generate(os="windows"), **kw)
    store.save(p)
    return p, store


def test_chromium_keeps_the_bookmarks_it_was_given(tmp_path):
    """A format Chromium rejects is replaced with an empty bar on startup, so
    finding them still there after a real session is the proof."""
    marks = [{"name": "Example", "url": "https://example.com/"},
             {"name": "GitHub", "url": "https://github.com/"}]
    p, store = make(tmp_path, bookmarks=marks)

    with drivers.session(p, store, headless=True) as ctx:
        ctx.pages[0].goto("about:blank")

    doc = json.loads((store.user_data_path(p.id) / "Default" / "Bookmarks")
                     .read_text(encoding="utf-8"))
    bar = doc["roots"]["bookmark_bar"]["children"]
    assert [c["url"] for c in bar] == [m["url"] for m in marks], \
        "Chromium discarded the bookmark file it was handed"
    assert bar[0]["name"] == "Example"


def test_an_extra_flag_reaches_the_running_browser(tmp_path):
    """`--blink-settings` is observable from the page, so a flag that never made
    it onto the command line cannot fake this."""
    p, store = make(tmp_path, launch_args=["--blink-settings=defaultFontSize=33"])
    with drivers.session(p, store, headless=True) as ctx:
        page = ctx.pages[0]
        page.goto("about:blank")
        size = page.evaluate("() => getComputedStyle(document.body).fontSize")
    assert size == "33px", f"the flag did not reach Chromium (font size {size})"


def test_a_profile_with_no_extras_still_launches(tmp_path):
    """The additions must not become a required step for everyone else."""
    p, store = make(tmp_path)
    with drivers.session(p, store, headless=True) as ctx:
        assert ctx.pages[0].evaluate("() => 1 + 1") == 2


def test_a_profile_with_both_subsystems_off_still_works(tmp_path):
    """Chromium ignores a flag it does not recognise, so "it launched" proves
    little on its own - but a *malformed* switch can break startup, and these
    disable real subsystems the browser might depend on."""
    p, store = make(tmp_path, switched_off=["googleServices", "systemExtensions"],
                    dns="1.1.1.1")
    with drivers.session(p, store, headless=True) as ctx:
        page = ctx.pages[0]
        page.goto("about:blank")
        assert page.evaluate("() => navigator.userAgent") == p.fingerprint.user_agent
        assert page.evaluate("() => 1 + 1") == 2
