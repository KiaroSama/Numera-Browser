"""A green required engine job must contain real, selected, unskipped tests."""

from contextlib import contextmanager
import fnmatch
import importlib.util
import os
from pathlib import Path
import re
import subprocess
import textwrap
from types import SimpleNamespace
import xml.etree.ElementTree as ET
import zipfile

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


PRODUCT_ARTIFACTS = (
    "numera-server-wheel", "numera-desktop-Linux", "numera-desktop-macOS",
    "numera-desktop-Windows",
)
DIAGNOSTIC_ARTIFACTS = (
    "live-engine-ubuntu-latest-playwright", "future-diagnostics",
    "numera-desktop-debug",
)


def _release_steps():
    release = _jobs(ROOT / ".github/workflows/build-desktop.yml")["release"]
    return release.split("\n      - ")[1:]


def test_release_downloads_and_publishes_only_product_artifacts():
    steps = _release_steps()
    downloads = [step for step in steps if "uses: actions/download-artifact@" in step]
    selected = []
    for step in downloads:
        inputs = dict(re.findall(r"(?m)^          ([\w-]+): (.+)$", step))
        assert inputs.get("name") in PRODUCT_ARTIFACTS
        assert inputs["path"] == "artifacts/" + inputs["name"]
        selected.append(inputs["name"])
    assert sorted(selected) == sorted(PRODUCT_ARTIFACTS)

    publish = next(step for step in steps if "uses: softprops/action-gh-release@" in step)
    files = re.search(r"files: (.+?)\n          fail_on_unmatched_files:", publish, re.S)[1]
    patterns = [line.strip() for line in files.strip().strip('"').removeprefix("|").splitlines()
                if line.strip()]
    candidates = [name + ".zip" for name in PRODUCT_ARTIFACTS + DIAGNOSTIC_ARTIFACTS]
    selected = {name for name in candidates if any(fnmatch.fnmatchcase(name, pattern)
                                                  for pattern in patterns)}
    assert selected == {name + ".zip" for name in PRODUCT_ARTIFACTS}
    assert set(patterns) == selected  # Publication must also reject future, unknown artifacts.
    assert "fail_on_unmatched_files: true" in publish


@pytest.mark.skipif(os.name == "nt", reason="The release packaging step runs on Ubuntu")
@pytest.mark.parametrize("missing", [None, *PRODUCT_ARTIFACTS])
def test_release_packaging_excludes_diagnostics_and_requires_every_product(tmp_path, missing):
    for name in PRODUCT_ARTIFACTS + DIAGNOSTIC_ARTIFACTS:
        if name != missing:
            artifact = tmp_path / "artifacts" / name
            artifact.mkdir(parents=True)
            (artifact / "payload.txt").write_text(name, encoding="utf-8")
    package = next(step for step in _release_steps() if "name: Zip each platform folder" in step)
    script = textwrap.dedent(package.split("run: |\n", 1)[1])
    result = subprocess.run(["bash", "-e", "-o", "pipefail", "-c", script], cwd=tmp_path,
                            stdin=subprocess.DEVNULL, capture_output=True, encoding="utf-8",
                            timeout=10)
    if missing:
        assert result.returncode != 0, result.stdout
        assert missing in result.stderr
    else:
        assert result.returncode == 0, result.stderr
        assert {p.name for p in tmp_path.glob("*.zip")} == {n + ".zip" for n in PRODUCT_ARTIFACTS}
        for name in PRODUCT_ARTIFACTS:
            with zipfile.ZipFile(tmp_path / (name + ".zip")) as archive:
                assert archive.read(name + "/payload.txt").decode("utf-8") == name
