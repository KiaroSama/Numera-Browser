"""
Headless parity: everything a human can do in the dashboard, a script can do too.

The dashboard is a client of the HTTP API, so "does the API cover the GUI?" is
true by construction and worth nothing as a test. The claim actually worth
proving is the harder one: **a machine with no browser at all can drive Numera
through the CLI.** So this file reads the two surfaces from source —
``dashboard/src/api.js`` for what the GUI calls, ``cli.build_parser()`` for what
the CLI offers — and holds them against an explicit map.

The map is the specification. A capability the dashboard grows and nobody gives
a headless path fails ``test_no_unmapped_dashboard_capabilities``; a gap that
gets closed fails ``test_known_gaps_have_not_drifted``, which is the point —
both directions are news.

And per the project's first lesson, a command *existing* proves nothing:
``test_every_offline_command_emits_only_json`` runs each mapped command for
real and reads what actually lands on stdout.
"""

from __future__ import annotations

import argparse
import io
import json
import re
import subprocess
import sys
from contextlib import redirect_stdout
from pathlib import Path

import pytest

from numera import cli

REPO = Path(__file__).resolve().parents[2]
API_JS = REPO / "dashboard" / "src" / "api.js"

# Verdicts.
CLI = "cli"    # one command does it
VIA = "via"    # a documented combination of commands does it
NA = "n/a"     # not a capability — only the GUI's own transport
GAP = "gap"    # no headless path exists (frozen in KNOWN_GAPS)

# Every api.* call in the dashboard -> (verdict, command paths, why).
PARITY: dict[str, tuple[str, tuple[tuple[str, ...], ...], str]] = {
    "health":          (NA, (), "the GUI probing whether its backend is reachable; "
                                "a CLI has no backend to probe"),
    "engines":         (CLI, (("engines",),), ""),
    "getConfig":       (CLI, (("config",),), ""),
    "setDefaultEngine": (CLI, (("default-engine",),), "also `config --default-engine`"),
    "setDefaultExtensions": (CLI, (("config",),), "`config --default-extension` replaces the "
                                                  "set, as the API call does; `ext default-add` "
                                                  "/ `default-remove` change it one at a time"),
    "installExtension": (CLI, (("ext", "install"),), "takes the same Web Store link the "
                                                     "Extensions page takes; the crx is "
                                                     "fetched and unpacked engine-side"),
    "installedExtensions": (CLI, (("ext", "installed"),), "id, folder and name of each one "
                                                          "fetched from the store"),
    "extensionIcon":   (NA, (), "builds a URL for an <img>; the icon is a thing to "
                                "look at, not an action to perform. `ext installed` "
                                "reports the icon's path for anything that needs it"),
    "createFolder":    (CLI, (("folder", "create"),), ""),
    "renameFolder":    (CLI, (("folder", "rename"),), "moves the folder's profiles with it"),
    "deleteFolder":    (CLI, (("folder", "delete"),), "reassigns to Unfiled, does not delete "
                                                      "the profiles"),
    "list":            (CLI, (("list",),), ""),
    "create":          (CLI, (("create",),), ""),
    "update":          (VIA, (("edit",), ("apply",), ("regen",), ("export",), ("import",)),
                        "edit/apply/regen cover the fields they name; export -> edit the "
                        "JSON -> import is the full-fidelity path for everything else"),
    "remove":          (CLI, (("delete",),), ""),
    "launch":          (CLI, (("launch",),), "add --detach for a job handle"),
    "stop":            (CLI, (("stop",),), "ends the jobs started with --detach"),
    "exportCookies":   (CLI, (("cookies", "export"),), ""),
    "importCookies":   (CLI, (("cookies", "import"),), ""),
    "proxyTest":       (CLI, (("proxy", "test"),), ""),
    "trust":           (CLI, (("trust",),), ""),
    "tls":             (CLI, (("tls",),), ""),
    "dns":             (CLI, (("dns",),), ""),
    "align":           (CLI, (("align",),), ""),
    "attach":          (CLI, (("attach",),), ""),
    "warmup":          (CLI, (("warmup",),), ""),
    "bulkUpdate":      (CLI, (("apply",),), ""),
    "batchCreate":     (CLI, (("bulk-create",),), ""),
    "importProfiles":  (CLI, (("import-from",),), ""),
    "listProxies":     (CLI, (("pool", "list"),), ""),
    "addProxies":      (CLI, (("pool", "add"),), ""),
    "deleteProxy":     (CLI, (("pool", "remove"),), ""),
    "deleteProxies":   (CLI, (("pool", "remove"),), "`pool remove` takes several ids"),
    "testProxy":       (CLI, (("pool", "test"),), ""),
    "testProxies":     (CLI, (("pool", "test"),), "`pool test` takes several ids, or --all"),
    "updateProxy":     (CLI, (("pool", "edit"),), "host/port/user/pass/type/country/label"),
    "exportProxies":   (CLI, (("pool", "export"),), "the dashboard builds the file in the "
                                                    "browser; `pool export` writes the same "
                                                    "lines, and both re-import"),
    "rotateProxy":     (CLI, (("pool", "rotate"),), ""),
    "rotateProxies":   (CLI, (("pool", "rotate"),), "takes several ids"),
    "archiveProxies":  (CLI, (("pool", "archive"),), "`--restore` puts them back"),
    "listTokens":      (CLI, (("token", "list"),), ""),
    "createToken":     (CLI, (("token", "create"),), "the value is printed once, then only hashed"),
    "revokeToken":     (CLI, (("token", "revoke"),), ""),
    "assignProxies":   (CLI, (("pool", "assign"),), ""),
    "listSchedules":   (CLI, (("schedule", "list"),), ""),
    "addSchedule":     (CLI, (("schedule", "add"),), ""),
    "updateSchedule":  (CLI, (("schedule", "update"),), ""),
    "deleteSchedule":  (CLI, (("schedule", "remove"),), ""),
    "profileHealth":   (CLI, (("health",),), "no cached trust grade on the engine side; "
                                             "`numera trust <ref>` gets a live one"),
    "collisions":      (CLI, (("collisions",),), ""),
}

