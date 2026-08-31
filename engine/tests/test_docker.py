"""The container definition has to stay true to the code it packages.

Docker is not installed on every machine that runs this suite, so these assert
the things that go wrong *silently* and are checkable without a daemon: a COPY
source that no longer exists, a browser image that has drifted from the
Playwright client, a dashboard copied somewhere the engine does not look, and a
compose file that would publish this API on every interface.

A real `docker build` is the release gate, not this. What this catches is the
edit that makes the build wrong months before anyone runs it.
"""
import re
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[2]
DOCKERFILE = ROOT / "Dockerfile"
COMPOSE = ROOT / "docker-compose.yml"

pytestmark = pytest.mark.skipif(not DOCKERFILE.is_file(), reason="no Dockerfile")


def dockerfile() -> str:
    return DOCKERFILE.read_text(encoding="utf-8")


def test_every_copy_source_exists():
    """A renamed directory turns into a build that fails on someone else's
    machine, long after the rename."""
    missing = [m.group(1) for m in re.finditer(r"^COPY (?!--from)(\S+)", dockerfile(), re.M)
               if not (ROOT / m.group(1).rstrip("/")).exists()]
    assert not missing, f"Dockerfile copies paths that do not exist: {missing}"


def test_the_browser_image_matches_the_playwright_client():
    """A browser build newer than its client fails at launch with a protocol
    error, which reads as a broken profile rather than a version mismatch."""
    import importlib.metadata as md

    try:
        pinned = md.version("playwright")
    except md.PackageNotFoundError:
        pytest.skip("playwright is not installed in this environment")
    assert f"v{pinned}-noble" in dockerfile(), (
        f"the image is not pinned to playwright {pinned}; "
        "update the FROM line and engine/pyproject.toml together")


def test_the_dashboard_lands_where_the_engine_serves_it():
    """`spa.py` looks for `numera/web`. Copying the build anywhere else produces
    a container whose API works and whose every page 404s - which has already
    happened once in this project, from a different cause."""
    spa = (ROOT / "engine" / "numera" / "server" / "spa.py").read_text(encoding="utf-8")
    assert 'parents[1] / "web"' in spa, "spa.py no longer serves numera/web"
    assert "numera/web/" in dockerfile(), "the Dockerfile copies the dashboard elsewhere"


def test_the_healthcheck_is_valid_python():
    body = dockerfile().split('CMD python -c "', 1)[1].split('" || exit 1')[0]
    compile(body.replace("\\\n", ""), "<healthcheck>", "exec")


def test_the_container_does_not_run_as_root():
    """Every file it writes into a mounted volume would otherwise land as root
    on the host."""
    assert re.search(r"^USER (?!root)", dockerfile(), re.M), "no non-root USER"


def test_shared_memory_is_raised_for_chromium():
    """Chromium in a container with the default 64 MB /dev/shm dies with an
    opaque crash rather than an error message."""
    assert COMPOSE.is_file()
    assert "shm_size" in COMPOSE.read_text(encoding="utf-8")


def test_compose_publishes_to_loopback_only():
    """`8787:8787` binds every interface on the host. This API creates profiles,
    launches browsers and exports cookies; a token is a second lock, not a
    reason to expose it."""
    yaml = pytest.importorskip("yaml")
    c = yaml.safe_load(COMPOSE.read_text(encoding="utf-8"))
    ports = c["services"]["numera"]["ports"]
    assert ports and all(str(p).startswith("127.0.0.1:") for p in ports), ports


def test_compose_refuses_to_start_without_a_token():
    """Inside a container 0.0.0.0 is the only reachable address, and the engine
    refuses that unauthenticated. Compose should fail loudly at `up`, not leave
    the operator reading a stack trace from the container."""
    text = COMPOSE.read_text(encoding="utf-8")
    assert "NUMERA_API_TOKEN" in text
    assert ":?" in text, "the token is not a required variable"


def test_dockerignore_keeps_local_only_files_out_of_the_image():
    """The build context is sent to the daemon and baked into image history, so
    a miss here is not a slow build - it is a secret in a layer."""
    ignored = set((ROOT / ".dockerignore").read_text(encoding="utf-8").split("\n"))
    for must in (".git", ".ai", ".ignoreme", "secrets.md", ".venv", "graphify-out"):
        assert must in ignored, f".dockerignore does not exclude {must}"
