"""Nothing here may leave half a change behind when it fails part-way.

Each of these injects the failure rather than waiting for one: a store that
raises mid-migration, a rename that cannot happen because the directory is in
use, a child process that finishes before its parent has finished recording it.
Every one of them left real damage before, and none of them is reachable by
normal use, so the injection is the only way to keep them fixed.
"""
import json
import os
import time

import pytest

from numera import generate, jobs
from numera.models import Profile, Proxy
from numera.scheduler import ScheduleStore
from numera.store import ProfileStore


def profile(name="p", password=None):
    px = Proxy(server="http://1.2.3.4:8080", username="u", password=password)
    return Profile(name=name, fingerprint=generate(seed=1),
                   proxy=px if password else None)


# --- vault activation is all or nothing -------------------------------------

def secret_of(tmp_path, pid):
    data = json.loads((tmp_path / "profiles" / f"{pid}.json").read_text(encoding="utf-8"))
    return ((data.get("proxy") or {}).get("password"))


def test_a_failed_migration_leaves_no_vault_and_no_half_encrypted_store(tmp_path, monkeypatch):
    """The descriptor used to be written first, so a failure part-way through
    left the vault switched on with plaintext secrets still in the profiles it
    had not reached - the user is told they are protected while some of them are
    not, which is worse than never turning it on."""
    pytest.importorskip("cryptography")
    store = ProfileStore(tmp_path)
    ids = [store.save(profile(f"p{i}", password=f"secret-{i}")).id for i in range(4)]
    before = {pid: secret_of(tmp_path, pid) for pid in ids}
    assert all(v == f"secret-{i}" for i, v in enumerate(before.values()))

    from numera import crypto
    real = crypto.encrypt
    calls = {"n": 0}

    def fail_on_the_third(*a, **k):
        calls["n"] += 1
        if calls["n"] == 3:
            raise OSError("disk full")
        return real(*a, **k)

    monkeypatch.setattr(crypto, "encrypt", fail_on_the_third)
    with pytest.raises(OSError):
        store.enable_vault("master")

    fresh = ProfileStore(tmp_path)
    assert not fresh.vault_enabled(), "the vault was switched on by a failed migration"
    for pid in ids:
        assert secret_of(tmp_path, pid) == before[pid], f"{pid} was left converted"


def test_a_successful_migration_converts_everything_and_then_switches_on(tmp_path):
    pytest.importorskip("cryptography")
    from numera import crypto

    store = ProfileStore(tmp_path)
    ids = [store.save(profile(f"p{i}", password=f"secret-{i}")).id for i in range(3)]
    plain = store.save(profile("no-secret")).id

    # Six, not three: `save()` writes both copies of a profile, and the
    # dashboard's `proxy.pass` holds the same secret the engine's
    # `proxy.password` does. Both are named in `vault.SITES` and both have to be
    # sealed, or an unlocked dashboard read would hand back a plaintext
    # credential the engine had already encrypted.
    assert store.enable_vault("master") == 6
    assert ProfileStore(tmp_path).vault_enabled()
    for pid in ids:
        assert crypto.is_encrypted(secret_of(tmp_path, pid)), f"{pid} stayed in the clear"
    assert secret_of(tmp_path, plain) is None


# --- the two copies of a profile move together ------------------------------

def test_a_failed_engine_write_does_not_leave_the_dashboard_copy_ahead(tmp_path, monkeypatch):
    """`_mirror` swallowed everything, so a write the engine refused still
    reported success and the two copies quietly described different profiles."""
    from numera.server.stores import DashboardStore

    engine = ProfileStore(tmp_path)
    dash = DashboardStore(tmp_path, engine_store=engine)
    saved = dash.save({"name": "before"})
    pid = saved["id"]

    # Inject where the engine copy is actually produced. The write goes through
    # one `write_batch` now rather than through `engine.save()`, so that both
    # copies roll back together - patching `save` would no longer intercept
    # anything and the test would pass without exercising the path. Failing
    # `_apply_secrets` is also the real-world case: it is what raises when the
    # vault is locked.
    def refuse(*_a, **_k):
        raise OSError("engine store is read-only")

    monkeypatch.setattr(engine, "_apply_secrets", refuse)
    with pytest.raises(OSError):
        dash.save({**saved, "name": "after"})

    on_disk = json.loads((tmp_path / "dashboard" / f"{pid}.json").read_text(encoding="utf-8"))
    assert on_disk["name"] == "before", "the dashboard copy moved on without the engine"