# Frozen. Adding to this set is a decision, not a maintenance chore, so the test
# fails when it changes in either direction.
#
# Empty since 2026-08-30: the folder commands closed the last three. Note the
# hole that let this table go stale for a while - `test_every_mapped_cli_command_exists`
# only checks entries marked CLI or VIA, so a GAP entry is never held against the
# real parser. A capability that gains a CLI path keeps claiming to be a gap until
# someone edits this file. Nothing here can detect that; only reading the parser can.
KNOWN_GAPS: set[str] = set()


# --------------------------------------------------------------------------
# Reading the two surfaces.
# --------------------------------------------------------------------------
_REQ = re.compile(r'^\s*(\w+)\s*[:(].*?req\(\s*"(\w+)"\s*,\s*[`"]([^`"]+)', re.MULTILINE)


def dashboard_calls() -> dict[str, tuple[str, str]]:
    """Every ``api.*`` method in api.js, mapped to the route it calls.

    ``health`` hand-rolls its own fetch (it must never throw), so it is picked
    up by name rather than by loosening the regex until it swallows unrelated
    lines.
    """
    src = API_JS.read_text(encoding="utf-8")
    body = src[src.index("export const api = {"):]
    calls = {name: (method, path) for name, method, path in _REQ.findall(body)}
    if re.search(r"\basync health\(\)", body):
        calls["health"] = ("GET", "/api/health")
    return calls


