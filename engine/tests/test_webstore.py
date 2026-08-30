"""Chrome Web Store URLs -> a folder Chromium can load.

The download itself needs the network, so it lives in test_extensions_live.py.
What is worth pinning offline is the part that has to be exactly right: the id
rules, the CRX header arithmetic, and the refusal to write outside the folder.
"""
import io
import json
import struct
import zipfile
from pathlib import Path

import pytest

from numera import webstore
from numera.webstore import WebStoreError


def crx3(payload: bytes, header: bytes = b"sig") -> bytes:
    """A minimal CRX3: magic, version, header length, header, then the zip."""
    return (b"Cr24" + struct.pack("<I", 3) + struct.pack("<I", len(header))
            + header + payload)


def zip_bytes(files: dict) -> bytes:
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w") as z:
        for name, data in files.items():
            z.writestr(name, data)
    return buf.getvalue()


@pytest.mark.parametrize("text", [
    "ddkjiahejlhfcafbddmgiahcphecmpfh",
    "https://chromewebstore.google.com/detail/ublock-origin-lite/ddkjiahejlhfcafbddmgiahcphecmpfh",
    "https://chrome.google.com/webstore/detail/x/ddkjiahejlhfcafbddmgiahcphecmpfh?hl=en",
    "  https://chromewebstore.google.com/detail/ublock/ddkjiahejlhfcafbddmgiahcphecmpfh/reviews  ",
])
def test_id_is_found_in_every_shape_of_link(text):
    assert webstore.extension_id(text) == "ddkjiahejlhfcafbddmgiahcphecmpfh"


def test_a_truncated_id_says_so():
    # 31 characters: what you get by selecting the id in the address bar and
    # missing the last one. "not found" would send the user hunting in the
    # wrong place, so the length has to be named.
    with pytest.raises(WebStoreError, match="31 characters"):
        webstore.extension_id("https://chromewebstore.google.com/detail/x/ddkjiahejlhfcafbddmgiahcphecmpf")


def test_a_letter_past_p_says_which_one():
    with pytest.raises(WebStoreError, match="contains q"):
        webstore.extension_id("https://chromewebstore.google.com/detail/buster/mpbjkejclgfqadimmefqebjfooflfhl")


def test_empty_input():
    with pytest.raises(WebStoreError):
        webstore.extension_id("")


def test_unpack_strips_the_header(tmp_path):
    crx = crx3(zip_bytes({"manifest.json": '{"name": "Test", "version": "1"}'}))
    webstore._unpack(crx, tmp_path)
    assert (tmp_path / "manifest.json").is_file()


def test_unpack_refuses_to_escape_the_folder(tmp_path):
    # The archive is third-party content; a member named ../evil.js must not be
    # written, and nothing may be extracted before that is known.
    crx = crx3(zip_bytes({"../escaped.js": "x", "manifest.json": "{}"}))
    with pytest.raises(WebStoreError, match="outside the folder"):
        webstore._unpack(crx, tmp_path / "ext")
    assert not (tmp_path / "escaped.js").exists()


def test_unpack_rejects_an_unknown_crx_version(tmp_path):
    with pytest.raises(WebStoreError, match="unsupported CRX version"):
        webstore._unpack(b"Cr24" + struct.pack("<I", 9) + b"\0" * 8, tmp_path)


def test_name_falls_back_to_the_locale_file(tmp_path):
    ext = tmp_path / "ext"
    (ext / "_locales" / "en").mkdir(parents=True)
    (ext / "manifest.json").write_text(
        '{"name": "__MSG_extName__", "default_locale": "en"}', encoding="utf-8")
    (ext / "_locales" / "en" / "messages.json").write_text(
        '{"extName": {"message": "Real Name"}}', encoding="utf-8")
    assert webstore._name(ext) == "Real Name"


def test_name_survives_a_broken_manifest(tmp_path):
    ext = tmp_path / "abcd"
    ext.mkdir()
    (ext / "manifest.json").write_text("{not json", encoding="utf-8")
    assert webstore._name(ext) == "abcd"      # the id, never an exception


def test_installed_lists_only_real_extensions(tmp_path):
    root = tmp_path / "extensions"
    good = root / ("a" * 32)
    good.mkdir(parents=True)
    (good / "manifest.json").write_text('{"name": "Good"}', encoding="utf-8")
    (root / "leftover").mkdir()               # no manifest: a half-removed folder
    listed = webstore.installed(tmp_path)
    assert [e["name"] for e in listed] == ["Good"]


def test_uninstall_removes_the_folder(tmp_path):
    ext_id = "a" * 32
    d = tmp_path / "extensions" / ext_id
    d.mkdir(parents=True)
    (d / "manifest.json").write_text("{}", encoding="utf-8")
    assert webstore.uninstall(ext_id, tmp_path) is True
    assert not d.exists()
    assert webstore.uninstall(ext_id, tmp_path) is False     # already gone


def write_ext(folder: Path, manifest: dict, files: dict = None) -> Path:
    folder.mkdir(parents=True, exist_ok=True)
    (folder / "manifest.json").write_text(json.dumps(manifest), encoding="utf-8")
    for name, data in (files or {}).items():
        f = folder / name
        f.parent.mkdir(parents=True, exist_ok=True)
        f.write_bytes(data if isinstance(data, bytes) else data.encode())
    return folder


def test_icon_picks_the_largest_declared_size(tmp_path):
    ext = write_ext(tmp_path / "e",
                    {"name": "X", "icons": {"16": "s.png", "128": "l.png", "48": "m.png"}},
                    {"s.png": b"s", "m.png": b"m", "l.png": b"l"})
    assert webstore.icon(ext).name == "l.png"


def test_icon_skips_a_size_whose_file_is_missing(tmp_path):
    # Manifests routinely name an icon that was not shipped. Returning that path
    # would render a broken image; the next size down actually exists.
    ext = write_ext(tmp_path / "e",
                    {"name": "X", "icons": {"128": "gone.png", "48": "here.png"}},
                    {"here.png": b"h"})
    assert webstore.icon(ext).name == "here.png"


def test_icon_falls_back_to_the_action_icon(tmp_path):
    ext = write_ext(tmp_path / "e",
                    {"name": "X", "action": {"default_icon": {"32": "a.png"}}},
                    {"a.png": b"a"})
    assert webstore.icon(ext).name == "a.png"


def test_icon_accepts_a_plain_string_action_icon(tmp_path):
    ext = write_ext(tmp_path / "e",
                    {"name": "X", "browser_action": {"default_icon": "only.png"}},
                    {"only.png": b"o"})
    assert webstore.icon(ext).name == "only.png"


def test_icon_refuses_a_path_that_escapes_the_folder(tmp_path):
    # The manifest is third-party text. `../` in an icon path must not turn the
    # icon endpoint into a file reader for the rest of the disk.
    (tmp_path / "secret.png").write_bytes(b"nope")
    ext = write_ext(tmp_path / "e", {"name": "X", "icons": {"128": "../secret.png"}})
    assert webstore.icon(ext) is None


def test_icon_is_none_when_nothing_is_declared(tmp_path):
    ext = write_ext(tmp_path / "e", {"name": "X"})
    assert webstore.icon(ext) is None


def test_describe_carries_name_and_icon(tmp_path):
    ext = write_ext(tmp_path / "e", {"name": "Real Name", "icons": {"48": "i.png"}},
                    {"i.png": b"i"})
    d = webstore.describe(ext)
    assert d["name"] == "Real Name"
    assert d["icon"].endswith("i.png")
    assert d["path"] == str(ext)