def test_a_profile_the_engine_cannot_model_is_refused_not_half_written(
        tmp_path, monkeypatch):
    """This test used to assert the opposite, and the opposite was the bug.

    The tolerance was argued as kindness - "refusing here would take the UI
    down for something the user cannot see or fix" - but what it actually
    produced was a dashboard record with no engine copy at all: the API
    answered 200, the GUI listed the profile, and `numera list` had never heard
    of it. That is the same half-written state every other test in this file
    exists to prevent, reached through the one door that was left open.

    What made the old behaviour defensible was the second half: the user could
    not see the reason. They can now. The API validates the whole dashboard
    shape before it gets here and answers 422 naming the field, and this
    refusal is what guarantees the two copies are written together or not at
    all.
    """
    from numera.server import stores as stores_mod
    from numera.server.stores import DashboardStore, UnconvertibleProfile

    dash = DashboardStore(tmp_path, engine_store=ProfileStore(tmp_path))
    monkeypatch.setattr(stores_mod, "to_engine_profile",
                        lambda d: (_ for _ in ()).throw(ValueError("unconvertible")))
    with pytest.raises(UnconvertibleProfile):
        dash.save({"name": "odd"})
    assert not list((tmp_path / "dashboard").glob("*.json")), \
        "the dashboard copy was written without an engine twin"


# --- restore keeps the old session until the new one is in place ------------

def backup_of(store, prof, tmp_path):
    ud = store.user_data_path(prof.id)
    ud.mkdir(parents=True, exist_ok=True)
    (ud / "Cookies").write_text("original session", encoding="utf-8")
    return store.backup(prof.id, tmp_path / "backup.zip")


def test_a_restore_that_cannot_finish_leaves_the_original_intact(tmp_path, monkeypatch):
    """It used to `rmtree(..., ignore_errors=True)` the live session and then
    move staging in. On Windows a running Chromium holds its files, so the
    delete failed silently - and `shutil.move` onto a directory that still
    exists moves the source *inside* it, leaving
    `user-data/<id>/numera-restore-xxxx/`. The profile had already been saved,
    so the store described a session it did not have."""
    import shutil

    store = ProfileStore(tmp_path)
    prof = store.save(profile("original"))
    archive = backup_of(store, prof, tmp_path)
    store.save(Profile(id=prof.id, name="renamed since", fingerprint=prof.fingerprint))
    (store.user_data_path(prof.id) / "Cookies").write_text("live session", encoding="utf-8")

    real_move = shutil.move
    monkeypatch.setattr(shutil, "move",
                        lambda *a, **k: (_ for _ in ()).throw(OSError("in use")))
    with pytest.raises(OSError):
        store.restore(archive)
    monkeypatch.setattr(shutil, "move", real_move)

    ud = store.user_data_path(prof.id)
    assert (ud / "Cookies").read_text(encoding="utf-8") == "live session", \
        "the live session was destroyed by a restore that could not finish"
    assert store.get(prof.id).name == "renamed since", "the metadata moved on anyway"
    leftovers = [d.name for d in ud.iterdir() if d.is_dir() and "restore" in d.name]
    assert not leftovers, f"staging was moved inside the old session: {leftovers}"
    assert not [d for d in ud.parent.iterdir() if ".replaced-" in d.name], \
        "the set-aside session was not put back"