def dashboard_members() -> set[str]:
    """Every member of the `api` object, however it is written.

    `dashboard_calls()` only sees members whose body is a `req(...)` call. A
    member that builds a URL instead - `extensionIcon` is one - is invisible to
    it, so a future capability written that way would slip past the parity guard
    without anyone noticing. This reader is deliberately dumber and wider: it
    takes every top-level key, and the test below demands a verdict for each.
    """
    src = API_JS.read_text(encoding="utf-8")
    body = src[src.index("export const api = {"):]
    # Top-level keys only: two spaces of indent inside the object literal.
    return set(re.findall(r"^  (?:async\s+)?(\w+)\s*[:(]", body, re.MULTILINE))


def cli_commands() -> set[tuple[str, ...]]:
    """Every command path the parser accepts, e.g. ``("cookies", "export")``."""
    found: set[tuple[str, ...]] = set()

    def walk(parser: argparse.ArgumentParser, prefix: tuple[str, ...]) -> None:
        subs = [a for a in parser._actions if isinstance(a, argparse._SubParsersAction)]
        if not subs:
            if prefix:
                found.add(prefix)
            return
        for action in subs:
            for name, sub in action.choices.items():
                walk(sub, (*prefix, name))

    walk(cli.build_parser(), ())
    return found


def test_the_readers_actually_read_something():
    """Guard the readers themselves: a regex that quietly matches nothing would
    make every assertion below pass while proving the opposite."""
    calls = dashboard_calls()
    assert len(calls) >= 30, f"only found {len(calls)} api.js calls: {sorted(calls)}"
    assert calls["launch"][0] == "POST" and "launch" in calls["launch"][1]
    assert calls["list"] == ("GET", "/api/profiles")

    commands = cli_commands()
    assert len(commands) >= 30, f"only found {len(commands)} CLI commands"
    assert {("create",), ("cookies", "export"), ("pool", "assign")} <= commands


# --------------------------------------------------------------------------
# The parity assertions.
# --------------------------------------------------------------------------
def test_no_unmapped_dashboard_capabilities():
    """Every control the GUI offers is accounted for in PARITY."""
    calls = set(dashboard_calls())
    unmapped = calls - set(PARITY)
    assert not unmapped, (
        f"the dashboard gained capabilities with no headless verdict: {sorted(unmapped)} "
        "— add them to PARITY in this file")
    # Compare against every member, not only the `req(...)` ones: PARITY may
    # legitimately name a member that builds a URL instead of calling req.
    stale = set(PARITY) - dashboard_members()
    assert not stale, f"PARITY names calls api.js no longer has: {sorted(stale)}"


def test_no_api_member_escapes_a_verdict():
    """Every key on the `api` object needs a verdict, not only the `req(...)` ones.

    Written after `extensionIcon` shipped and the guard did not see it: it builds
    a URL rather than calling `req`, and `dashboard_calls()` only reads `req`
    calls. It is genuinely not a capability - but nothing established that, and
    the next member written the same way could be one.
    """
    unaccounted = dashboard_members() - set(PARITY)
    assert not unaccounted, (
        f"api.js members with no PARITY verdict: {sorted(unaccounted)}. "
        "Give each one a verdict — NA if it is transport rather than a capability.")


def test_every_mapped_cli_command_exists():
    """The map may not point at a command the parser does not have."""
    commands = cli_commands()
    missing = {
        cap: [" ".join(path) for path in paths if path not in commands]
        for cap, (verdict, paths, _) in PARITY.items()
        if verdict in (CLI, VIA) and any(path not in commands for path in paths)
    }
    assert not missing, f"PARITY points at commands that do not exist: {missing}"


def test_known_gaps_have_not_drifted(capsys):
    """The only capabilities with no headless path are the ones we chose.

    Reported out loud rather than swallowed — running this file prints the gaps.
    """
    gaps = {cap for cap, (verdict, _, _) in PARITY.items() if verdict == GAP}
    with capsys.disabled():
        print(f"\n  headless parity: {len(PARITY) - len(gaps)}/{len(PARITY)} "
              f"dashboard capabilities have a CLI path")
        for cap in sorted(gaps):
            print(f"  GAP  {cap}: {PARITY[cap][2]}")
    assert gaps == KNOWN_GAPS, (
        f"the headless gap set changed: {sorted(gaps)} != {sorted(KNOWN_GAPS)}. "
        "Closing one is good news — update KNOWN_GAPS. Opening one is not.")


