"""Terminal output and the small helpers every command shares.

`emit` is the one that matters: under --json it owns stdout and prints
the payload alone, so a caller parsing the output never has to strip
human text out of it. `say` writes the human half and goes quiet in that
mode. Keeping both here is what stops a new command inventing a third
way to talk."""

from __future__ import annotations

import json
import os
import sys
from typing import Optional
from ..models import Profile, Proxy
from ..store import ProfileStore


# ---- output -------------------------------------------------------------
# One process, two audiences. `say` is for the person at the prompt; `emit` is
# the result an agent parses. Under --json only `emit` writes to stdout, so the
# stream holds exactly one JSON object and nothing else.
_JSON = False


def json_mode() -> bool:
    """Whether --json was given.

    Read through a function, never imported as a value: `from .out import _JSON`
    binds a copy at import time and would never see main() set it, which is the
    kind of break that shows up as --json quietly printing human text again.
    """
    return _JSON


def set_json_mode(on: bool) -> None:
    global _JSON
    _JSON = bool(on)


class C:
    B = "\033[1m"; DIM = "\033[2m"; GRN = "\033[32m"; RED = "\033[31m"
    YEL = "\033[33m"; CYN = "\033[36m"; END = "\033[0m"


def _c(text: str, color: str) -> str:
    return f"{color}{text}{C.END}" if sys.stdout.isatty() else text


def say(*args, **kwargs) -> None:
    """Human narration. Silent under --json."""
    if not json_mode():
        print(*args, **kwargs)


def emit(payload, code: int = 0) -> int:
    """The command's machine-readable result. Printed only under --json."""
    if json_mode():
        print(json.dumps(payload, default=str))
    return code


def _confirm(prompt: str) -> bool:
    """Ask before something destructive.

    A --json caller has no keyboard, so a missing ``-y`` is an error there
    rather than a prompt nobody will ever answer.
    """
    if json_mode():
        raise RuntimeError("this needs confirmation — pass -y "
                           "(a --json run never prompts)")
    try:
        return input(prompt).strip().lower() == "y"
    except EOFError:
        # A detached job has no stdin either. Say so, rather than traceback.
        raise RuntimeError("nothing to read the confirmation from — pass -y") from None


def _parse_proxy(raw: Optional[str], country: Optional[str]) -> Optional[Proxy]:
    """Accept 'scheme://[user:pass@]host:port' and return a Proxy."""
    if not raw:
        return None
    server, user, pw = raw, None, None
    if "@" in raw:
        scheme, rest = raw.split("://", 1) if "://" in raw else ("http", raw)
        creds, hostport = rest.rsplit("@", 1)
        if ":" in creds:
            user, pw = creds.split(":", 1)
        server = f"{scheme}://{hostport}"
    return Proxy(server=server, username=user, password=pw, country=country)


def _row(prof: Profile) -> dict:
    """The summary of a profile that `list` and `health` report.

    Deliberately not ``to_dict()``: that carries the proxy password, and a
    fleet-wide listing is the last place it should turn up. ``show`` and
    ``export`` are the full-fidelity paths.
    """
    fp = prof.fingerprint
    return {
        "id": prof.id, "name": prof.name, "engine": prof.engine,
        "os": fp.os, "locale": fp.locale, "timezone": fp.timezone,
        "mobile": fp.is_mobile, "tags": list(prof.tags),
        "proxy": prof.proxy.server if prof.proxy else None,
        "proxyCountry": prof.proxy.country if prof.proxy else None,
    }


def _no_profile(ref: str) -> int:
    say(_c(f"No profile matching '{ref}'.", C.RED))
    return emit({"ok": False, "error": f"no profile matching '{ref}'"}, 1)


def _resolve_or_fail(args, store: ProfileStore) -> Optional[Profile]:
    prof = store.resolve(args.ref)
    if not prof:
        say(_c(f"No profile matching '{args.ref}'.", C.RED))
    return prof


def _print_checks(checks: list[dict]) -> None:
    for c in checks:
        mark = _c("PASS", C.GRN) if c["ok"] else _c("FAIL", C.RED)
        say(f"  [{mark}] {c['name']}")
        if c.get("detail") and not c["ok"]:
            say(f"         {_c(c['detail'], C.DIM)}")


# ---- rendering -----------------------------------------------------------
def _print_summary(prof: Profile, full: bool = False) -> None:
    fp = prof.fingerprint
    rows = [
        ("id", prof.id),
        ("engine", prof.engine),
        ("os", fp.os + (" (mobile)" if fp.is_mobile else "")),
        ("chrome", fp.chrome_version),
        ("screen", f"{fp.screen_width}x{fp.screen_height}"),
        ("cpu / ram", f"{fp.hardware_concurrency} cores / {fp.device_memory} GB"),
        ("gpu", fp.webgl_renderer),
        ("locale / tz", f"{fp.locale}  {fp.timezone}"),
    ]
    if full:
        rows.insert(2, ("user-agent", fp.user_agent))
        rows.append(("languages", ", ".join(fp.languages)))
        rows.append(("proxy", prof.proxy.server if prof.proxy else "-"))
        rows.append(("tags", ", ".join(prof.tags) or "-"))
        rows.append(("notes", prof.notes or "-"))
        rows.append(("seed", str(fp.seed)))
    for k, v in rows:
        say(f"  {_c(k + ':', C.DIM):<24}{v}")
