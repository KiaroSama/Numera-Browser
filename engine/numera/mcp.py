"""
An MCP server over stdio: every CLI command, as a tool.

The tool list is *derived from the argparse parser*, not written out beside it.
A hand-kept copy would be one more surface to drift out of sync with the CLI —
and this project's first lesson is that a control which looks present and does
nothing is the failure mode to design against. Adding a command to ``cli.py``
adds a tool here; changing a flag changes its schema.

No SDK: MCP's stdio transport is newline-delimited JSON-RPC 2.0, which ``json``
already speaks. Run it with ``numera mcp``.

Each call shells out to ``python -m numera <command> … --json``, so a tool gets
the same structured answer a script does, and a crashing command cannot take
the server down with it. Anything that opens a browser should be called with
``detach: true``; it returns a job handle immediately and ``jobs_show`` polls it.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from typing import Any, Iterator, Optional

from . import __version__, cli

# Bounded, because a tool call that never returns is indistinguishable from a
# hung server. Anything slower than this is what `detach` is for.
CALL_TIMEOUT = 120.0

# The protocol revision to fall back on when a client does not name one.
DEFAULT_PROTOCOL = "2024-11-05"

# Flags this module supplies itself, so they never appear in a tool's schema.
_MANAGED = {"--json", "--help"}

_JSON_TYPE = {int: "integer", float: "number"}


def _schema_for(parser: argparse.ArgumentParser) -> dict:
    """A JSON Schema for one command, read off its own argparse actions."""
    props: dict[str, dict] = {}
    required: list[str] = []
    for action in parser._actions:
        if isinstance(action, argparse._SubParsersAction):
            continue
        if any(opt in _MANAGED for opt in action.option_strings):
            continue
        if isinstance(action, argparse._HelpAction):
            continue

        if not action.option_strings:                       # positional
            name = action.dest
            if action.nargs in ("*", "+"):
                prop = {"type": "array", "items": {"type": "string"}}
            else:
                prop = {"type": _JSON_TYPE.get(action.type, "string")}
                if action.nargs != "?":
                    required.append(name)
        elif isinstance(action, (argparse._StoreTrueAction, argparse._StoreFalseAction)):
            name = action.dest
            prop = {"type": "boolean"}
        elif isinstance(action, argparse._AppendAction):
            name = action.dest
            prop = {"type": "array", "items": {"type": "string"}}
        else:
            name = action.dest
            prop = {"type": _JSON_TYPE.get(action.type, "string")}

        if action.choices:
            prop["enum"] = list(action.choices)
        if action.help:
            prop["description"] = action.help
        props[name] = prop

    return {"type": "object", "properties": props, "required": required}


def _walk(parser: argparse.ArgumentParser, prefix: tuple[str, ...], helps: dict
          ) -> Iterator[tuple[tuple[str, ...], argparse.ArgumentParser, str]]:
    subs = [a for a in parser._actions if isinstance(a, argparse._SubParsersAction)]
    if not subs:
        if prefix:
            yield prefix, parser, helps.get(prefix[-1], "")
        return
    for action in subs:
        # add_parser(help=...) hangs the text on a pseudo-action, not the parser.
        texts = {ca.dest: (ca.help or "") for ca in action._choices_actions}
        for name, sub in action.choices.items():
            yield from _walk(sub, (*prefix, name), texts)


# Commands that are servers or need a terminal. Exposing them as tools would
# hand a caller something that blocks until the timeout kills it.
_NOT_TOOLS = {("serve",), ("mcp",)}


def tools() -> list[dict]:
    """Every CLI command as an MCP tool definition."""
    out = []
    for path, parser, help_text in _walk(cli.build_parser(), (), {}):
        if path in _NOT_TOOLS:
            continue
        schema = _schema_for(parser)
        schema["properties"]["detach"] = {
            "type": "boolean",
            "description": "Run in the background and return a job handle instead of "
                           "the result. Use it for anything that opens a browser, then "
                           "poll with jobs_show.",
        }
        out.append({
            "name": "_".join(path).replace("-", "_"),
            "description": help_text or f"numera {' '.join(path)}",
            "inputSchema": schema,
        })
    return out


def _argv(path: tuple[str, ...], parser: argparse.ArgumentParser,
          arguments: dict) -> list[str]:
    """Turn a tool call's arguments back into a command line."""
    positional, flags = [], []
    for action in parser._actions:
        if isinstance(action, (argparse._SubParsersAction, argparse._HelpAction)):
            continue
        if any(opt in _MANAGED for opt in action.option_strings):
            continue
        if action.dest not in arguments or arguments[action.dest] is None:
            continue
        value = arguments[action.dest]
        if not action.option_strings:
            if isinstance(value, list):
                positional.extend(str(v) for v in value)
            else:
                positional.append(str(value))
            continue
        opt = action.option_strings[-1]
        if isinstance(action, (argparse._StoreTrueAction, argparse._StoreFalseAction)):
            if value:
                flags.append(opt)
        elif isinstance(value, list):
            for v in value:
                flags += [opt, str(v)]
        else:
            flags += [opt, str(value)]
    # `--detach` needs no special case here: it is a real flag on every command,
    # so the loop above already emitted it. tools() only rewrites its wording.
    return [*path, *positional, *flags, "--json"]