def test_a_gap_is_really_a_gap():
    """A capability declared a gap must not have an obvious CLI command sitting there.

    Written after the folder commands shipped and PARITY kept calling them gaps for
    days. Nothing caught it: `test_every_mapped_cli_command_exists` checks only the
    entries marked CLI or VIA, so a GAP entry was never held against the parser.
    The table was its own source of truth, and it was wrong.

    The guess is deliberately crude - `createFolder` -> `folder create`. It cannot
    find every path a gap might have, but it finds the one that matters: a gap
    closed by a command named after it."""
    commands = cli_commands()
    words = re.compile(r"[A-Z][a-z]*|^[a-z]+")
    contradicted = {}
    for cap, (verdict, _, _) in PARITY.items():
        if verdict != GAP:
            continue
        parts = [w.lower() for w in words.findall(cap)]
        if len(parts) == 2:
            verb, noun = parts
            for guess in ((noun, verb), (noun.rstrip("s"), verb)):
                if guess in commands:
                    contradicted[cap] = " ".join(guess)
    assert not contradicted, (
        f"declared a gap, but the CLI has the command: {contradicted}. "
        "Update PARITY and KNOWN_GAPS — the gap closed and nobody noticed.")


def test_dashboard_routes_all_exist_on_the_api(tmp_path):
    """A dashboard call to a route the server does not serve is a dead control."""
    pytest.importorskip("fastapi")
    from numera.server import create_app

    # An explicit root, always: create_app(None) resolves the *real* data
    # directory and runs the first-run import against it.
    app = create_app(str(tmp_path), start_scheduler=False)
    served = {(m, getattr(r, "path", "")) for r in app.routes
              for m in (getattr(r, "methods", None) or ())}
    # api.js writes `${id}` where FastAPI writes `{pid}` / `{sid}` / `{tid}`, and
    # the names never have to agree. Comparing shapes rather than spellings means
    # a route with a new parameter name needs no hand-written alias here - the
    # previous version carried one for schedules and would have needed another
    # for every route added since.
    def shape(path):
        return re.sub(r"(\$\{[^}]+\}|\{[^}]+\})", "{}", path)

    for name, (method, path) in dashboard_calls().items():
        assert (method, shape(path)) in {(m, shape(r)) for m, r in served}, \
            f"api.{name} calls {method} {shape(path)}, which no route serves"


# --------------------------------------------------------------------------
# "A control existing proves nothing" — run the commands and read stdout.
# --------------------------------------------------------------------------
def run(argv: list[str]) -> tuple[int, str]:
    """Invoke the CLI in-process and return (exit code, everything on stdout)."""
    buf = io.StringIO()
    with redirect_stdout(buf):
        code = cli.main(argv)
    return code, buf.getvalue()


