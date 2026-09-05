"""The closure pass: nine defects that each reported one state while disk held another.

The shape they share is a promise the code could not keep - a detached job that
carried the password it was told to hide, a rename that replaced the profile it
was meant to patch, a failed delete that had already deleted, a locked vault
that exported blanks, and a guard that validated one address and connected to
whatever DNS said next.
"""
import json
import socket
import threading

import pytest

from numera import crypto, generate, proxypool
from numera.jobs import child_argv
from numera.models import Profile, Proxy
from numera.store import ProfileStore

pytest.importorskip("cryptography")
pytest.importorskip("fastapi")

from fastapi.testclient import TestClient                       # noqa: E402

from numera.server import create_app                            # noqa: E402
from numera.server.stores import DashboardStore, ProxyPool      # noqa: E402

SECRET = "PROXY-PASSWORD"


@pytest.fixture
def c(tmp_path):
    return TestClient(create_app(data_dir=str(tmp_path), start_scheduler=False))


def vaulted(root, password="master"):
    engine = ProfileStore(root)
    pool = ProxyPool(root, engine)
    engine.save(Profile(name="p1", fingerprint=generate(seed=1),
                        proxy=Proxy(server="http://1.2.3.4:8080",
                                    username="u", password=SECRET)))
    pool.add_many([{"type": "HTTP", "host": "5.6.7.8", "port": "8080",
                    "user": "u", "pass": SECRET}])
    engine.enable_vault(password)
    return engine


# --- 1. a detached job must not carry the password ------------------------

@pytest.mark.parametrize("argv", [
    ["--password", "SECRET", "launch", "p", "--detach"],
    ["--password=SECRET", "launch", "p", "--detach"],
    ["launch", "p", "--password", "SECRET", "--detach"],
])
def test_a_detached_child_argv_carries_no_password(tmp_path, argv):
    """`spawn()` writes the argv it ran into `jobs/<id>.json`, so anything left
    here is both in the process table and on disk in the clear."""
    out = child_argv(argv, tmp_path)
    assert not any("SECRET" in a for a in out), out
    assert "--password" not in out
    assert "--detach" not in out
    assert "launch" in out and "p" in out


def test_the_job_record_never_holds_the_password(tmp_path, monkeypatch):
    from numera import jobs

    # Every call, then pick out the one `spawn` made. `subprocess.Popen` is
    # patched process-wide, so keeping only the most recent call meant asserting
    # against whatever ran last - which on Windows happened to be `spawn`, and on
    # Linux was a stranger with no `env=`. Matching `-m numera` by position is
    # exact: the interpreter path contains the word too.
    calls = []

    class FakeProc:
        pid = 4242

    def capture(cmd, *a, **k):
        calls.append({"argv": list(cmd), "env": k.get("env") or {}})
        return FakeProc()

    monkeypatch.setattr(jobs.subprocess, "Popen", capture)
    rec = jobs.spawn(tmp_path, child_argv(["--password", "SECRET", "launch", "p", "--detach"],
                                          tmp_path), command="launch", password="SECRET")

    spawned = [c for c in calls if c["argv"][1:3] == ["-m", "numera"]]
    assert len(spawned) == 1, f"expected one numera child, saw {len(spawned)}"
    call = spawned[0]

    on_disk = (tmp_path / "jobs" / f"{rec['id']}.json").read_text(encoding="utf-8")
    assert "SECRET" not in on_disk, "the password was written to the job record"
    assert "SECRET" not in " ".join(call["argv"]), "the password reached the child's argv"
    assert call["env"].get("NUMERA_PASSWORD") == "SECRET", "the child was left locked"


# --- 2. a partial PUT is a patch ------------------------------------------

def test_a_name_only_put_changes_only_the_name(c, tmp_path):
    """Normalising the body alone treated every omitted field as "reset me": a
    rename moved a Camoufox profile to Playwright (the converter's default),
    emptied tags, notes and folder, and minted a fresh `_created`."""
    made = c.post("/api/profiles", json={
        "name": "orig", "engine": "camoufox", "tags": ["keep"], "notes": "important",
        "folder": "Work", "startupUrls": "https://example.com",
        "proxy": {"type": "SOCKS5", "host": "9.9.9.9", "port": "1080", "user": "u"},
    }).json()
    pid = made["id"]
    path = tmp_path / "dashboard" / f"{pid}.json"
    before = json.loads(path.read_text(encoding="utf-8"))

    assert c.put(f"/api/profiles/{pid}", json={"name": "renamed"}).status_code == 200
    after = json.loads(path.read_text(encoding="utf-8"))

    assert after["name"] == "renamed"
    # `_rev` is expected to change - a write that did not bump it would defeat
    # the stale-write check that exists to stop one client erasing another's edit.
    differing = {k for k in set(before) | set(after)
                 if k not in ("name", "_rev") and before.get(k) != after.get(k)}
    assert not differing, f"a rename also changed: {sorted(differing)}"


