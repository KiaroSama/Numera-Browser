"""Every profile field the engine reads must be settable in the editor.

The mirror image of the dead-control bug this project keeps hitting. A control
the engine ignores does nothing; a field the engine *reads* but the editor never
offers is just as bad the other way - every profile silently takes the default
and the user has no way to change it. `clientRects`, `webglImage`,
`mediaDevices`, `mediaSpeakers` and `fontList` were all in that state: mapped in
both directions by `convert.py`, invisible in the UI.

Reads `to_engine_profile` for what the engine consumes and the editor source for
what it writes, so neither list has to be maintained by hand.
"""
import re
from pathlib import Path

import pytest

SRC = Path(__file__).resolve().parents[2] / "dashboard" / "src"
# The conversion moved to the core in 0.20.0 so the CLI could use it without
# the HTTP package; `server/convert.py` is now a re-export, and reading that
# would pass this test vacuously.
CONVERT = Path(__file__).resolve().parents[1] / "numera" / "convert.py"

pytestmark = pytest.mark.skipif(not SRC.is_dir(), reason="dashboard sources not present")

# Keys the engine reads that the editor is not expected to expose, each with the
# reason. Anything not listed here has to be reachable in the UI.
NOT_IN_EDITOR = {
    "id": "assigned by the store",
    "seed": "derived from the id so canvas noise is stable across launches",
    "name": "edited in the page header, not a tab field",
    "devicePixelRatio": "follows the screen and mobile flag; set by generate()",
    "maxTouchPoints": "follows the mobile flag",
    "deviceModel": "only meaningful for an Android UA, which bulk-create sets",
    "osVersion": "chosen with the OS archetype",
    "mediaVideo": "exposed as Cameras",
    "mediaAudio": "exposed as Microphones",
}


def engine_reads() -> set[str]:
    """Dashboard keys `to_engine_profile` consumes."""
    text = CONVERT.read_text(encoding="utf-8")
    body = text[text.index("def to_engine_profile"):]
    body = body[:body.index("\ndef ")] if "\ndef " in body else body
    return set(re.findall(r'd\.get\("(\w+)"', body))


def editor_writes() -> set[str]:
    """Keys any view writes through `set({ key: ... })` or a named setter."""
    keys = set()
    for path in SRC.rglob("*.jsx"):
        text = path.read_text(encoding="utf-8")
        keys |= set(re.findall(r"set\(\{\s*(\w+)\s*[:,}]", text))
        keys |= set(re.findall(r"set\(\{\s*\.\.\.\w+,\s*(\w+)\s*[:,}]", text))
    return keys


def test_every_engine_field_is_reachable_in_the_editor():
    missing = sorted(engine_reads() - editor_writes() - set(NOT_IN_EDITOR))
    assert not missing, (
        f"the engine reads {missing} but nothing in the dashboard sets them - "
        "every profile takes the default and the user cannot change it. Add the "
        "control, or add the key to NOT_IN_EDITOR with the reason.")


def test_the_exemption_list_has_not_gone_stale():
    """An exemption for a key the engine no longer reads is a lie in the file."""
    stale = sorted(set(NOT_IN_EDITOR) - engine_reads())
    assert not stale, f"NOT_IN_EDITOR names keys the engine no longer reads: {stale}"


def test_the_summary_panel_stays_in_step_with_the_editor():
    """The summary must not claim a value the editor cannot produce.

    It is the panel a user reads to cross-check the identity, so a row showing a
    field nobody can set is worse than no row.
    """
    # The panel moved out of `Editor.jsx` into its own file when that file hit
    # the 800-line ceiling. Located by where the component actually lives rather
    # than by which file used to hold it: this assertion is about the panel, and
    # `index()` raising `ValueError: substring not found` told us nothing about
    # the summary at all.
    panel = SRC / "views" / "ProfileSummary.jsx"
    assert panel.is_file(), f"the summary panel is not at {panel}"
    block = panel.read_text(encoding="utf-8")
    assert "function ProfileSummary" in block, "ProfileSummary moved again"
    shown = set(re.findall(r"p\.(\w+)", block))
    known = editor_writes() | set(NOT_IN_EDITOR) | {
        "proxy", "extensions", "startupUrls", "tags", "notes", "engine", "os",
    }
    unknown = sorted(shown - known)
    assert not unknown, f"the summary reads fields nothing sets: {unknown}"


def test_a_minimal_post_cannot_blank_the_profile_list():
    """`{"name": "alpha"}` is a reasonable API call. It used to be stored with no
    `tags`, and the list view calls `p.tags.map(...)` - so one such profile took
    the whole page down on the next load. The write path fills what the view
    dereferences instead of refusing the request.
    """
    import re as _re
    from numera.server import routes_profiles as rp

    view = (SRC / "views" / "ProfilesView.jsx").read_text(encoding="utf-8")
    dereferenced = set(_re.findall(r"p\.([a-zA-Z]+)\.(?:map|join|split)\b", view))
    filled = set(rp.LIST_FIELDS) | set(rp.TEXT_FIELDS)
    # `name` is required by the API in its own right and checked separately.
    missing = sorted(dereferenced - filled - {"name"})
    assert not missing, (
        f"the profile list calls a method on {missing}, but a POST that omits "
        "them is still stored - one such profile blanks the page")


def test_a_profile_created_without_a_user_agent_still_gets_a_real_one():
    """Every other field in the converter falls back to something coherent; the
    user agent alone fell back to "". The dashboard always sends one, so only the
    API path could hit it - and a profile presenting an empty UA is a worse tell
    than any fingerprint mismatch.
    """
    from numera.server.convert import to_engine_profile

    for os_name, marker in [("Windows", "Windows NT"), ("macOS", "Mac OS X"),
                            ("Linux", "X11; Linux"), ("Android", "Android")]:
        ua = to_engine_profile({"name": "x", "os": os_name}).fingerprint.user_agent
        assert marker in ua, f"{os_name} produced {ua!r}"
        assert "Chrome/" in ua


def test_an_explicit_user_agent_is_never_overwritten():
    from numera.server.convert import to_engine_profile

    mine = "Mozilla/5.0 (something deliberate)"
    assert to_engine_profile({"name": "x", "userAgent": mine}).fingerprint.user_agent == mine
