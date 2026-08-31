"""One version, declared in five places, that must never disagree again.

The engine reached 0.3.0 while the desktop app and the dashboard sat at 0.1.0
from the first commit, because nothing tied them together and nothing noticed.
A user reading the app's About box was told a number three releases old.

The Python package is the source of truth — it is the one a release is built
from — and everything else has to match it.
"""
import json
import re
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[2]

# Each entry: the file, and how to pull its version out.
SOURCES = {
    "engine/pyproject.toml":
        lambda t: re.search(r'^version = "([^"]+)"', t, re.M).group(1),
    "engine/numera/__init__.py":
        lambda t: re.search(r'^__version__ = "([^"]+)"', t, re.M).group(1),
    "dashboard/package.json":
        lambda t: json.loads(t)["version"],
    "desktop/src-tauri/Cargo.toml":
        lambda t: re.search(r'^version = "([^"]+)"', t, re.M).group(1),
    "desktop/src-tauri/tauri.conf.json":
        lambda t: json.loads(t)["version"],
}


def declared():
    out = {}
    for rel, extract in SOURCES.items():
        path = ROOT / rel
        if path.is_file():
            out[rel] = extract(path.read_text(encoding="utf-8"))
    return out


def test_every_manifest_declares_the_same_version():
    found = declared()
    assert found, "no version manifest was found at all"
    unique = set(found.values())
    assert len(unique) == 1, (
        "the version has drifted across manifests: "
        + ", ".join(f"{k} = {v}" for k, v in sorted(found.items())))


def test_the_running_package_reports_that_version():
    """`numera --version` and the API's About box both read this."""
    from numera import __version__
    assert __version__ == declared()["engine/pyproject.toml"]


def test_the_version_is_a_release_number():
    v = declared()["engine/pyproject.toml"]
    assert re.fullmatch(r"\d+\.\d+\.\d+", v), f"{v!r} is not MAJOR.MINOR.PATCH"


@pytest.mark.skipif(not (ROOT / "CHANGELOG.md").is_file(), reason="no changelog")
def test_the_changelog_has_an_entry_for_it():
    """A bump with nothing written down tells a user the number moved and
    nothing else."""
    v = declared()["engine/pyproject.toml"]
    text = (ROOT / "CHANGELOG.md").read_text(encoding="utf-8")
    assert f"## [{v}]" in text, f"CHANGELOG.md has no section for {v}"