def test_a_partial_put_preserves_server_owned_bookkeeping(c, tmp_path):
    made = c.post("/api/profiles", json={"name": "orig"}).json()
    pid = made["id"]
    path = tmp_path / "dashboard" / f"{pid}.json"
    data = json.loads(path.read_text(encoding="utf-8"))
    data["_trust"] = {"grade": "A"}
    data["_last_active"] = 1234.0
    path.write_text(json.dumps(data, indent=2), encoding="utf-8")

    c.put(f"/api/profiles/{pid}", json={"name": "renamed"})
    after = json.loads(path.read_text(encoding="utf-8"))
    assert after["_trust"] == {"grade": "A"}, "cached trust was discarded"
    assert after["_last_active"] == 1234.0
    assert after["_created"] == data["_created"]


def test_a_put_can_still_change_what_it_names(c, tmp_path):
    made = c.post("/api/profiles", json={"name": "a", "tags": ["x"]}).json()
    c.put(f"/api/profiles/{made['id']}", json={"tags": ["y", "z"]})
    assert set(c.get("/api/profiles").json()[0]["tags"]) == {"y", "z"}


# --- 3. a failed delete deleted nothing -----------------------------------

def test_a_held_session_leaves_the_profile_entirely_intact(tmp_path, monkeypatch):
    """The metadata used to go first, so a session that could not be detached
    produced a failed request that had already removed the profile."""
    from pathlib import Path

    engine = ProfileStore(tmp_path)
    dash = DashboardStore(tmp_path, engine_store=engine)
    pid = dash.save({"name": "held", "tags": []})["id"]
    session = engine.user_data_path(pid)
    session.mkdir(parents=True, exist_ok=True)
    (session / "Cookies").write_text("live", encoding="utf-8")

    real_rename = Path.rename
    monkeypatch.setattr(Path, "rename",
                        lambda self, t: (_ for _ in ()).throw(OSError("in use"))
                        if self.name == pid else real_rename(self, t))
    with pytest.raises(OSError):
        dash.delete(pid)
    monkeypatch.setattr(Path, "rename", real_rename)

    assert (tmp_path / "dashboard" / f"{pid}.json").is_file()
    assert (tmp_path / "profiles" / f"{pid}.json").is_file()
    assert (session / "Cookies").read_text(encoding="utf-8") == "live"


def test_a_metadata_failure_puts_the_session_back(tmp_path, monkeypatch):
    from pathlib import Path

    engine = ProfileStore(tmp_path)
    dash = DashboardStore(tmp_path, engine_store=engine)
    pid = dash.save({"name": "p", "tags": []})["id"]
    session = engine.user_data_path(pid)
    session.mkdir(parents=True, exist_ok=True)
    (session / "Cookies").write_text("live", encoding="utf-8")

    real_unlink = Path.unlink
    monkeypatch.setattr(Path, "unlink",
                        lambda self, *a, **k: (_ for _ in ()).throw(OSError("locked"))
                        if self.parent.name == "profiles" else real_unlink(self, *a, **k))
    with pytest.raises(OSError):
        dash.delete(pid)
    monkeypatch.setattr(Path, "unlink", real_unlink)

    assert (session / "Cookies").read_text(encoding="utf-8") == "live", \
        "the session was not restored after the metadata failure"
    assert (tmp_path / "dashboard" / f"{pid}.json").is_file()


def test_a_tombstone_that_cannot_be_reclaimed_still_commits(tmp_path, monkeypatch):
    """Cleanup is untidiness, not failure: the profile is gone and the detached
    bytes are unreachable through it."""
    engine = ProfileStore(tmp_path)
    dash = DashboardStore(tmp_path, engine_store=engine)
    pid = dash.save({"name": "p", "tags": []})["id"]
    engine.user_data_path(pid).mkdir(parents=True, exist_ok=True)

    monkeypatch.setattr(ProfileStore, "discard_tombstone", staticmethod(lambda t: False))
    with pytest.warns(RuntimeWarning, match="still on disk"):
        assert dash.delete(pid) is True
    assert not (tmp_path / "dashboard" / f"{pid}.json").exists()


