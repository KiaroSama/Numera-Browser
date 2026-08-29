"""File logging for the engine.

A new file per run under ``<data-dir>/logs``, named
``numera_YYYY-MM-DD_HH-mm-ss_UTC.log``. Timestamps are UTC to the second so logs
from a machine that moved timezone -- or a profile deliberately spoofing one --
still sort and correlate.

**Redaction is the part that matters here.** This program's whole job is holding
proxy credentials and browser sessions. A log that leaks a proxy password is
worse than no log, and "we just won't log that" is not a control: the value
arrives inside URLs, inside dict reprs, inside exception messages nobody wrote.
So redaction is a filter on the handler, applied to every record regardless of
which call site produced it, including debug output.
"""

from __future__ import annotations

import logging
import os
import re
import time
from pathlib import Path
from typing import Optional

LOGGER_NAME = "numera"
KEEP_RUNS = 20          # keep the last N run logs; older ones are pruned

# Anything that looks like a credential, wherever it turns up in a message.
# Order matters and is load-bearing: the auth-header rule must run before the
# generic key=value rule, or "Authorization: Bearer <token>" matches the generic
# rule first, which stops at the space and redacts only the word "Bearer" -
# leaving the token itself in the log. That was a real leak caught by a test.
_AUTH_LINE = re.compile(
    r"(?i)(?P<pre>\b(?:proxy-)?authorization\b\s*[=:]\s*)\S.*")
_BEARER = re.compile(r"(?i)(?P<pre>\b(?:bearer|basic)\s+)[A-Za-z0-9._~+/=-]{8,}")
# user:pass@host -- proxy URLs and git-style remotes
_URL_CREDS = re.compile(r"(?P<pre>://)[^/\s:@]+:[^/\s@]+(?P<post>@)")
# key=value / "key": "value". The key may itself be quoted, as it is in JSON -
# an unquoted-only pattern silently missed every {"password": "..."} line.
_KEY_VALUE = re.compile(
    r"""(?ix)
    (?P<pre>
      ["']?\b(?:pass|password|passwd|pwd|token|secret|api[_-]?key)\b["']?
      \s*[=:]\s*
    )
    (?P<q>["']?)
    [^\s,;"'})\]]+
    (?P=q)
    """)


def redact(text: str) -> str:
    """Replace credential-shaped substrings with [REDACTED]."""
    out = _AUTH_LINE.sub(lambda m: f"{m.group('pre')}[REDACTED]", text)
    out = _BEARER.sub(lambda m: f"{m.group('pre')}[REDACTED]", out)
    out = _URL_CREDS.sub(lambda m: f"{m.group('pre')}[REDACTED]{m.group('post')}", out)
    out = _KEY_VALUE.sub(lambda m: f"{m.group('pre')}{m.group('q')}[REDACTED]{m.group('q')}", out)
    return out


class RedactingFilter(logging.Filter):
    """Scrub the formatted message and its arguments.

    On the handler rather than on a logger, so a library logging under its own
    name cannot bypass it."""

    def filter(self, record: logging.LogRecord) -> bool:
        try:
            record.msg = redact(str(record.msg))
            if record.args:
                if isinstance(record.args, dict):
                    record.args = {k: redact(str(v)) for k, v in record.args.items()}
                else:
                    record.args = tuple(redact(str(a)) for a in record.args)
        except Exception:       # a broken record must never silence the log
            pass
        return True


class UTCFormatter(logging.Formatter):
    converter = time.gmtime

    def formatTime(self, record, datefmt=None):
        return time.strftime("%Y-%m-%d %H:%M:%S", self.converter(record.created)) + " UTC"


def log_dir(data_dir: Path) -> Path:
    return Path(data_dir) / "logs"


def _prune(directory: Path, keep: int = KEEP_RUNS) -> None:
    """Drop the oldest run logs. Bounded disk use without a config knob."""
    try:
        files = sorted(directory.glob("numera_*.log"), key=lambda p: p.stat().st_mtime)
        for old in files[:-keep]:
            old.unlink(missing_ok=True)
    except OSError:
        pass


def setup(data_dir: Path, verbose: bool = False) -> Optional[Path]:
    """Attach a file handler for this run. Returns the path, or None if logging
    could not be started.

    A logging failure must never take the application down with it, so this
    reports the problem on stderr and returns None rather than raising -- but it
    also never pretends a file exists that does not."""
    logger = logging.getLogger(LOGGER_NAME)
    logger.setLevel(logging.DEBUG if verbose else logging.INFO)
    logger.propagate = False

    for h in list(logger.handlers):     # idempotent: no duplicate handlers
        if getattr(h, "_numera_run_handler", False):
            return getattr(h, "_numera_path", None)

    directory = log_dir(data_dir)
    try:
        directory.mkdir(parents=True, exist_ok=True)
    except OSError as e:
        print(f"numera: could not create the log directory ({e}); continuing without a log file")
        return None

    stamp = time.strftime("%Y-%m-%d_%H-%M-%S", time.gmtime())
    path = directory / f"numera_{stamp}_UTC.log"
    n = 1
    while path.exists():                # never overwrite a previous run
        path = directory / f"numera_{stamp}_UTC.{n}.log"
        n += 1

    try:
        handler = logging.FileHandler(path, encoding="utf-8")
    except OSError as e:
        print(f"numera: could not open the log file ({e}); continuing without one")
        return None

    handler.setFormatter(UTCFormatter("[%(asctime)s] [%(levelname)s] [%(name)s] %(message)s"))
    handler.addFilter(RedactingFilter())
    handler._numera_run_handler = True  # type: ignore[attr-defined]
    handler._numera_path = path         # type: ignore[attr-defined]
    logger.addHandler(handler)

    _prune(directory)
    logger.info("numera starting  pid=%s  data-dir=%s", os.getpid(), data_dir)
    return path


def get(name: str = "") -> logging.Logger:
    return logging.getLogger(f"{LOGGER_NAME}.{name}" if name else LOGGER_NAME)