def test_a_restore_that_succeeds_replaces_the_session_and_the_metadata(tmp_path):
    store = ProfileStore(tmp_path)
    prof = store.save(profile("original"))
    archive = backup_of(store, prof, tmp_path)
    store.save(Profile(id=prof.id, name="renamed since", fingerprint=prof.fingerprint))
    (store.user_data_path(prof.id) / "Cookies").write_text("live session", encoding="utf-8")
    (store.user_data_path(prof.id) / "Extra").write_text("not in the backup", encoding="utf-8")

    store.restore(archive)
    ud = store.user_data_path(prof.id)
    assert (ud / "Cookies").read_text(encoding="utf-8") == "original session"
    assert not (ud / "Extra").exists(), "the restore merged instead of replacing"
    assert store.get(prof.id).name == "original"
    assert not [d for d in ud.parent.iterdir() if ".replaced-" in d.name], \
        "the old session was left behind"


# --- an interrupted write never truncates a file ----------------------------

@pytest.mark.parametrize("kind", ["profile", "config", "schedules"])
def test_an_interrupted_write_leaves_the_previous_content(tmp_path, monkeypatch, kind):
    """`write_text` truncates first and then writes, so a crash mid-write left
    an empty or half-written file - and the store reads that as "no profiles".

    `os.replace` is broken for the whole call here, which is deliberately worse
    than a real transient failure: it breaks the rollback as well as the write.
    A profile save is a two-file batch now, so that worst case surfaces as
    `RollbackFailed` naming what could not be put back, rather than the bare
    `OSError` a single-file writer raised. Either way the assertion below is
    the point - the target was never touched, because the swap that would have
    touched it is the thing that failed.
    """
    from numera import paths
    from numera.paths import RollbackFailed

    store = ProfileStore(tmp_path)
    sched = ScheduleStore(tmp_path)
    if kind == "profile":
        prof = store.save(profile("first"))
        target = tmp_path / "profiles" / f"{prof.id}.json"
        act = lambda: store.save(Profile(id=prof.id, name="second",  # noqa: E731
                                         fingerprint=prof.fingerprint))
    elif kind == "config":
        # Through the public mutation API: full replacement is private now and
        # requires the caller to already hold the transaction, because a
        # snapshot built before the lock deletes whatever landed in between.
        store.set_config_many({"default_engine": "playwright"})
        target = store.config_path
        act = lambda: store.set_config_many({"default_engine": "camoufox"})  # noqa: E731
    else:
        sched.add(profile_id="p1", every_hours=2)
        target = sched.path
        act = lambda: sched.add(profile_id="p2", every_hours=3)  # noqa: E731

    before = target.read_text(encoding="utf-8")
    real = os.replace
    monkeypatch.setattr(os, "replace",
                        lambda *a, **k: (_ for _ in ()).throw(OSError("crash")))
    with pytest.raises((OSError, RollbackFailed)):
        act()
    monkeypatch.setattr(os, "replace", real)

    assert target.read_text(encoding="utf-8") == before, "the file was left truncated"
    assert json.loads(target.read_text(encoding="utf-8")) is not None
    assert not list(target.parent.glob("*.tmp*")), "a temp file was left behind"
    assert paths  # the writer under test


# --- a job that finishes before its parent recorded it ----------------------

def test_a_fast_child_keeps_its_outcome(tmp_path):
    """The exact interleaving that lost it: the parent read the record before
    `Popen` returned, the child wrote its outcome, and then the parent's write
    put back a record with no outcome in it. The job showed as running until the
    startup grace expired and then as "lost", having actually finished."""
    d = tmp_path / "jobs"
    d.mkdir()
    rec = d / "abc123.json"

    jobs.record_exit(str(rec), 0)              # the child, first
    jobs._write(rec, {"id": "abc123", "pid": 4242, "started": time.time()})   # the parent

    got = jobs.get(tmp_path, "abc123")
    assert got["state"] == "done", got
    assert got["exit"] == 0
    assert got["pid"] == 4242, "the child's write lost the parent's fields"
    assert got["running"] is False


def test_a_slow_child_also_keeps_both_halves(tmp_path):
    """The ordinary order, so the fix is not one-directional."""
    d = tmp_path / "jobs"
    d.mkdir()
    rec = d / "abc123.json"
    jobs._write(rec, {"id": "abc123", "pid": 4242, "started": time.time()})
    jobs.record_exit(str(rec), 3)

    got = jobs.get(tmp_path, "abc123")
    assert got["state"] == "failed" and got["exit"] == 3 and got["pid"] == 4242