# --- 4. locked proxy work refuses everywhere ------------------------------

def test_a_locked_pool_export_is_refused(tmp_path):
    """A blank-credential export looks complete and re-imports as broken."""
    vaulted(tmp_path)
    c = TestClient(create_app(data_dir=str(tmp_path), start_scheduler=False))
    r = c.post("/api/proxies/export", json={})
    assert r.status_code == 409, r.status_code
    assert SECRET not in r.text


def test_a_locked_bulk_rotate_is_refused(tmp_path):
    vaulted(tmp_path)
    c = TestClient(create_app(data_dir=str(tmp_path), start_scheduler=False))
    pid = c.get("/api/proxies").json()[0]["id"]
    assert c.post("/api/proxies/rotate", json={"ids": [pid]}).status_code == 409


def test_an_unlocked_export_still_carries_the_credential(tmp_path):
    vaulted(tmp_path)
    c = TestClient(create_app(data_dir=str(tmp_path), start_scheduler=False,
                              password="master"))
    r = c.post("/api/proxies/export", json={})
    assert r.status_code == 200, r.json()
    assert SECRET in r.json()["text"]


def test_a_withheld_secret_does_not_dedupe_as_passwordless(tmp_path):
    """A locked entry reads back `pass=None`, so a genuinely passwordless proxy
    at the same address would be dropped as a duplicate."""
    engine = vaulted(tmp_path)
    locked = ProxyPool(tmp_path, ProfileStore(tmp_path))
    added = locked.add_many([{"type": "HTTP", "host": "5.6.7.8", "port": "8080",
                              "user": "u"}])
    assert added, "a passwordless proxy was deduped against a withheld secret"
    assert engine is not None


@pytest.mark.parametrize("declared, expect", [
    ("socks4", "SOCKS4"), ("socks5", "SOCKS5"), ("https", "HTTPS"), ("http", "HTTP"),
])
def test_the_importer_keeps_the_protocol_the_export_declared(declared, expect):
    """Every `socks*` became SOCKS5 and everything else HTTP, so the imported
    profile claimed one protocol while the browser spoke another."""
    from numera.importers import profile_from_export

    prof, _ = profile_from_export({
        "name": "x",
        "proxy": {"host": "1.2.3.4", "port": 1080, "type": declared, "mode": declared},
    })
    assert prof.proxy is not None
    assert proxypool.split_server(prof.proxy.server)[0] == expect.lower(), prof.proxy.server


# --- 5. bulk update and import are transactions ---------------------------

def test_a_bulk_update_that_fails_part_way_changes_nothing(c, tmp_path, monkeypatch):
    from numera import paths

    ids = c.post("/api/profiles/batch",
                 json={"profiles": [{"name": f"p{i}"} for i in range(3)]}).json()["ids"]
    before = {i: json.loads((tmp_path / "dashboard" / f"{i}.json").read_text(encoding="utf-8"))
              for i in ids}

    real = paths.write_json
    calls = {"n": 0}

    def fail_late(path, data):
        calls["n"] += 1
        if calls["n"] == 3:
            raise OSError("disk full")
        return real(path, data)

    monkeypatch.setattr(paths, "write_json", fail_late)
    with pytest.raises(OSError):
        c.post("/api/profiles/bulk", json={"ids": ids, "patch": {"notes": "edited"}})
    monkeypatch.setattr(paths, "write_json", real)

    after = {i: json.loads((tmp_path / "dashboard" / f"{i}.json").read_text(encoding="utf-8"))
             for i in ids}
    assert after == before, "part of the selection was edited"


def test_an_import_that_fails_part_way_writes_nothing(c, tmp_path, monkeypatch):
    from numera import paths

    payload = json.dumps([{"name": f"imported-{i}"} for i in range(3)])
    real = paths.write_json
    calls = {"n": 0}

    def fail_late(path, data):
        calls["n"] += 1
        if calls["n"] == 3:
            raise OSError("disk full")
        return real(path, data)

    monkeypatch.setattr(paths, "write_json", fail_late)
    with pytest.raises(OSError):
        c.post("/api/profiles/import", json={"text": payload})
    monkeypatch.setattr(paths, "write_json", real)

    assert c.get("/api/profiles").json() == [], "a partial import survived"


