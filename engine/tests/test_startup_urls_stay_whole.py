"""A line that is one URL stays one URL - through the API and onto disk.

Second pass over the same field. The first fix stopped `?q=red,blue` from
becoming two startup tabs and left two ways to cut a URL in half:

    https://example.test/redirect?targets=https://a.test,https://b.test
      -> ['https://example.test/redirect?targets=https://a.test', 'https://b.test']
    https://example.test/red,blue:green
      -> ['https://example.test/red', 'https://blue:green']

Both are one URL. The splitter read a colon *after* a comma as the start of a
new scheme wherever that comma sat, so any query value carrying a URL of its own
- every redirect, sign-in return and share link - arrived at launch truncated,
with its tail opened as a second tab. The second line shows what the heuristic
was really doing: `blue:green` is a path segment, and nothing after the host can
say where an entry ends.

The rule now has one source of truth: a comma separates entries only while the
entry before it is still a scheme and a host, because no host name contains a
comma. Once an entry has reached a path, query or fragment, every comma in it is
data. An explicit list never splits at all - a caller that sent a list has
already said where each entry ends, and overriding that is worse than guessing,
it is guessing against an answer already given.

Legacy comma-separated *hosts* are what the field used to mean and still parse,
because that input has nowhere else for the comma to belong.

The round-trip tests read the engine twin on disk rather than the parser's
return value: `startup_urls` in that file is the list `_open_startup_urls`
navigates, and a profile is only correct if what survives the save is right.
"""

import json

import pytest

from numera.convert import _parse_startup_urls, to_engine_profile

pytest.importorskip("fastapi")

from fastapi.testclient import TestClient                       # noqa: E402

from numera.server import create_app                            # noqa: E402


#: The two lines the audit reproduced. Each one is a single navigation target.
CARRIES_A_URL = "https://example.test/redirect?targets=https://a.test,https://b.test"
CARRIES_A_COLON = "https://example.test/red,blue:green"


def saved(raw):
    """The URLs a stored dashboard profile actually launches."""
    return to_engine_profile({"id": "p1", "name": "p", "startupUrls": raw}).startup_urls


@pytest.fixture
def c(tmp_path):
    return TestClient(create_app(data_dir=str(tmp_path), start_scheduler=False))


def launched(tmp_path, pid):
    """What the launcher will open: the engine twin's own list, off disk."""
    twin = tmp_path / "profiles" / f"{pid}.json"
    return json.loads(twin.read_text(encoding="utf-8"))["startup_urls"]


def reloaded(c, pid):
    """The profile as the dashboard reads it back."""
    return next(p for p in c.get("/api/profiles").json() if p["id"] == pid)


# --- one URL in, one URL out ------------------------------------------------

def test_a_url_inside_a_query_value_is_not_a_second_entry():
    """The reported defect. `targets=` holds two URLs as data; splitting there
    both loses half the parameter and navigates to a target nobody typed."""
    assert saved(CARRIES_A_URL) == [CARRIES_A_URL]


def test_a_colon_in_a_path_segment_does_not_start_a_new_url():
    """`blue:green` is a path segment. Reading it as a scheme is the guess that
    made the line above split, stated in its smallest form."""
    assert saved(CARRIES_A_COLON) == [CARRIES_A_COLON]


def test_a_comma_in_a_fragment_stays_in_the_fragment():
    """A fragment ends the entry exactly as a path or query does."""
    url = "https://example.test/#section,two"
    assert saved(url) == [url]


def test_an_encoded_comma_is_neither_a_split_point_nor_decoded():
    """%2C and a literal comma on the same line: the encoded one is not a
    separator, and it must reach the browser still encoded - decoding it would
    change the request the site receives."""
    url = "https://example.test/s?q=a%2Cb,c%2Cd"
    assert saved(url) == [url]


