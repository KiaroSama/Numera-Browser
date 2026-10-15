"""Prepare a required live engine and reject incomplete JUnit evidence.

CI owns installation and command time limits; this helper does not run pytest.
Cloak uses the explicitly pinned, official key-free v146 build
for internal CI only. It is never bundled or uploaded as a release artifact.
"""

from __future__ import annotations

import argparse
import importlib
from importlib.metadata import version
import logging
import os
from pathlib import Path
import sys
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "engine"))

ENGINES = {"playwright": "playwright", "patchright": "patchright",
           "camoufox": "camoufox", "cloak": "cloakbrowser"}
CLOAK_VERSION = "146.0.7680.177.5"
TREE_TESTS = (
    "test_every_installed_engine_gets_a_tree_of_its_own",
    "test_hard_killed_owner_releases_only_an_empty_real_browser_tree",
)


def prepare(engine: str, *, fetch: bool = False) -> Path:
    """Fail on a missing package/binary; only explicit Cloak fetch can download."""
    package = ENGINES[engine]
    print(f"Required engine: {engine}; {package}=={version(package)}", flush=True)
    if engine in {"playwright", "patchright"}:
        api = importlib.import_module(f"{package}.sync_api")
        with api.sync_playwright() as driver:
            binary = Path(driver.chromium.executable_path)
    elif engine == "camoufox":
        from camoufox.pkgman import camoufox_path, launch_path
        binary = Path(launch_path(camoufox_path(download_if_missing=False)))
    else:
        from cloakbrowser.config import get_binary_path
        from cloakbrowser.license import resolve_license_key

        if resolve_license_key() or any(os.environ.get(key) for key in (
                "CLOAKBROWSER_BINARY_PATH", "CLOAKBROWSER_DOWNLOAD_URL")):
            raise RuntimeError("The Cloak CI contract requires the official key-free "
                               "pinned build; license or binary/download overrides are set")
        if os.environ.get("CLOAKBROWSER_VERSION") != CLOAK_VERSION:
            raise RuntimeError(f"Set CLOAKBROWSER_VERSION={CLOAK_VERSION} explicitly")
        binary = get_binary_path(CLOAK_VERSION)
        if fetch:
            from cloakbrowser.download import ensure_binary
            obtained = Path(ensure_binary(browser_version=CLOAK_VERSION))
            if obtained.resolve() != binary.resolve():
                raise RuntimeError("Cloak resolved a different binary than the CI pin")
        print(f"Cloak binary contract: {CLOAK_VERSION} (official key-free build)", flush=True)
    if not binary.is_file() or (os.name != "nt" and not os.access(binary, os.X_OK)):
        raise RuntimeError(f"Required {engine} browser binary is missing or not executable: {binary}")
    print(f"Required browser binary: {binary}", flush=True)
    return binary


def export_native(engine: str, binary: Path) -> None:
    """Pass only the verified existing Cloak executable to subsequent CI steps."""
    if engine != 'cloak':
        return
    target = os.environ.get('GITHUB_ENV')
    if not target:
        raise RuntimeError('Native browser handoff requires the GitHub environment file')
    path = str(binary.resolve(strict=True))
    if any(ord(char) < 32 or ord(char) == 127 for char in path):
        raise RuntimeError('Native browser path contains invalid environment characters')
    with Path(target).open('a', encoding='utf-8', newline='\n') as stream:
        stream.write(f'CLOAKBROWSER_BINARY_PATH={path}\n')


def check_report(path: Path, engine: str) -> None:
    """Success means both selected engine tests ran and passed without skips."""
    cases = list(ET.parse(path).iter("testcase"))
    expected = {f"{test}[{engine}]" for test in TREE_TESTS}
    if len(cases) != len(expected) or {case.get("name") for case in cases} != expected:
        raise RuntimeError(f"Required {engine} session-tree coverage is incomplete or unexpected")
    if any(case.find(outcome) is not None for case in cases
           for outcome in ("skipped", "failure", "error")):
        raise RuntimeError(f"Required {engine} live tests skipped or failed")
    print(f"Required {engine} live coverage: {len(cases)} passed, 0 skipped", flush=True)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("prepare", "report"))
    parser.add_argument("--engine", choices=tuple(ENGINES), required=True)
    parser.add_argument("--fetch", action="store_true", help="fetch the pinned Cloak binary")
    parser.add_argument("--report", type=Path)
    parser.add_argument("--export-native", action="store_true",
                        help="hand the verified Cloak executable to later GitHub CI steps")
    args = parser.parse_args()
    if args.command == "report" and args.report is None:
        parser.error("report requires --report PATH")
    if args.fetch and (args.command != "prepare" or args.engine != "cloak"):
        parser.error("--fetch is only valid for prepare --engine cloak")
    if args.export_native and args.command != "prepare":
        parser.error("--export-native is only valid for prepare")

    from numera import logs
    logs.setup(ROOT / "build" / "live-engine")
    log = logs.get("live-engine")
    log.info("command=%s engine=%s fetch=%s", args.command, args.engine, args.fetch)
    try:
        if args.command == "prepare":
            binary = prepare(args.engine, fetch=args.fetch)
            if args.export_native:
                export_native(args.engine, binary)
        else:
            check_report(args.report, args.engine)
    except Exception as exc:
        log.exception("Live engine contract failed")
        print(f"Live engine contract failed: {exc}", file=sys.stderr)
        return 1
    else:
        log.info("Live engine contract passed")
        return 0
    finally:
        logging.shutdown()


if __name__ == "__main__":
    raise SystemExit(main())