def test_a_successful_bulk_update_still_applies(c):
    ids = c.post("/api/profiles/batch",
                 json={"profiles": [{"name": "a"}, {"name": "b"}]}).json()["ids"]
    r = c.post("/api/profiles/bulk", json={"ids": ids, "patch": {"notes": "edited"}})
    assert r.status_code == 200, r.json()
    assert all(p["notes"] == "edited" for p in c.get("/api/profiles").json())


# --- 6. a folder edit and a profile save do not overwrite each other ------

def test_a_folder_rename_and_a_profile_save_both_survive(tmp_path):
    """`folders` rewrote `dashboard/*.json` through its own batch while
    `DashboardStore.save()` locked per file, so one atomic replace could erase
    the other's change."""
    from numera import folders

    engine = ProfileStore(tmp_path)
    dash = DashboardStore(tmp_path, engine_store=engine)
    pid = dash.save({"name": "p", "tags": [], "folder": "Work"})["id"]
    folders.create(tmp_path, "Work")

    errors = []

    def rename():
        try:
            for _ in range(10):
                folders.rename(tmp_path, "Work", "Client")
                folders.rename(tmp_path, "Client", "Work")
        except Exception as exc:                                # noqa: BLE001
            errors.append(exc)

    def edit():
        try:
            for i in range(10):
                current = dash.get(pid)
                dash.save({**current, "notes": f"edit-{i}"})
        except Exception as exc:                                # noqa: BLE001
            errors.append(exc)

    threads = [threading.Thread(target=rename), threading.Thread(target=edit)]
    for t in threads:
        t.start()
    for t in threads:
        t.join(timeout=60)
    assert not errors, errors
    assert not any(t.is_alive() for t in threads)

    final = dash.get(pid)
    assert final["notes"].startswith("edit-"), "the profile edit was lost"
    assert final["folder"] in ("Work", "Client"), final["folder"]


# --- 7. the SSRF guard allows only globally routable addresses ------------

@pytest.mark.parametrize("url", [
    "http://100.64.0.1/rotate",          # CGNAT - not private, not reserved
    "http://100.127.255.254/rotate",
    "http://127.0.0.1/rotate",
    "http://169.254.169.254/latest/",
    "http://10.0.0.5/rotate",
    "http://192.0.2.1/rotate",           # TEST-NET-1
    "http://[fc00::1]/rotate",
])
def test_only_globally_routable_destinations_are_allowed(url):
    """The old check enumerated private/loopback/link-local/reserved and missed
    `100.64.0.0/10`, which Python reports as none of them."""
    problem, pinned = proxypool._public_destination(url)
    assert problem, f"{url} was allowed"
    assert pinned is None


def test_a_public_destination_is_allowed_and_pinned():
    problem, pinned = proxypool._public_destination("https://example.com/rotate")
    assert problem is None, problem
    assert pinned is not None and pinned.is_global


def test_the_connection_is_pinned_to_the_address_that_was_validated(monkeypatch):
    """Validation resolved once and `urlopen` resolved again, so a hostname
    could answer public for the check and private for the connection."""
    calls = {"n": 0}
    real = socket.getaddrinfo

    def flip(host, port, *a, **k):
        if host == "rebind.test":
            calls["n"] += 1
            ip = "93.184.216.34" if calls["n"] == 1 else "127.0.0.1"
            return [(socket.AF_INET, socket.SOCK_STREAM, 6, "", (ip, port))]
        return real(host, port, *a, **k)

    monkeypatch.setattr(socket, "getaddrinfo", flip)
    problem, pinned = proxypool._public_destination("http://rebind.test/rotate")
    assert problem is None
    assert str(pinned) == "93.184.216.34", "validation did not pin the checked address"
    # The second lookup would hand back loopback; the pin means it is never used.
    assert not pinned.is_loopback


def test_a_non_http_scheme_is_still_refused():
    for url in ("ftp://example.com/x", "file:///etc/passwd"):
        problem, _ = proxypool._public_destination(url)
        assert problem


def test_crypto_still_round_trips_after_all_of_this(tmp_path):
    """A cheap canary: several of these touched the vault path."""
    salt = crypto.new_salt()
    assert crypto.decrypt("pw", salt, crypto.encrypt("pw", salt, "x")) == "x"