def offline_invocations(tmp_path: Path) -> list[list[str]]:
    """Every mapped command that needs neither a browser nor the network, with
    arguments that make it do something real.

    All of them, not a sample: a --json flag that works for the easy command and
    prints prose for the hard one is the exact bug this list exists to catch.
    """
    d = ["--data-dir", str(tmp_path)]
    ext = tmp_path / "ext"
    ext.mkdir()
    exported = tmp_path / "p.json"
    backup = tmp_path / "p.zip"
    proxies = tmp_path / "proxies.txt"
    proxies.write_text("http://u:p@1.2.3.4:8080:US\n5.6.7.8:3128\n", encoding="utf-8")
    return [
        [*d, "create", "alpha", "--tag", "t1"],
        [*d, "create", "beta", "--os", "macos"],
        [*d, "list"],
        [*d, "list", "--tag", "t1"],
        [*d, "show", "alpha"],
        [*d, "show", "nope"],                       # the not-found path answers too
        [*d, "check", "alpha"],
        [*d, "health"],
        [*d, "collisions"],
        [*d, "engines"],
        [*d, "default-engine"],
        [*d, "config"],
        [*d, "config", "--headless-launch", "on"],
        [*d, "edit", "alpha", "--notes", "hello", "--add-tag", "t2"],
        [*d, "regen", "alpha", "--os", "linux"],
        [*d, "export", "alpha", str(exported)],
        [*d, "import", str(exported)],
        [*d, "backup", "alpha", str(backup)],
        [*d, "restore", str(backup), "--new-id"],
        [*d, "bulk-create", "--count", "3", "--prefix", "Bulk"],
        [*d, "apply", "--all", "--notes", "bulk note", "-y"],
        [*d, "ext", "list", "alpha"],
        [*d, "ext", "add", "alpha", str(ext)],
        [*d, "ext", "remove", "alpha", str(ext)],
        [*d, "ext", "default-list"],
        [*d, "ext", "default-add", str(ext)],
        [*d, "ext", "default-remove", str(ext)],
        [*d, "pool", "add", str(proxies)],
        [*d, "pool", "list"],
        # run-due first, while nothing is scheduled. A freshly added schedule is
        # due immediately by design (next_run_at falls back to createdAt), so
        # running it after the add opens a real browser for minutes.
        [*d, "schedule", "run-due"],
        [*d, "schedule", "add", "alpha", "--every", "6"],
        [*d, "schedule", "list"],
        [*d, "jobs", "list"],
        [*d, "jobs", "clear"],
        [*d, "stop", "alpha"],
        [*d, "vault", "status"],
        [*d, "import-from", str(exported)],
        [*d, "delete", "beta", "-y"],
    ]


def test_every_offline_command_emits_only_json(tmp_path):
    """--json puts one JSON document on stdout and nothing else.

    Not "starts with a brace": the whole stream is parsed, so a progress line
    before the payload or a summary after it fails here rather than in a caller.
    """
    for argv in offline_invocations(tmp_path):
        code, out = run([*argv, "--json"])
        label = " ".join(a for a in argv[2:] if not a.startswith(str(tmp_path)))
        assert out.strip(), f"`{label} --json` printed nothing"
        assert "\x1b[" not in out, f"`{label} --json` leaked colour codes"
        try:
            payload = json.loads(out)
        except json.JSONDecodeError as e:
            pytest.fail(f"`{label} --json` did not emit parseable JSON ({e}):\n{out!r}")
        assert isinstance(payload, (dict, list)), f"`{label} --json` emitted {type(payload)}"
        assert isinstance(code, int)


def test_the_commands_keyed_by_a_generated_id_emit_json_too(tmp_path):
    """The ones whose arguments are ids an earlier command minted.

    They cannot go in the static list, and leaving them out is how a "--json
    works everywhere" claim quietly stops being true.
    """
    d = ["--data-dir", str(tmp_path)]
    src = tmp_path / "px.txt"
    src.write_text("http://1.2.3.4:8080:US\n", encoding="utf-8")
    _, out = run([*d, "pool", "add", str(src), "--json"])
    proxy_id = json.loads(out)["proxies"][0]["id"]

    run([*d, "create", "alpha", "--json"])
    _, out = run([*d, "schedule", "add", "alpha", "--every", "6", "--json"])
    sched_id = json.loads(out)["id"]

    for argv in ([*d, "pool", "assign", "--all"],
                 [*d, "pool", "remove", proxy_id],
                 [*d, "schedule", "update", sched_id, "--minutes", "9"],
                 [*d, "schedule", "remove", sched_id],
                 [*d, "jobs", "show", "no-such-job"],
                 [*d, "jobs", "stop", "no-such-job"]):
        _, out = run([*argv, "--json"])
        assert json.loads(out), f"`{' '.join(argv[2:])} --json` emitted {out!r}"


