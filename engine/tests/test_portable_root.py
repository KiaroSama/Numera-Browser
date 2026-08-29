"""The portable data root, the first-run import, and log redaction.

This code moves a user's real profiles, proxy pool and browser sessions. The
failure that matters is not a crash — it is silently importing nothing, or
overwriting something, and only being noticed once the data is gone. So the
tests here assert the destructive edges, not the happy path alone.
"""

import json
import logging

import pytest

from numera import firstrun, logs, paths


# ---- resolution order -------------------------------------------------

def test_an_explicit_dir_beats_everything(tmp_path, monkeypatch):
    monkeypatch.setenv("NUMERA_DATA_DIR", str(tmp_path / "from-env"))
    assert paths.resolve_data_dir(tmp_path / "explicit") == tmp_path / "explicit"


def test_the_env_var_is_used_when_there_is_no_argument(tmp_path, monkeypatch):
    monkeypatch.setenv("NUMERA_DATA_DIR", str(tmp_path / "from-env"))
    assert paths.resolve_data_dir() == tmp_path / "from-env"


def test_a_writable_app_dir_is_preferred_so_the_app_stays_portable(tmp_path, monkeypatch):
    monkeypatch.delenv("NUMERA_DATA_DIR", raising=False)
    monkeypatch.setattr(paths, "app_dir", lambda: tmp_path / "app")
    assert paths.resolve_data_dir() == tmp_path / "app" / "data"


def test_an_unwritable_app_dir_falls_back_to_home(tmp_path, monkeypatch):
    """Unpacked into Program Files, a data dir beside the binary is read-only.

    Falling back here is what stops every later write failing far from the cause."""
    monkeypatch.delenv("NUMERA_DATA_DIR", raising=False)
    monkeypatch.setattr(paths, "app_dir", lambda: tmp_path / "app")
    monkeypatch.setattr(paths, "_is_writable", lambda p: False)
    monkeypatch.setattr(paths.Path, "home", classmethod(lambda cls: tmp_path / "home"))
    assert paths.resolve_data_dir() == tmp_path / "home" / ".numera"


def test_the_pre_rename_directory_is_used_when_it_is_the_only_one(tmp_path, monkeypatch):
    monkeypatch.delenv("NUMERA_DATA_DIR", raising=False)
    monkeypatch.setattr(paths, "app_dir", lambda: tmp_path / "app")
    monkeypatch.setattr(paths, "_is_writable", lambda p: False)
    home = tmp_path / "home"
    (home / ".persona").mkdir(parents=True)
    monkeypatch.setattr(paths.Path, "home", classmethod(lambda cls: home))
    assert paths.resolve_data_dir() == home / ".persona"


# ---- the first-run import --------------------------------------------

@pytest.fixture
def legacy(tmp_path):
    """An old install: two profiles, a proxy pool, config and a browser session."""
    src = tmp_path / "old"
    (src / "profiles").mkdir(parents=True)
    (src / "user-data" / "abc123").mkdir(parents=True)
    (src / "profiles" / "a.json").write_text('{"id": "a"}', encoding="utf-8")
    (src / "profiles" / "b.json").write_text('{"id": "b"}', encoding="utf-8")
    (src / "user-data" / "abc123" / "Cookies").write_text("session", encoding="utf-8")
    (src / "proxies.json").write_text(json.dumps([{"host": "h", "port": "1"}]), encoding="utf-8")
    (src / "config.json").write_text('{"default_engine": "camoufox"}', encoding="utf-8")
    return src


def test_import_carries_profiles_pool_config_and_sessions(tmp_path, legacy):
    dst = tmp_path / "new"
    result = firstrun.import_from(legacy, dst)
    assert result["profiles"] == 2
    assert (dst / "profiles" / "a.json").exists()
    assert (dst / "proxies.json").exists()
    assert (dst / "config.json").exists()
    # Sessions matter: profiles without them come back logged out, which looks
    # exactly like data loss to the person using them.
    assert (dst / "user-data" / "abc123" / "Cookies").read_text(encoding="utf-8") == "session"


def test_import_never_overwrites_the_destination(tmp_path, legacy):
    dst = tmp_path / "new"
    (dst / "profiles").mkdir(parents=True)
    (dst / "profiles" / "a.json").write_text('{"id": "MINE"}', encoding="utf-8")
    result = firstrun.import_from(legacy, dst)
    assert "profiles" in result["skipped"]
    assert json.loads((dst / "profiles" / "a.json").read_text(encoding="utf-8"))["id"] == "MINE"


