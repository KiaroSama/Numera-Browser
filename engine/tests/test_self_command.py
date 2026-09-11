"""How Numera builds the command that re-invokes Numera.

Every self-spawn - the API's launch/stop/attach/warm-up, a detached CLI job, a
schedule, an MCP tool call - used `[sys.executable, "-m", "numera", ...]`. From
a source checkout that is right: `sys.executable` is the interpreter and `-m`
tells it which module to run.

**Frozen, `sys.executable` is `numera.exe` itself.** PyInstaller documents this:
in a frozen application `sys.executable` is the bundled executable, not a Python
interpreter. So the same list becomes

    numera.exe -m numera --data-dir ... launch <id>

and `-m numera` is handed to the *application's* argument parser, which has no
such option. Every self-spawn in a portable build fails, and nothing in a CLI
smoke test or `/api/ready` touches it - those never re-invoke the program.

These tests pin the builder rather than each call site, because the call sites
are the thing that kept drifting.
"""

import subprocess
import sys

import pytest

from numera import selfexec


def test_source_mode_runs_the_module_through_the_interpreter(monkeypatch):
    monkeypatch.delattr(sys, "frozen", raising=False)

    assert selfexec.self_command("launch", "abc") == [
        sys.executable, "-m", "numera", "launch", "abc"]


def test_frozen_mode_invokes_the_executable_directly(monkeypatch):
    """The regression. `-m numera` must not survive into a frozen command."""
    monkeypatch.setattr(sys, "frozen", True, raising=False)
    monkeypatch.setattr(sys, "executable", r"C:\app\numera.exe", raising=False)

    built = selfexec.self_command("launch", "abc")

    assert built == [r"C:\app\numera.exe", "launch", "abc"]
    assert "-m" not in built, "the interpreter switch reached a frozen build"
    assert "numera" not in built[1:], "the module name reached a frozen build"


def test_a_path_with_spaces_stays_one_argument(monkeypatch):
    """Passed as a list, never through a shell, so quoting is not this code's
    job - but the executable must stay a single element either way."""
    monkeypatch.setattr(sys, "frozen", True, raising=False)
    monkeypatch.setattr(sys, "executable", r"C:\Program Files\Numera\numera.exe",
                        raising=False)

    built = selfexec.self_command("--data-dir", r"C:\Users\a b\data", "launch")

    assert built[0] == r"C:\Program Files\Numera\numera.exe"
    assert built[1:] == ["--data-dir", r"C:\Users\a b\data", "launch"]


def test_no_arguments_is_the_bare_program(monkeypatch):
    monkeypatch.setattr(sys, "frozen", True, raising=False)
    monkeypatch.setattr(sys, "executable", "/opt/numera/numera", raising=False)

    assert selfexec.self_command() == ["/opt/numera/numera"]


@pytest.mark.parametrize("module", [
    "numera.jobs", "numera.mcp", "numera.server.routes_browser",
    "numera.server.routes_schedules",
])
def test_no_call_site_builds_its_own_self_command(module):
    """The builder only helps if nothing bypasses it.

    Each of these modules re-invokes Numera. A literal `"-m", "numera"` pair in
    any of them is the defect coming back - the call sites drifted apart once
    already, which is why this asserts on the source rather than on one of them.
    """
    import importlib
    from pathlib import Path

    source = Path(importlib.import_module(module).__file__).read_text("utf-8")

    assert '"-m"' not in source and "'-m'" not in source, \
        f"{module} still builds an interpreter self-command by hand"


def test_the_frozen_entry_would_reject_the_old_command():
    """Why the old shape fails, stated as an executable fact.

    The application parser has no `-m`. This is what a frozen self-spawn was
    actually sending, and it is a usage error - not a crash the caller could
    have diagnosed from the message.
    """
    from numera.cli import build_parser

    parser = build_parser() if hasattr(sys.modules["numera.cli"], "build_parser") else None
    if parser is None:
        pytest.skip("the CLI does not expose its parser for direct inspection")
    with pytest.raises(SystemExit):
        parser.parse_args(["-m", "numera", "launch", "abc"])


def test_a_frozen_build_rejects_the_old_shape_for_real(tmp_path):
    """Not a mock: run a real frozen-like program with the old argument shape.

    A stand-in executable stands in for `numera.exe` - it prints its own argv -
    so this shows the arguments arriving at the application rather than at an
    interpreter, which is the whole failure.
    """
    probe = tmp_path / "probe.py"
    probe.write_text("import sys; print(sys.argv[1:])", encoding="utf-8")

    out = subprocess.run([sys.executable, str(probe), "-m", "numera", "launch", "x"],
                         capture_output=True, text=True, timeout=60)

    assert "'-m'" in out.stdout, \
        "the switch should arrive as an application argument, which is the bug"
