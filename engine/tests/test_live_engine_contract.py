"""A green required engine job must contain real, selected, unskipped tests."""

from contextlib import contextmanager
import importlib.util
from pathlib import Path
import re
from types import SimpleNamespace
import xml.etree.ElementTree as ET

import pytest

from numera import drivers

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("live_engine", ROOT / "scripts/live_engine.py")
live = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(live)


@pytest.mark.parametrize("damage", [None, "missing", "wrong-engine", "duplicate",
                                    "skipped", "failure", "error"])
def test_report_requires_both_selected_tests_to_pass(tmp_path, damage):
    suite = ET.Element("testsuite")
    for name in live.TREE_TESTS:
        ET.SubElement(suite, "testcase", name=f"{name}[patchright]")
    if damage == "missing":
        suite.remove(suite[0])
    elif damage == "wrong-engine":
        suite[0].set("name", f"{live.TREE_TESTS[0]}[playwright]")
    elif damage == "duplicate":
        suite[1].set("name", suite[0].get("name"))
    elif damage:
        ET.SubElement(suite[0], damage)
    report = tmp_path / "results.xml"
    ET.ElementTree(suite).write(report, encoding="utf-8", xml_declaration=True)
    if damage:
        with pytest.raises(RuntimeError):
            live.check_report(report, "patchright")
    else:
        live.check_report(report, "patchright")


def test_missing_report_and_browser_are_failures(tmp_path, monkeypatch):
    with pytest.raises(FileNotFoundError):
        live.check_report(tmp_path / "missing.xml", "playwright")

    @contextmanager
    def driver():
        yield SimpleNamespace(chromium=SimpleNamespace(executable_path=tmp_path / "missing"))

    monkeypatch.setattr(live, "version", lambda package: "test")
    monkeypatch.setattr(live.importlib, "import_module",
                        lambda module: SimpleNamespace(sync_playwright=driver))
    with pytest.raises(RuntimeError, match="browser binary is missing"):
        live.prepare("patchright")


def _jobs(path):
    text = path.read_text(encoding="utf-8").split("\njobs:\n", 1)[1]
    chunks = re.split(r"(?m)^  ([a-z][a-z0-9-]*):\n", text)
    return dict(zip(chunks[1::2], chunks[2::2]))


def test_every_supported_engine_is_required_on_both_platforms():
    jobs = _jobs(ROOT / ".github/workflows/ci.yml")
    matrix = jobs["engine-live"]
    engines = re.search(r"engine: \[([^\]]+)\]", matrix).group(1).split(", ")
    assert set(engines) == set(drivers.names()) == set(live.ENGINES)
    assert "os: [ubuntu-latest, windows-latest]" in matrix
    pins = re.findall(r"- engine: (\w+)\n\s+package: ([^\n]+)", matrix)
    assert {name for name, _ in pins} == set(engines)
    assert all(re.fullmatch(r"[a-z]+(?:\[geoip\])?==\d+\.\d+\.\d+", pin)
               for _, pin in pins)
    assert "NUMERA_TEST_ENGINE: ${{ matrix.engine }}" in matrix
    assert 'CLOAKBROWSER_VERSION: "' + live.CLOAK_VERSION + '"' in matrix
    assert "prepare --engine ${{ matrix.engine }}" in matrix
    assert "test_session_tree_live.py" in matrix
    assert "--strict-config" in matrix and "--timeout=120" in matrix
    assert "report --engine ${{ matrix.engine }}" in matrix
    assert "continue-on-error" not in matrix
    assert "max-parallel: 2" in matrix and "timeout-minutes:" in matrix
    for name in ("engine", "engine-windows"):
        assert "--ignore-glob=tests/test_*_live.py" in jobs[name]
    required = jobs["required"]
    claimed = re.search(r"needs: \[([^\]]+)\]", required).group(1).split(", ")
    assert set(claimed) == set(jobs) - {"required"}
    assert "if: always()" in required
    assert 'job["result"] != "success"' in required
    build = _jobs(ROOT / ".github/workflows/build-desktop.yml")
    assert "uses: ./.github/workflows/ci.yml" in build["verify-release"]
    assert "needs: [verify-release," in build["release"]