def test_the_source_is_left_untouched(tmp_path, legacy):
    """Copy, never move — going back to an older build must still find the data."""
    firstrun.import_from(legacy, tmp_path / "new")
    assert (legacy / "profiles" / "a.json").exists()
    assert (legacy / "proxies.json").exists()


def test_it_runs_once_and_then_stops(tmp_path, legacy, monkeypatch):
    dst = tmp_path / "new"
    monkeypatch.setattr(firstrun, "legacy_dirs", lambda: [legacy])
    assert firstrun.run_if_needed(dst) is not None
    assert firstrun.run_if_needed(dst) is None      # marker written
    assert (dst / firstrun.MARKER).exists()


def test_a_marker_is_written_even_with_nothing_to_import(tmp_path, monkeypatch):
    dst = tmp_path / "new"
    monkeypatch.setattr(firstrun, "legacy_dirs", lambda: [tmp_path / "nope"])
    assert firstrun.run_if_needed(dst) is None
    assert (dst / firstrun.MARKER).exists()         # don't rescan on every start


def test_importing_from_the_destination_itself_is_a_no_op(tmp_path, legacy, monkeypatch):
    monkeypatch.setattr(firstrun, "legacy_dirs", lambda: [legacy])
    assert firstrun.find_legacy(legacy) is None


# ---- log redaction ----------------------------------------------------

@pytest.mark.parametrize("raw, secret", [
    ("connecting to http://alice:hunter2@gw.example:3128", "hunter2"),
    ("proxy pass=s3cr3t-value applied", "s3cr3t-value"),
    ('{"password": "topsecret"}', "topsecret"),
    ("Authorization: Bearer abcdef0123456789", "abcdef0123456789"),
    ("api_key: sk-live-9f8e7d6c5b4a", "sk-live-9f8e7d6c5b4a"),
])
def test_credentials_never_reach_the_log(raw, secret):
    out = logs.redact(raw)
    assert secret not in out
    assert "[REDACTED]" in out


def test_redaction_leaves_ordinary_text_alone():
    line = "launched profile a3bcf1a93b22 with engine camoufox on 127.0.0.1:8787"
    assert logs.redact(line) == line


@pytest.fixture
def fresh_logger():
    """`logs.setup` is deliberately idempotent per process, so a handler left by
    one test would be returned to the next. Isolate them."""
    logger = logs.get()
    saved = list(logger.handlers)
    logger.handlers.clear()
    yield
    for h in logger.handlers:
        h.close()
    logger.handlers[:] = saved


def test_redaction_is_applied_by_the_handler_not_the_call_site(tmp_path, fresh_logger):
    """A library logging under its own name must not be able to bypass it."""
    path = logs.setup(tmp_path, verbose=True)
    assert path is not None and path.exists()
    logging.getLogger("numera.someplugin").warning(
        "upstream http://bob:leakme@proxy.example:8080 refused")
    for h in logs.get().handlers:
        h.flush()
    text = path.read_text(encoding="utf-8")
    assert "leakme" not in text
    assert "[REDACTED]" in text


def test_redaction_does_not_break_numeric_format_args(tmp_path, fresh_logger):
    """Regression: the filter used to stringify every arg, so a %d then raised
    TypeError and logging dropped the whole record. A redaction filter that
    silently deletes log lines is worse than the leak it exists to prevent."""
    path = logs.setup(tmp_path)
    assert path is not None
    logs.get("firstrun").info("imported from %s - %d profile(s), %.1f MB",
                              "/old/place", 3, 1.5)
    for h in logs.get().handlers:
        h.flush()
    text = path.read_text(encoding="utf-8")
    assert "3 profile(s)" in text, "the record was dropped instead of formatted"
    assert "1.5 MB" in text


def test_redaction_still_scrubs_string_args(tmp_path, fresh_logger):
    """The type fix must not have opened a hole: strings are still scrubbed."""
    path = logs.setup(tmp_path)
    logs.get("proxy").warning("upstream %s failed", "http://bob:hunter2@gw:3128")
    for h in logs.get().handlers:
        h.flush()
    text = path.read_text(encoding="utf-8")
    assert "hunter2" not in text
    assert "[REDACTED]" in text


def test_setup_reports_failure_instead_of_claiming_a_file(tmp_path, monkeypatch, capsys,
                                                          fresh_logger):
    """Never print a log path that does not exist."""
    monkeypatch.setattr(logs.Path, "mkdir",
                        lambda *a, **k: (_ for _ in ()).throw(OSError("denied")))
    assert logs.setup(tmp_path / "nope") is None
    assert "could not create" in capsys.readouterr().out
