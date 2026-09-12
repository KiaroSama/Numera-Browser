"""Startup URLs: one per line, and a comma inside a URL stays inside it.

The editor field is a textarea labelled "one per line", but the parser replaced
every comma in the text with a newline before splitting it. An ordinary search
URL - `https://example.test/search?q=red,blue` - was therefore stored as two
navigation targets, the second of them `https://blue`: a host that does not
exist, opened in its own tab on every launch of that profile.

The tests assert the *saved* list, the one `_open_startup_urls` navigates, not
the raw text the user typed. Legacy comma-separated input still splits where a
comma cannot be part of the URL it sits in - between two whole URLs, or inside
what would have to be a host name - because that is what the field used to mean
and existing profiles still carry it.
"""

from numera.convert import _parse_startup_urls, to_engine_profile


def saved(raw):
    """The URLs a stored dashboard profile actually launches."""
    prof = to_engine_profile({"id": "p1", "name": "p", "startupUrls": raw})
    return prof.startup_urls


def test_a_comma_in_a_query_string_is_not_a_separator():
    """The reported defect. A comma between two query values is data, and
    splitting on it invented `https://blue` as a second startup tab."""
    url = "https://example.test/search?q=red,blue"
    assert saved(url) == [url]


def test_a_comma_in_a_path_is_not_a_separator():
    """Same rule one segment earlier - map and API URLs put coordinates and id
    lists in the path."""
    url = "https://example.test/maps/@52.5,13.4,12z"
    assert saved(url) == [url]


def test_an_encoded_comma_is_left_exactly_as_written():
    """%2C is already unambiguous; re-encoding or splitting it would change the
    request the site receives."""
    url = "https://example.test/search?q=red%2Cblue"
    assert saved(url) == [url]


def test_one_url_per_line_is_the_contract():
    """What the field's own label promises, including a line that itself
    contains a comma."""
    assert saved("https://one.test\nhttps://two.test/s?q=a,b") == [
        "https://one.test", "https://two.test/s?q=a,b"]


def test_a_list_is_taken_entry_by_entry():
    """The API may send a list instead of a textarea's text. Each entry is one
    URL, whatever punctuation it contains."""
    assert _parse_startup_urls(["https://one.test/a,b", "two.test"]) == [
        "https://one.test/a,b", "https://two.test"]


def test_a_missing_scheme_is_filled_in():
    """Unchanged behaviour: people paste bare host names."""
    assert saved("example.test/path") == ["https://example.test/path"]


def test_about_blank_is_kept_as_a_scheme_of_its_own():
    """`about:blank` is a real startup target and must not become
    `https://about:blank`."""
    assert saved("about:blank\nhttps://one.test") == ["about:blank", "https://one.test"]


def test_legacy_comma_separated_entries_still_split_where_they_cannot_be_one_url():
    """Profiles saved under the old contract still hold comma-separated text.
    A comma between two schemes, or inside what would have to be a host name,
    cannot be part of a single URL - so that input keeps splitting."""
    assert saved("https://one.test,https://two.test") == [
        "https://one.test", "https://two.test"]
    assert saved("one.test,two.test") == ["https://one.test", "https://two.test"]


def test_nothing_in_means_nothing_out():
    assert saved("") == []
    assert _parse_startup_urls(None) == []
