"""A CLI command is one request, and a slow route commits onto the current record.

Two families of the same defect.

**Compound CLI commands** applied their fields one commit at a time, so a
failure part-way through returned an error over changes that had already
landed: `config` wrote the default engine before validating the extensions,
`create --engine` wrote the global default before saving the profile, and
`apply` / `bulk create` / the vendor importer saved in a loop.

**Slow routes** read a record, opened a real browser for tens of seconds, and
then wrote that stale object back - reverting anything the user changed while
they waited.
"""
import json

import pytest

from _rendezvous import Rendezvous, finish, start

from numera import cli, generate, paths
from numera.models import Profile
from numera.repo import repo
from numera.store import ProfileStore

pytest.importorskip("fastapi")

from fastapi.testclient import TestClient        # noqa: E402

from numera.server import create_app             # noqa: E402


def client(tmp_path):
    return TestClient(create_app(data_dir=str(tmp_path), start_scheduler=False))


def config_of(tmp_path) -> dict:
    path = tmp_path / "config.json"
    return json.loads(path.read_text(encoding="utf-8")) if path.exists() else {}


def unpacked(root, name="ext"):
    d = root / name
    d.mkdir(parents=True, exist_ok=True)
    (d / "manifest.json").write_text(
        '{"manifest_version": 3, "name": "x", "version": "1.0"}', encoding="utf-8")
    return d


# --- item 5: the CLI --------------------------------------------------------

def test_an_invalid_extension_leaves_the_earlier_config_fields_alone(tmp_path):
    """`--headless-launch on` used to be committed before the extension was
    even looked at, so the command failed *and* changed something."""
    root = tmp_path / "data"
    ProfileStore(root)
    before = config_of(root)

    code = cli.main(["--data-dir", str(root), "config",
                     "--headless-launch", "on",
                     "--default-extension", str(tmp_path / "does-not-exist")])
    assert code == 1
    assert config_of(root).get("headless_launch") == before.get("headless_launch")
    assert "default_extensions" not in config_of(root) or \
        config_of(root)["default_extensions"] == before.get("default_extensions", [])


def test_a_valid_compound_config_still_applies_every_field(tmp_path):
    root = tmp_path / "data"
    ext = unpacked(tmp_path)
    assert cli.main(["--data-dir", str(root), "config", "--headless-launch", "on",
                     "--default-extension", str(ext), "--default-engine", "playwright"]) == 0
    cfg = config_of(root)
    assert cfg["headless_launch"] is True
    assert cfg["default_engine"] == "playwright"
    assert cfg["default_extensions"] == [str(ext)]


def test_a_failed_create_does_not_move_the_default_engine(tmp_path, monkeypatch):
    root = tmp_path / "data"
    store = ProfileStore(root)
    store.set_default_engine("playwright")

    real = paths.write_json
    calls = {"n": 0}

    def fail_first(path, data):
        calls["n"] += 1
        if calls["n"] == 1:
            raise OSError("no space left on device")
        return real(path, data)

    monkeypatch.setattr(paths, "write_json", fail_first)
    #  turns an OSError into exit 1 with a message - that is the
    # command's contract - so the assertion is on the outcome, not the raise.
    assert cli.main(["--data-dir", str(root), "create", "boom", "--engine", "camoufox"]) == 1
    monkeypatch.undo()

    assert config_of(root)["default_engine"] == "playwright", \
        "a create that failed still changed the global default"
    assert ProfileStore(root).find_by_name("boom") is None


def test_a_failing_bulk_create_writes_nothing(tmp_path, monkeypatch):
    root = tmp_path / "data"
    ProfileStore(root)

    real = paths.write_json
    calls = {"n": 0}

    def fail_late(path, data):
        calls["n"] += 1
        if calls["n"] == 5:
            raise OSError("no space left on device")
        return real(path, data)

    monkeypatch.setattr(paths, "write_json", fail_late)
    assert cli.main(["--data-dir", str(root), "bulk-create", "--count", "4",
                     "--prefix", "b"]) == 1
    monkeypatch.undo()

    assert ProfileStore(root).list() == [], "a partial bulk create landed"