def test_pool_assign_actually_rewrites_the_profile(tmp_path):
    """`pool assign` reports a number; this checks the profile really moved.

    It has to change two things at once — the proxy and the locale/timezone that
    has to follow the proxy's country, or the profile stops being coherent.
    """
    d = ["--data-dir", str(tmp_path)]
    src = tmp_path / "px.txt"
    src.write_text("http://user:pw@9.9.9.9:8080:DE\n", encoding="utf-8")
    run([*d, "pool", "add", str(src), "--json"])
    run([*d, "create", "alpha", "--locale", "en-US", "--json"])

    _, out = run([*d, "pool", "assign", "--all", "--json"])
    assert json.loads(out)["assigned"] == 1

    _, out = run([*d, "show", "alpha", "--json"])
    prof = json.loads(out)
    assert prof["proxy"]["server"] == "http://9.9.9.9:8080"
    assert prof["proxy"]["username"] == "user"
    assert prof["fingerprint"]["locale"] == "de-DE", "the locale did not follow the country"
    assert prof["fingerprint"]["timezone"].startswith("Europe/")


def test_human_output_is_still_the_default(tmp_path):
    """The prose has to survive: --json is an addition, not a replacement."""
    d = ["--data-dir", str(tmp_path)]
    run([*d, "create", "alpha"])
    for argv in ([*d, "list"], [*d, "show", "alpha"], [*d, "health"], [*d, "config"]):
        _, out = run(argv)
        assert out.strip(), f"`{' '.join(argv[2:])}` printed nothing"
        with pytest.raises(json.JSONDecodeError):
            json.loads(out)          # prose, not a payload


def test_json_mode_never_waits_for_a_keyboard(tmp_path):
    """An agent has no stdin. A destructive command without -y must fail, not hang."""
    d = ["--data-dir", str(tmp_path)]
    run([*d, "create", "alpha", "--json"])
    code, out = run([*d, "delete", "alpha", "--json"])
    assert code == 1
    assert "confirmation" in json.loads(out)["error"]
    _, out = run([*d, "list", "--json"])
    assert json.loads(out)["total"] == 1, "it deleted the profile anyway"


def test_export_edit_import_is_a_real_full_profile_edit(tmp_path):
    """PARITY maps the dashboard's whole-profile PUT onto export -> edit -> import.

    Only worth claiming if the round trip keeps the id and applies a field no
    `edit`/`apply` flag can reach.
    """
    d = ["--data-dir", str(tmp_path)]
    _, out = run([*d, "create", "alpha", "--json"])
    pid = json.loads(out)["profile"]["id"]

    dest = tmp_path / "alpha.json"
    run([*d, "export", "alpha", str(dest), "--json"])
    data = json.loads(dest.read_text(encoding="utf-8"))
    data["fingerprint"]["hardware_concurrency"] = 24     # no CLI flag sets this
    dest.write_text(json.dumps(data), encoding="utf-8")

    run([*d, "import", str(dest), "--json"])
    _, out = run([*d, "show", "alpha", "--json"])
    after = json.loads(out)
    assert after["id"] == pid, "the round trip minted a new profile instead of editing one"
    assert after["fingerprint"]["hardware_concurrency"] == 24


def test_the_installed_entry_point_agrees_with_the_in_process_one(tmp_path):
    """Everything above calls main() directly; this proves `python -m numera`
    reaches the same code and prints no banner around the payload."""
    out = subprocess.run(
        [sys.executable, "-m", "numera", "--data-dir", str(tmp_path), "engines", "--json"],
        capture_output=True, text=True, timeout=120, cwd=str(REPO / "engine"),
    )
    assert out.returncode == 0, out.stderr
    assert "default" in json.loads(out.stdout)