def test_the_outcome_file_is_not_listed_as_a_second_job(tmp_path):
    d = tmp_path / "jobs"
    d.mkdir()
    rec = d / "abc123.json"
    jobs._write(rec, {"id": "abc123", "pid": 1, "started": time.time()})
    jobs.record_exit(str(rec), 0)
    assert [r["id"] for r in jobs.listing(tmp_path)] == ["abc123"]


def test_pruning_removes_both_halves(tmp_path):
    d = tmp_path / "jobs"
    d.mkdir()
    rec = d / "abc123.json"
    jobs._write(rec, {"id": "abc123", "pid": 1, "started": time.time()})
    jobs.record_exit(str(rec), 0)
    assert jobs.prune(tmp_path) == 1
    assert not list(d.glob("abc123*")), list(d.glob("abc123*"))


# --- schedules ---------------------------------------------------------------

def test_the_store_refuses_an_invalid_schedule_patch(tmp_path):
    """Validation lived only in the HTTP route, so the CLI and any internal
    caller wrote raw values straight to disk."""
    sched = ScheduleStore(tmp_path)
    entry = sched.add(profile_id="p1", every_hours=2)
    before = sched.path.read_text(encoding="utf-8")

    for bad in ({"everyHours": "soon"}, {"everyHours": -5}, {"minutes": 0},
                {"enabled": "yes"}):
        with pytest.raises(ValueError):
            sched.update(entry["id"], bad)
    assert sched.path.read_text(encoding="utf-8") == before, "an invalid patch was stored"

    from numera.scheduler import next_run_at
    assert isinstance(next_run_at(sched.get(entry["id"])), float)


def test_a_client_cannot_move_the_next_run(tmp_path):
    sched = ScheduleStore(tmp_path)
    entry = sched.add(profile_id="p1", every_hours=2)
    with pytest.raises(ValueError):
        sched.update(entry["id"], {"lastRun": 0})


def test_the_scheduler_can_still_write_its_own_outcome(tmp_path):
    """The other half of the split: validation must not block the writes the
    scheduler itself has to make on every tick."""
    sched = ScheduleStore(tmp_path)
    entry = sched.add(profile_id="p1", every_hours=2)
    sched.mark_run(entry["id"], 1234.0)
    sched.record(entry["id"], lastOutcome="ok", lastError=None)
    row = sched.get(entry["id"])
    assert row["lastRun"] == 1234.0 and row["lastOutcome"] == "ok"

    with pytest.raises(ValueError, match="not a scheduler-owned field"):
        sched.record(entry["id"], everyHours=99)


def test_a_valid_patch_is_normalised_not_just_accepted(tmp_path):
    sched = ScheduleStore(tmp_path)
    entry = sched.add(profile_id="p1", every_hours=2)
    out = sched.update(entry["id"], {"everyHours": 0.01})
    assert out["everyHours"] == pytest.approx(0.25), "the floor was not applied"


def test_concurrent_schedule_writes_do_not_lose_each_other(tmp_path):
    """A background tick and an HTTP edit hold different snapshots; writing a
    stale one back drops the other's change."""
    import threading

    sched = ScheduleStore(tmp_path)
    rows = [sched.add(profile_id=f"p{i}", every_hours=2)["id"] for i in range(8)]
    errors = []

    def touch(sid, i):
        try:
            for _ in range(15):
                sched.update(sid, {"minutes": float(i + 1)})
                sched.record(sid, lastRun=float(i))
        except Exception as exc:                                # noqa: BLE001
            errors.append(exc)

    threads = [threading.Thread(target=touch, args=(sid, i))
               for i, sid in enumerate(rows)]
    for t in threads:
        t.start()
    for t in threads:
        t.join(timeout=30)
    assert not errors, errors
    assert not any(t.is_alive() for t in threads), "a writer never finished"

    final = sched.list()
    assert len(final) == 8, f"schedules were lost: {len(final)}"
    for i, sid in enumerate(rows):
        row = next(s for s in final if s["id"] == sid)
        assert row["minutes"] == float(i + 1), f"{sid} lost its update"
        assert row["lastRun"] == float(i)
