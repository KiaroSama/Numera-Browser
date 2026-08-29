"""
The MCP surface: a real handshake, real tool calls, real answers.

The tool list is derived from the CLI parser, so most of what could break here
is the derivation — a schema that names an argument the CLI does not have, or a
call that builds the wrong command line. Both are only visible by running the
command, so these tests do.
"""

from __future__ import annotations

import io
import json
import subprocess
import sys
from pathlib import Path

import pytest

from numera import mcp

REPO = Path(__file__).resolve().parents[2]


def test_the_tool_list_covers_the_cli_and_nothing_it_should_not():
    names = {t["name"] for t in mcp.tools()}
    assert {"create", "list", "launch", "stop", "trust", "health",
            "cookies_export", "pool_assign", "schedule_update",
            "jobs_show", "bulk_create"} <= names
    # Servers are not tools: a caller would sit there until the timeout.
    assert "serve" not in names and "mcp" not in names
    assert len(names) >= 40, f"only {len(names)} tools derived"


def test_every_schema_is_well_formed_and_matches_its_command():
    for tool in mcp.tools():
        schema = tool["inputSchema"]
        assert schema["type"] == "object"
        assert tool["description"], f"{tool['name']} has no description"
        assert set(schema["required"]) <= set(schema["properties"]), tool["name"]
        for prop, spec in schema["properties"].items():
            assert spec["type"] in ("string", "boolean", "integer", "number", "array"), \
                f"{tool['name']}.{prop} has type {spec['type']}"
        # Every command inherits --detach, and it must arrive as a boolean.
        assert schema["properties"]["detach"]["type"] == "boolean", tool["name"]
        # --json is ours to add; a caller passing it would double the flag.
        assert "json" not in schema["properties"], tool["name"]


def test_required_arguments_are_the_positional_ones():
    by_name = {t["name"]: t for t in mcp.tools()}
    assert by_name["create"]["inputSchema"]["required"] == ["name"]
    assert by_name["show"]["inputSchema"]["required"] == ["ref"]
    assert by_name["cookies_export"]["inputSchema"]["required"] == ["ref", "file"]
    # `default-engine` takes an optional positional, so nothing is required.
    assert by_name["default_engine"]["inputSchema"]["required"] == []
    # A repeatable flag is an array, a store_true is a boolean, a typed one keeps its type.
    props = by_name["bulk_create"]["inputSchema"]["properties"]
    assert props["count"]["type"] == "integer"
    assert props["tag"]["type"] == "array"
    assert props["mobile"]["type"] == "boolean"
    assert props["os"]["enum"] == ["windows", "macos", "linux", "android"]


def test_a_tool_call_runs_the_command_and_returns_its_payload(tmp_path):
    made = mcp.call("create", {"name": "alpha", "os": "linux", "tag": ["t1", "t2"]},
                    data_dir=str(tmp_path))
    assert made["isError"] is False
    payload = json.loads(made["content"][0]["text"])
    assert payload["ok"] is True
    assert payload["profile"]["fingerprint"]["os"] == "linux", "the --os flag was dropped"
    assert payload["profile"]["tags"] == ["t1", "t2"], "the repeatable --tag was dropped"
    assert made["structuredContent"] == payload

    listed = json.loads(mcp.call("list", {}, data_dir=str(tmp_path))["content"][0]["text"])
    assert [p["name"] for p in listed["profiles"]] == ["alpha"], \
        "the second call did not see the first one's work"


def test_a_verdict_is_a_result_and_a_broken_call_is_an_error(tmp_path):
    mcp.call("create", {"name": "alpha"}, data_dir=str(tmp_path))

    # A non-zero exit that still has a payload is an answer, not a failure.
    ok = mcp.call("check", {"ref": "alpha"}, data_dir=str(tmp_path))
    assert ok["isError"] is False
    assert "exitCode" in ok["_meta"]

    missing = mcp.call("show", {"ref": "nope"}, data_dir=str(tmp_path))
    assert missing["isError"] is False           # the CLI answered, in JSON
    assert json.loads(missing["content"][0]["text"])["ok"] is False

    unknown = mcp.call("no_such_tool", {}, data_dir=str(tmp_path))
    assert unknown["isError"] is True


def test_detach_comes_back_as_a_job_handle(tmp_path):
    res = mcp.call("bulk_create", {"count": 5, "prefix": "Job", "detach": True},
                   data_dir=str(tmp_path))
    handle = json.loads(res["content"][0]["text"])
    assert handle["command"] == "bulk-create" and handle["pid"]

    shown = json.loads(mcp.call("jobs_show", {"id": handle["id"]},
                                data_dir=str(tmp_path))["content"][0]["text"])
    assert shown["id"] == handle["id"]
    assert shown["state"] in ("starting", "running", "done", "failed")

    # Leave nothing behind: the guarded runner counts stray processes.
    mcp.call("jobs_stop", {"id": handle["id"]}, data_dir=str(tmp_path))


def _rpc(*messages: dict) -> list[dict]:
    """Drive the stdio loop the way a client does, and read what comes back."""
    out = io.StringIO()
    mcp.serve(stdin=io.StringIO("".join(json.dumps(m) + "\n" for m in messages)),
              stdout=out)
    return [json.loads(line) for line in out.getvalue().splitlines() if line.strip()]


def test_the_handshake_a_client_actually_performs():
    replies = _rpc(
        {"jsonrpc": "2.0", "id": 1, "method": "initialize",
         "params": {"protocolVersion": "2025-06-18", "capabilities": {},
                    "clientInfo": {"name": "test", "version": "1"}}},
        {"jsonrpc": "2.0", "method": "notifications/initialized"},   # no id: no reply
        {"jsonrpc": "2.0", "id": 2, "method": "tools/list"},
        {"jsonrpc": "2.0", "id": 3, "method": "ping"},
        {"jsonrpc": "2.0", "id": 4, "method": "nonsense/method"},
    )
    assert [r.get("id") for r in replies] == [1, 2, 3, 4], \
        "a notification was answered, or a request was not"
    assert replies[0]["result"]["protocolVersion"] == "2025-06-18"
    assert replies[0]["result"]["serverInfo"]["name"] == "numera"
    assert replies[0]["result"]["capabilities"]["tools"] is not None
    assert len(replies[1]["result"]["tools"]) >= 40
    assert replies[2]["result"] == {}
    assert replies[3]["error"]["code"] == -32601


def test_a_malformed_frame_does_not_end_the_session():
    out = io.StringIO()
    mcp.serve(stdin=io.StringIO('{"not json\n{"jsonrpc":"2.0","id":9,"method":"ping"}\n'),
              stdout=out)
    replies = [json.loads(line) for line in out.getvalue().splitlines() if line.strip()]
    assert [r["id"] for r in replies] == [9]


def test_it_runs_as_a_real_subprocess_over_stdio(tmp_path):
    """Everything above calls serve() in-process; MCP clients spawn a process."""
    request = json.dumps({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}) + "\n"
    proc = subprocess.run(
        [sys.executable, "-m", "numera", "--data-dir", str(tmp_path), "mcp"],
        input=request, capture_output=True, text=True, timeout=120,
        cwd=str(REPO / "engine"),
    )
    assert proc.returncode == 0, proc.stderr
    reply = json.loads(proc.stdout.strip().splitlines()[0])
    assert any(t["name"] == "create" for t in reply["result"]["tools"])