def test_a_failing_import_writes_nothing(tmp_path, monkeypatch):
    from numera import importers

    root = tmp_path / "data"
    ProfileStore(root)
    export = tmp_path / "export.json"
    export.write_text(json.dumps({"profiles": [
        {"name": "one", "os": "Windows"}, {"name": "two", "os": "Windows"},
        {"name": "three", "os": "Windows"}]}), encoding="utf-8")

    real = paths.write_json
    calls = {"n": 0}

    def fail_late(path, data):
        calls["n"] += 1
        if calls["n"] == 4:
            raise OSError("no space left on device")
        return real(path, data)

    monkeypatch.setattr(paths, "write_json", fail_late)
    with pytest.raises(OSError):
        importers.import_file(export, ProfileStore(root))
    monkeypatch.undo()

    assert ProfileStore(root).list() == [], "a partial vendor import landed"


# --- item 3: slow routes commit onto the current record ---------------------

def test_a_metadata_stamp_does_not_revert_a_concurrent_edit(tmp_path):
    """`_last_active` and `_trust` are written after a browser session ends.
    Saving the whole record back reverted every edit made while it was open."""
    c = client(tmp_path)
    pid = c.post("/api/profiles", json={"name": "stamped", "notes": ""}).json()["id"]

    from numera.server.stores import DashboardStore
    dash = DashboardStore(tmp_path, ProfileStore(tmp_path))
    stale = dash.get(pid)                       # the snapshot a slow route holds

    c.put(f"/api/profiles/{pid}", json={**stale, "notes": "typed while it ran"})

    # What the route does now: patch only the field it owns.
    dash.patch(pid, lambda cur: ({**cur, "_trust": {"grade": "A"}}, None))

    final = dash.get(pid)
    assert final["notes"] == "typed while it ran", "the stamp reverted a user edit"
    assert final["_trust"]["grade"] == "A"


# --- item 4: proxy assignment is one transaction ----------------------------

def test_a_failing_assignment_assigns_nobody(tmp_path, monkeypatch):
    c = client(tmp_path)
    ids = c.post("/api/profiles/batch",
                 json={"profiles": [{"name": f"p{i}"} for i in range(4)]}).json()["ids"]
    c.post("/api/proxies", json={"text": "1.1.1.1:8080\n2.2.2.2:8080"})

    real = paths.write_json
    calls = {"n": 0}

    def fail_late(path, data):
        calls["n"] += 1
        if calls["n"] == 5:
            raise OSError("no space left on device")
        return real(path, data)

    monkeypatch.setattr(paths, "write_json", fail_late)
    monkeypatch.setattr("numera.server.stores.write_json", fail_late)
    with pytest.raises(OSError):
        c.post("/api/proxies/assign", json={"profileIds": ids})
    monkeypatch.undo()

    after = c.get("/api/profiles").json()
    assert all(not p.get("proxy") for p in after), \
        "an unknown prefix of the selection was assigned before the failure"


def test_assignment_survives_a_concurrent_note_edit(tmp_path):
    c = client(tmp_path)
    pid = c.post("/api/profiles", json={"name": "target", "notes": ""}).json()["id"]
    c.post("/api/proxies", json={"text": "1.1.1.1:8080"})

    from numera.server.stores import DashboardStore
    dash = DashboardStore(tmp_path, ProfileStore(tmp_path))
    dash.patch(pid, lambda cur: ({**cur, "notes": "edited first"}, None))

    c.post("/api/proxies/assign", json={"profileIds": [pid]})

    final = dash.get(pid)
    assert final["proxy"]["host"] == "1.1.1.1"
    assert final["notes"] == "edited first", "assignment wrote a stale snapshot back"


# --- item 7: warm-up validation and the due claim ---------------------------

