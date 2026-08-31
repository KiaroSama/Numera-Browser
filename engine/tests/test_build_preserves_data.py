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


def runner(stage) -> str:
    """The real block, run verbatim with `$stage` bound from outside.

    Substituting the text of `$stage` would also rewrite it inside the script's
    own strings, so the block has to receive it as a variable. `Step` is the
    script's logging helper and is not defined out of context.
    """
    literal = str(stage).replace("\\", "\\\\")
    return ("function Step($m) { }\n"
            f'$stage = "{literal}"\n'
            + assemble_block())


def test_a_rebuild_keeps_the_data_directory(tmp_path):
    stage = tmp_path / "Numera"
    (stage / "data" / "profiles").mkdir(parents=True)
    (stage / "data" / "profiles" / "real.json").write_text('{"name": "mine"}', encoding="utf-8")
    (stage / "data" / "config.json").write_text("{}", encoding="utf-8")
    (stage / "numera.exe").write_text("previous build", encoding="utf-8")

    r = subprocess.run([pwsh, "-NoProfile", "-NonInteractive", "-Command", runner(stage)],
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
    r = subprocess.run([pwsh, "-NoProfile", "-NonInteractive", "-Command", runner(stage)],
                       capture_output=True, text=True, timeout=120)
    assert r.returncode == 0, r.stderr
    assert stage.is_dir()


@pytest.mark.skipif(not hasattr(__import__("os"), "O_BINARY"),
                    reason="exclusive file locking is a Windows behaviour")
def test_a_locked_stage_stops_the_build_instead_of_shipping_the_old_one(tmp_path):
    """`Remove-Item` on a file another process holds is a *non-terminating*
    error. The script used to carry on: Copy-Item overwrote what it could, every
    later Test-Path still passed because the old files were there, and the build
    reported success while shipping the previous version. Running the app from
    the stage folder is enough to cause it.
    """
    import msvcrt
    import os

    stage = tmp_path / "Numera"
    (stage / "data" / "profiles").mkdir(parents=True)
    (stage / "data" / "profiles" / "real.json").write_text('{"name": "mine"}', encoding="utf-8")
    (stage / "numera.exe").write_text("previous build", encoding="utf-8")
    locked = stage / "engine" / "held.pyd"
    locked.parent.mkdir()
    locked.write_text("in use", encoding="utf-8")

    fd = os.open(locked, os.O_RDWR | os.O_BINARY)
    try:
        msvcrt.locking(fd, msvcrt.LK_NBLCK, 1)          # exclusive, like a loaded DLL
        r = subprocess.run([pwsh, "-NoProfile", "-NonInteractive", "-Command", runner(stage)],
                           capture_output=True, text=True, timeout=120)
    finally:
        msvcrt.locking(fd, msvcrt.LK_UNLCK, 1)
        os.close(fd)

    assert r.returncode != 0, (
        "the build reported success after failing to clear the stage folder, so it "
        "would have shipped the previous version")
    assert "close Numera" in (r.stderr + r.stdout), \
        "the failure does not say what to do about it"
    # The data directory is moved to a temp folder before the wipe. Giving up
    # without putting it back would be worse than the wipe this guard prevents:
    # the next build would start empty and the profiles would look deleted.
    profile = stage / "data" / "profiles" / "real.json"
    assert profile.is_file(), "the abandoned build left the user's data in a temp folder"
    assert profile.read_text(encoding="utf-8") == '{"name": "mine"}'


def test_the_frozen_engine_includes_the_launch_extra():
    """`engine[api]` freezes an engine that cannot open a browser: `playwright`
    lives in the `launch` extra and is the default engine, so a portable build
    made this way starts, serves the dashboard, and fails at the first launch."""
    text = SCRIPT.read_text(encoding="utf-8")
    assert "engine[api,launch]" in text, \
        "the desktop build does not install the launch dependency"
    assert "engine[api]\"" not in text