def test_one_url_per_line_is_still_the_contract():
    """What the field's label promises, over lines that each hold a comma."""
    assert saved(f"{CARRIES_A_URL}\n{CARRIES_A_COLON}") == [CARRIES_A_URL, CARRIES_A_COLON]


def test_about_blank_keeps_its_own_scheme_beside_a_comma_line():
    """`about:blank` is a real startup target and must not collect an
    `https://`, nor drag the line after it into itself."""
    assert saved(f"about:blank\n{CARRIES_A_URL}") == ["about:blank", CARRIES_A_URL]


# --- an explicit list is the caller's own answer ----------------------------

def test_an_explicit_list_entry_is_never_split():
    """A list says where each entry ends. The parser splitting one anyway is
    the same corruption with the caller's intent already on record."""
    assert _parse_startup_urls([CARRIES_A_URL, CARRIES_A_COLON]) == [
        CARRIES_A_URL, CARRIES_A_COLON]


def test_a_list_entry_holding_the_legacy_comma_form_is_one_entry():
    """The legacy form is a property of the *textarea*, not of a list. An
    element that looks like the old input is still one element."""
    assert _parse_startup_urls(["one.test,two.test"]) == ["https://one.test,two.test"]


def test_a_list_still_gets_a_missing_scheme_filled_in():
    """Unchanged: not splitting an entry is not the same as not normalising it."""
    assert _parse_startup_urls([CARRIES_A_URL, "two.test"]) == [
        CARRIES_A_URL, "https://two.test"]


# --- the legacy form this field used to mean --------------------------------

def test_legacy_comma_separated_hosts_still_split():
    """A comma between two hosts has nowhere else to belong - no host name
    contains one - so profiles saved under the old contract keep working."""
    assert saved("one.test,two.test") == ["https://one.test", "https://two.test"]
    assert saved("https://one.test,https://two.test") == [
        "https://one.test", "https://two.test"]


def test_a_legacy_split_leaves_the_rest_of_the_line_alone():
    """The split happens at the host comma and stops there: the entry it starts
    keeps every comma of its own, encoded or not."""
    assert saved("one.test,https://example.test/s?q=a%2Cb,c") == [
        "https://one.test", "https://example.test/s?q=a%2Cb,c"]


# --- the save/load round trip -----------------------------------------------

def test_a_saved_profile_launches_the_urls_that_were_typed(c, tmp_path):
    """The check that matters: not what the parser returns, but what the stored
    profile opens. The engine twin is written from the converted profile, so a
    parser that splits puts the extra tab on disk."""
    typed = f"{CARRIES_A_URL}\n{CARRIES_A_COLON}\nhttps://example.test/s?q=a%2Cb,c%2Cd"
    r = c.post("/api/profiles", json={"name": "typed", "startupUrls": typed})
    assert r.status_code == 200, r.text
    pid = r.json()["id"]

    assert launched(tmp_path, pid) == typed.splitlines()
    assert reloaded(c, pid)["startupUrls"] == typed, "the editor text was rewritten"


def test_an_unrelated_edit_does_not_re_split_the_urls(c, tmp_path):
    """A profile is saved many times. Every PUT re-converts the merged record,
    so a splitting parser corrupts the list on an edit that never touched it."""
    typed = f"{CARRIES_A_URL}\n{CARRIES_A_COLON}"
    pid = c.post("/api/profiles", json={"name": "edited", "startupUrls": typed}).json()["id"]

    r = c.put(f"/api/profiles/{pid}", json={"notes": "renamed something else"})
    assert r.status_code == 200, r.text
    assert launched(tmp_path, pid) == typed.splitlines()
    assert reloaded(c, pid)["startupUrls"] == typed


def test_a_saved_legacy_profile_still_launches_both_hosts(c, tmp_path):
    """The other half of the contract, through the same path."""
    pid = c.post("/api/profiles", json={"name": "legacy",
                                        "startupUrls": "one.test,two.test"}).json()["id"]
    assert launched(tmp_path, pid) == ["https://one.test", "https://two.test"]
