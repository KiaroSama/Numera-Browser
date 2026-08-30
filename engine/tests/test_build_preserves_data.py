"""The desktop build must not eat the data of the app it is replacing.

`build-desktop.ps1` assembles into `build/windows/Numera`, and the app is
*portable*: run it from there and it writes profiles, browser sessions, proxies
and logs into `<stage>/data`. The assemble step wipes the stage folder, so for
one commit a rebuild silently destroyed a real profile.

Grepping for the guard would only prove the text is present. This runs the real
block out of the real file, so removing or breaking the guard fails the test
rather than the user's profile.
"""
import re
import shutil
import subprocess
from pathlib import Path

import pytest

SCRIPT = Path(__file__).resolve().parents[2] / "scripts" / "build-desktop.ps1"

pwsh = shutil.which("pwsh") or shutil.which("powershell")
pytestmark = pytest.mark.skipif(pwsh is None, reason="no PowerShell on this machine")


def assemble_block() -> str:
    """The part of the script that clears and recreates the stage folder.

    Bounded by the 'Assembling' step and the first Copy-Item, which is where the
    build artefacts start going in - everything before that is the wipe.
    """
    text = SCRIPT.read_text(encoding="utf-8")
    m = re.search(r'^Step "Assembling.*?(?=^Copy-Item )', text, re.S | re.M)
    assert m, "could not find the assemble block; did build-desktop.ps1 change shape?"
    return m.group(0)


def test_a_rebuild_keeps_the_data_directory(tmp_path):
    stage = tmp_path / "Numera"
    (stage / "data" / "profiles").mkdir(parents=True)
    (stage / "data" / "profiles" / "real.json").write_text('{"name": "mine"}', encoding="utf-8")
    (stage / "data" / "config.json").write_text("{}", encoding="utf-8")
    (stage / "numera.exe").write_text("previous build", encoding="utf-8")

    block = assemble_block().replace("$stage", '"' + str(stage).replace("\\", "\\\\") + '"')
    # `Step` is the script's own logging helper and is not defined out of context.
    script = "function Step($m) { }\n" + block
    r = subprocess.run([pwsh, "-NoProfile", "-NonInteractive", "-Command", script],
                       capture_output=True, text=True, timeout=120)
    assert r.returncode == 0, r.stderr

    profile = stage / "data" / "profiles" / "real.json"
    assert profile.is_file(), "the rebuild destroyed the user's profile"
    assert profile.read_text(encoding="utf-8") == '{"name": "mine"}'
    assert (stage / "data" / "config.json").is_file()
    # Everything that is not data is build output and must be gone.
    assert not (stage / "numera.exe").exists(), "the previous build was left behind"


def test_a_first_build_needs_no_data_directory(tmp_path):
    """The guard must not require the folder it preserves."""
    stage = tmp_path / "Numera"
    block = assemble_block().replace("$stage", '"' + str(stage).replace("\\", "\\\\") + '"')
    r = subprocess.run([pwsh, "-NoProfile", "-NonInteractive", "-Command",
                        "function Step($m) { }\n" + block],
                       capture_output=True, text=True, timeout=120)
    assert r.returncode == 0, r.stderr
    assert stage.is_dir()