@pytest.mark.parametrize("minutes", [float("inf"), float("nan"), -1, 0, 10 ** 6, "x"])
def test_an_impossible_warmup_duration_is_refused_everywhere(tmp_path, minutes):
    from numera.warmup import WarmupInvalid, validate_warmup

    with pytest.raises(WarmupInvalid):
        validate_warmup(minutes)

    c = client(tmp_path)
    pid = c.post("/api/profiles", json={"name": "w"}).json()["id"]
    r = c.post(f"/api/profiles/{pid}/warmup", json={"minutes": str(minutes)})
    assert r.status_code == 400, r.text


def test_an_unknown_preset_is_refused_before_a_browser_starts(tmp_path):
    from numera.warmup import WarmupInvalid, validate_warmup

    with pytest.raises(WarmupInvalid):
        validate_warmup(5, preset="not-a-preset")

    c = client(tmp_path)
    pid = c.post("/api/profiles", json={"name": "w2"}).json()["id"]
    assert c.post(f"/api/profiles/{pid}/warmup",
                  json={"minutes": 5, "preset": "not-a-preset"}).status_code == 400


def test_a_schedule_cannot_store_a_duration_a_run_would_refuse(tmp_path):
    from numera.scheduler import ScheduleStore

    sched = ScheduleStore(tmp_path)
    with pytest.raises(ValueError):
        sched.add(profile_id="p", every_hours=1, minutes=float("inf"))
    with pytest.raises(ValueError):
        sched.add(profile_id="p", every_hours=1, minutes=5, preset="nope")


def test_two_processes_racing_one_due_occurrence_produce_one_claim(tmp_path):
    from numera.scheduler import ScheduleStore

    sched = ScheduleStore(tmp_path)
    row = sched.add(profile_id="p1", every_hours=1, minutes=5)

    body = """
from numera.scheduler import ScheduleStore
sched = ScheduleStore(ROOT)
wait()
print("WON" if sched.claim_due({sid!r}, WHO) else "LOST")
say("done")
""".format(sid=row["id"])

    bar = Rendezvous()
    procs = {}
    try:
        for who in ("a", "b"):
            procs[who] = start(body, tmp_path, who, bar.port)
        for who in procs:
            bar.release(who)
        for who in procs:
            bar.expect(who, "done")
        results = finish(procs)
    finally:
        bar.close()
        for pr in procs.values():
            if pr.poll() is None:
                pr.kill()
    assert all(r["code"] == 0 for r in results.values()), results
    won = [w for w, r in results.items() if r["out"] == "WON"]
    assert len(won) == 1, f"exactly one worker may own a due occurrence: {results}"


# --- item 6: first run is a migration, and migrations are locked ------------

def test_two_first_runs_import_once(tmp_path):
    from numera import firstrun

    legacy = tmp_path / "legacy"
    (legacy / "profiles").mkdir(parents=True)
    (legacy / "profiles" / "aaaaaaaaaaaa.json").write_text(
        json.dumps(Profile(name="old", fingerprint=generate(seed=1),
                           id="aaaaaaaaaaaa").to_dict()), encoding="utf-8")
    dest = tmp_path / "dest"

    body = """
from pathlib import Path
from numera import firstrun
firstrun.find_legacy = lambda destination: Path({legacy!r})
wait()
res = firstrun.run_if_needed(Path(ROOT))
print("IMPORTED" if res and res.get("copied") else "NOTHING")
say("done")
""".format(legacy=str(legacy))

    bar = Rendezvous()
    procs = {}
    try:
        for who in ("one", "two"):
            procs[who] = start(body, dest, who, bar.port)
        for who in procs:
            bar.release(who)
        for who in procs:
            bar.expect(who, "done")
        results = finish(procs)
    finally:
        bar.close()
        for pr in procs.values():
            if pr.poll() is None:
                pr.kill()
    assert all(r["code"] == 0 for r in results.values()), results
    imported = [w for w, r in results.items() if r["out"] == "IMPORTED"]
    assert len(imported) == 1, f"both processes ran the import: {results}"
    assert len(list((dest / "profiles").glob("*.json"))) == 1