# --------------------------------------------------------------------------
# The API is a CLI caller too. These are its exact invocations.
# --------------------------------------------------------------------------
# server.py's _run_cli() shells out to this CLI for everything that needs a
# browser, so the CLI's argument names and exit codes are a contract the
# dashboard depends on. `allow_fail` mirrors the flag server.py passes: without
# it, a non-zero exit becomes a 500 behind the button that ran it.
SERVER_INVOCATIONS = [
    (["cookies", "export", "PID", "out.json"], False),
    (["cookies", "import", "PID", "in.json"], False),
    (["cookies", "import", "PID", "in.json", "--clear"], False),
    (["proxy", "test", "PID", "--json"], True),
    (["trust", "PID", "--json"], False),
    (["tls", "PID", "--json"], True),
    (["dns", "PID", "--json"], True),
    (["align", "PID", "--json"], False),
    (["launch", "PID", "--headless"], False),
    (["attach", "PID", "--port", "9333"], False),
    (["attach", "PID", "--port", "9333", "--headless"], False),
    (["warmup", "PID", "--minutes", "5.0", "--headless"], False),
    (["warmup", "PID", "--minutes", "5.0", "--headless", "--preset", "ads"], False),
]


def test_the_api_can_still_parse_every_command_it_shells_out_to():
    parser = cli.build_parser()
    for argv, _ in SERVER_INVOCATIONS:
        args = parser.parse_args(["--data-dir", "x", *argv])
        assert args.func is not None, argv
        if "--json" in argv:
            assert args.json is True, f"{argv} lost its --json flag"


@pytest.mark.parametrize("command", ["trust", "align"])
def test_trust_and_align_exit_zero_under_json_whatever_the_grade(monkeypatch, tmp_path, command):
    """server.py runs these two without allow_fail.

    A non-zero exit for a merely-imperfect profile would turn the dashboard's
    Trust and Auto-adjust buttons into 500s, so the grade belongs in the payload
    and nowhere near the exit code. The human form still reports the verdict.
    """
    from numera import probe

    poor = {"score": 62, "grade": "C", "passed": 5, "total": 8,
            "checks": [{"name": "canvas", "ok": False, "detail": "noisy"}]}
    monkeypatch.setattr(probe, "trust_report", lambda *a, **k: poor)
    monkeypatch.setattr(probe, "align_to_reality", lambda *a, **k: {
        "before": poor, "after": poor, "changed": [], "os": "windows",
        "checks": poor["checks"], "fingerprint": {}})

    d = ["--data-dir", str(tmp_path)]
    run([*d, "create", "alpha", "--json"])

    code, out = run([*d, command, "alpha", "--json"])
    assert code == 0, f"`{command} --json` exited {code}; server.py reads that as a 500"
    payload = json.loads(out)
    assert (payload.get("score") or payload["after"]["score"]) == 62

    code, out = run([*d, command, "alpha"])
    assert code == 1, f"the human `{command}` stopped reporting the verdict in its exit code"


# --------------------------------------------------------------------------
# The gap no command map can see.
# --------------------------------------------------------------------------
# Was a strict xfail while the two stores diverged (B17). `DashboardStore` now
# mirrors every write into the engine store, and `list_profiles` adopts anything
# the CLI created, so this passes outright - the strictness is what announced it.
def test_a_cli_created_profile_is_visible_to_the_dashboard(tmp_path):
    pytest.importorskip("fastapi")
    from fastapi.testclient import TestClient

    from numera.server import create_app

    run(["--data-dir", str(tmp_path), "create", "headless-made", "--json"])
    client = TestClient(create_app(str(tmp_path), start_scheduler=False))
    names = [p["name"] for p in client.get("/api/profiles").json()]
    assert "headless-made" in names, \
        f"`numera create` wrote a profile the dashboard cannot see: {names}"