def call(name: str, arguments: dict, data_dir: Optional[str] = None) -> dict:
    """Run one tool. Returns an MCP tool result."""
    commands = {"_".join(p).replace("-", "_"): (p, parser)
                for p, parser, _ in _walk(cli.build_parser(), (), {})
                if p not in _NOT_TOOLS}
    found = commands.get(name)
    if not found:
        return _result({"error": f"no such tool '{name}'"}, is_error=True)
    path, parser = found

    cmd = [sys.executable, "-m", "numera"]
    if data_dir:
        cmd += ["--data-dir", str(data_dir)]
    cmd += _argv(path, parser, arguments or {})
    try:
        proc = subprocess.run(cmd, capture_output=True, text=True, timeout=CALL_TIMEOUT)
    except subprocess.TimeoutExpired:
        return _result({"error": f"`numera {' '.join(path)}` did not finish within "
                                 f"{CALL_TIMEOUT:.0f}s — call it again with detach: true"},
                       is_error=True)
    try:
        payload = json.loads(proc.stdout)
    except json.JSONDecodeError:
        return _result({"error": (proc.stderr or proc.stdout).strip() or "command failed",
                        "exit": proc.returncode}, is_error=True)
    # A non-zero exit is a verdict, not a crash: `check` says the fingerprint is
    # incoherent, `proxy test` says the proxy leaks. The payload holds the detail,
    # so it is returned as a normal result with the code alongside.
    return _result(payload, is_error=False, exit_code=proc.returncode)


def _result(payload: Any, is_error: bool = False, exit_code: int = 0) -> dict:
    body = {"content": [{"type": "text", "text": json.dumps(payload, default=str)}],
            "isError": is_error}
    if not is_error:
        body["structuredContent"] = payload if isinstance(payload, dict) else {"result": payload}
        body["_meta"] = {"exitCode": exit_code}
    return body


def handle(message: dict, data_dir: Optional[str] = None) -> Optional[dict]:
    """One JSON-RPC message in, at most one response out (None for a notification)."""
    method = message.get("method")
    mid = message.get("id")
    params = message.get("params") or {}

    if method == "initialize":
        # Echo the revision the client asked for when we understand the shape of
        # it; every revision so far uses the same three methods below.
        return _ok(mid, {
            "protocolVersion": params.get("protocolVersion") or DEFAULT_PROTOCOL,
            "capabilities": {"tools": {"listChanged": False}},
            "serverInfo": {"name": "numera", "version": __version__},
        })
    if method == "tools/list":
        return _ok(mid, {"tools": tools()})
    if method == "tools/call":
        return _ok(mid, call(params.get("name", ""), params.get("arguments") or {}, data_dir))
    if method == "ping":
        return _ok(mid, {})
    if mid is None:
        return None                      # a notification we do not act on
    return {"jsonrpc": "2.0", "id": mid,
            "error": {"code": -32601, "message": f"unknown method '{method}'"}}


def _ok(mid, result) -> dict:
    return {"jsonrpc": "2.0", "id": mid, "result": result}


def serve(stdin=None, stdout=None, data_dir: Optional[str] = None) -> int:
    """The stdio loop: newline-delimited JSON-RPC, until stdin closes."""
    stdin = stdin or sys.stdin
    stdout = stdout or sys.stdout
    for line in stdin:
        line = line.strip()
        if not line:
            continue
        try:
            message = json.loads(line)
        except json.JSONDecodeError:
            # No id to answer with, so there is nobody to tell. Skip it rather
            # than kill a session over one malformed frame.
            continue
        response = handle(message, data_dir)
        if response is not None:
            stdout.write(json.dumps(response) + "\n")
            stdout.flush()
    return 0
