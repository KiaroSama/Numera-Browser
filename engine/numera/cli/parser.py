"""The argparse tree: every command, flag and help string.

It is the single source of truth for the CLI surface. `mcp.py` walks it
to generate its tool schemas, and the parity test walks it to check the
GUI has no capability the CLI lacks - so a command added here becomes an
MCP tool and a parity entry without a second list to keep in step."""

from __future__ import annotations

import argparse
import json
import os
from .. import __version__
from .. import jobs
from ..models import Profile, Proxy
from .browser import cmd_align, cmd_attach, cmd_cookies_export, cmd_cookies_import, cmd_cookies_list, cmd_dns, cmd_jobs, cmd_launch, cmd_proxy_test, cmd_stop, cmd_tls, cmd_trust, cmd_warmup
from .pool import cmd_pool
from .profiles import cmd_apply, cmd_backup, cmd_bulk_create, cmd_check, cmd_collisions, cmd_create, cmd_delete, cmd_edit, cmd_export, cmd_health, cmd_import, cmd_import_from, cmd_list, cmd_regen, cmd_restore, cmd_show
from .setup import (cmd_config, cmd_default_engine, cmd_engines, cmd_ext, cmd_folder,
                    cmd_mcp, cmd_schedule, cmd_serve, cmd_token, cmd_vault)


# ---- parser --------------------------------------------------------------
# The two flags every command carries, defined once and inherited. They live on
# the leaf parsers so their position never matters: `numera trust x --json` is
# what server.py already runs, and `numera launch x --detach` reads the same way.
_AGENT = argparse.ArgumentParser(add_help=False)
_AGENT.add_argument("--json", action="store_true",
                    help="Print the result as JSON on stdout (and nothing else)")
_AGENT.add_argument("--detach", action="store_true",
                    help="Run in the background and print a job handle instead of blocking")


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        prog="numera",
        description="Browser fingerprint & profile manager.",
    )
    p.add_argument("--version", action="version", version=f"numera {__version__}")
    p.add_argument("--data-dir", default=None,
                   help="Override the data directory (default: ~/.numera)")
    p.add_argument("--password", default=None,
                   help="Master password for the secrets vault "
                        "(or set the NUMERA_PASSWORD env var)")
    sub = p.add_subparsers(dest="command", required=True)

    def add(name, **kw):
        return sub.add_parser(name, parents=[_AGENT], **kw)

    c = add("create", help="Create a new profile")
    c.add_argument("name")
    c.add_argument("--os", choices=["windows", "macos", "linux", "android"])
    c.add_argument("--locale")
    c.add_argument("--proxy", help="scheme://[user:pass@]host:port")
    c.add_argument("--country", help="ISO code to align locale/timezone (e.g. US, PK)")
    c.add_argument("--seed", type=int, help="Fixed seed for a reproducible fingerprint")
    c.add_argument("--mobile", action="store_true", help="Generate a mobile device")
    c.add_argument("--engine", help="Launch backend: cloak | camoufox | patchright | playwright "
                                    "(default: saved default engine)")
    c.add_argument("--notes", default="")
    c.add_argument("--tag", action="append", help="Add a tag (repeatable)")
    c.set_defaults(func=cmd_create)

    c = add("list", help="List profiles")
    c.add_argument("--tag", help="Filter by tag")
    c.set_defaults(func=cmd_list)

    c = add("show", help="Show full profile details")
    c.add_argument("ref", help="Profile name or id")
    c.set_defaults(func=cmd_show)

    c = add("launch", help="Launch a profile in a browser")
    c.add_argument("ref")
    c.add_argument("--headless", action="store_true")
    c.add_argument("--engine", help="Override the profile's launch engine for this run")
    c.set_defaults(func=cmd_launch)

    c = add("stop", help="End the sessions a profile has running")
    c.add_argument("ref")
    c.set_defaults(func=cmd_stop)

    c = add("jobs", help="Background work started with --detach")
    jb = c.add_subparsers(dest="jobs_command", required=True)
    s = jb.add_parser("list", parents=[_AGENT], help="Every job and its state")
    s.set_defaults(func=cmd_jobs)
    s = jb.add_parser("show", parents=[_AGENT], help="One job, with the tail of its output")
    s.add_argument("id")
    s.add_argument("--lines", type=int, default=20, help="How much output to show")
    s.set_defaults(func=cmd_jobs)
    s = jb.add_parser("stop", parents=[_AGENT], help="Terminate a job and anything it started")
    s.add_argument("id")
    s.set_defaults(func=cmd_jobs)
    s = jb.add_parser("clear", parents=[_AGENT], help="Forget the jobs that have finished")
    s.set_defaults(func=cmd_jobs)

    c = add("engines", help="List available launch engines")
    c.add_argument('--cloak-offer', action='store_true',
                   help='Show the local Cloak acquisition target without downloading')
    c.add_argument('--acquire-cloak', metavar='OFFER_ID',
                   help='Acquire only the exact Cloak offer previously shown')
    c.add_argument('--approve', action='store_true',
                   help='Explicitly approve this exact browser acquisition')
    c.set_defaults(func=cmd_engines)

    c = add("default-engine",
            help="Show or set the engine used for new profiles")
    c.add_argument("name", nargs="?",
                   help="Engine to make default (omit to show the current one)")
    c.set_defaults(func=cmd_default_engine)

    c = add("config", help="Show or change settings (the dashboard's /api/config)")
    c.add_argument("--default-engine", help="Engine new profiles are created with")
    c.add_argument("--headless-launch", choices=["on", "off"],
                   help="Whether the API launches profiles without a window")
    c.add_argument("--default-extension", action="append",
                   help="Unpacked extension folder for new profiles "
                        "(repeatable, replaces the set; pass \"\" to clear)")
    c.set_defaults(func=cmd_config)

    c = add("edit", help="Edit notes / tags / proxy")
    c.add_argument("ref")
    c.add_argument("--notes")
    c.add_argument("--add-tag", action="append")
    c.add_argument("--proxy")
    c.add_argument("--country")
    c.set_defaults(func=cmd_edit)

    c = add("regen", help="Regenerate the fingerprint")
    c.add_argument("ref")
    c.add_argument("--os", choices=["windows", "macos", "linux", "android"])
    c.add_argument("--locale")
    c.set_defaults(func=cmd_regen)

    c = add("delete", help="Delete one or more profiles")
    # Several refs, matching `pool remove`. The dashboard deletes a whole
    # selection in one call, and an agent driving the CLI had no equivalent.
    c.add_argument("ref", nargs="+")
    c.add_argument("-y", "--yes", action="store_true", help="Skip confirmation")
    c.set_defaults(func=cmd_delete)

    c = add("export", help="Export a profile to JSON")
    c.add_argument("ref"); c.add_argument("file")
    c.set_defaults(func=cmd_export)

    c = add("import", help="Import a profile from JSON")
    c.add_argument("file")
    c.set_defaults(func=cmd_import)

    c = add("backup", help="Zip a profile + its whole session to a portable file")
    c.add_argument("ref"); c.add_argument("file", help="Destination .zip")
    c.set_defaults(func=cmd_backup)

    c = add("restore", help="Recreate a profile + session from a backup zip")
    c.add_argument("file", help="A backup made with `numera backup`")
    c.add_argument("--new-id", action="store_true",
                   help="Import as a fresh copy instead of overwriting the original id")
    c.set_defaults(func=cmd_restore)

    c = add("collisions",
            help="Find profiles that share a fingerprint (a linkage risk)")
    c.set_defaults(func=cmd_collisions)

    c = add("health", help="One row per profile: coherence, proxy, collisions, schedule")
    c.set_defaults(func=cmd_health)

    c = add("import-from",
            help="Import profiles exported from GoLogin / AdsPower / Multilogin")
    c.add_argument("file", help="Their exported JSON (a single profile or a list)")
    c.set_defaults(func=cmd_import_from)

    c = add("cookies", help="List, export or import a profile's cookies")
    ck = c.add_subparsers(dest="cookies_command", required=True)

    s = ck.add_parser("list", parents=[_AGENT], help="Show what's in the profile's cookie jar")
    s.add_argument("ref")
    s.add_argument("--engine", help="Open with a different engine than the profile's")
    s.set_defaults(func=cmd_cookies_list)

    s = ck.add_parser("export", parents=[_AGENT], help="Write the profile's cookies to a file")
    s.add_argument("ref"); s.add_argument("file")
    s.add_argument("--format", choices=["json", "netscape"],
                   help="Output format (default: netscape for .txt, else json)")
    s.add_argument("--engine", help="Open with a different engine than the profile's")
    s.set_defaults(func=cmd_cookies_export)

    s = ck.add_parser("import", parents=[_AGENT], help="Load cookies from a file into the profile")
    s.add_argument("ref"); s.add_argument("file")
    s.add_argument("--clear", action="store_true",
                   help="Empty the existing jar first (replace instead of merge)")
    s.add_argument("--engine", help="Open with a different engine than the profile's")
    s.set_defaults(func=cmd_cookies_import)

    c = add("check", help="Validate fingerprint coherence")
    c.add_argument("ref")
    c.set_defaults(func=cmd_check)

    c = add("proxy", help="Proxy tools")
    px = c.add_subparsers(dest="proxy_command", required=True)
    s = px.add_parser("test", parents=[_AGENT],
                      help="Check the exit IP, country and WebRTC leaks")
    s.add_argument("ref")
    s.add_argument("--engine", help="Test with a different engine than the profile's")
    s.set_defaults(func=cmd_proxy_test)

    c = add("pool", help="The shared proxy pool: import, test and assign proxies")
    pl = c.add_subparsers(dest="pool_command", required=True)
    s = pl.add_parser("list", parents=[_AGENT], help="Every proxy in the pool")
    s.set_defaults(func=cmd_pool)
    s = pl.add_parser("add", parents=[_AGENT], help="Import a proxy list")
    s.add_argument("source", help="A file of proxy lines, or - to read stdin")
    s.set_defaults(func=cmd_pool)
    s = pl.add_parser("remove", parents=[_AGENT], help="Drop proxies from the pool")
    s.add_argument("id", nargs="+", help="One or more proxy ids")
    s.set_defaults(func=cmd_pool)
    s = pl.add_parser("edit", parents=[_AGENT], help="Change one proxy in place")
    s.add_argument("id")
    s.add_argument("--host")
    s.add_argument("--port")
    s.add_argument("--user")
    s.add_argument("--pass", dest="password")
    s.add_argument("--type", choices=["HTTP", "HTTPS", "SOCKS5", "SOCKS4"])
    s.add_argument("--country", help="ISO code; aligns locale/timezone on assign")
    s.add_argument("--label", help="Free text to tell same-country proxies apart")
    s.add_argument("--rotate-url", dest="rotate_url",
                   help="A rotating proxy's change-IP endpoint; see `pool rotate`")
    s.set_defaults(func=cmd_pool)
    s = pl.add_parser("rotate", parents=[_AGENT],
                      help="Ask a rotating proxy for a new exit IP and prove it moved")
    s.add_argument("id", nargs="+", help="One or more proxy ids")
    s.set_defaults(func=cmd_pool)
    s = pl.add_parser("archive", parents=[_AGENT],
                      help="Retire proxies without deleting them (kept out of assign)")
    s.add_argument("id", nargs="+", help="One or more proxy ids")
    s.add_argument("--restore", action="store_true", help="Put them back in rotation")
    s.set_defaults(func=cmd_pool)
    s = pl.add_parser("export", parents=[_AGENT],
                      help="Write the pool back out as importable lines")
    s.add_argument("id", nargs="*", help="Only these ids (default: the whole pool)")
    s.add_argument("--file", help="Write here instead of stdout")
    s.set_defaults(func=cmd_pool)
    s = pl.add_parser("test", parents=[_AGENT], help="Route a real request through a proxy")
    s.add_argument("id", nargs="*", help="Proxy ids (omit with --all)")
    s.add_argument("--all", action="store_true", help="Test the whole pool")
    s.set_defaults(func=cmd_pool)
    s = pl.add_parser("assign", parents=[_AGENT],
                      help="Spread the pool across profiles, aligning locale to country")
    s.add_argument("ref", nargs="*", help="Profile names or ids")
    s.add_argument("--tag", help="Every profile carrying this tag")
    s.add_argument("--all", action="store_true", help="Every profile")
    s.add_argument("--proxy-id", action="append",
                   help="Use only these pool entries (repeatable; default: the whole pool)")
    s.set_defaults(func=cmd_pool)

    c = add("token", help="Named API tokens for the HTTP surface")
    tk = c.add_subparsers(dest="token_command", required=True)
    t = tk.add_parser("list", parents=[_AGENT], help="Every named token (never the value)")
    t.set_defaults(func=cmd_token)
    t = tk.add_parser("create", parents=[_AGENT], help="Mint one; shown once, then only hashed")
    t.add_argument("name", help="What this token is for, so it can be revoked knowingly")
    t.set_defaults(func=cmd_token)
    t = tk.add_parser("revoke", parents=[_AGENT], help="Revoke one without touching the rest")
    t.add_argument("id")
    t.set_defaults(func=cmd_token)

    c = add("tls", help="Show the profile's TLS/JA3 handshake fingerprint")
    c.add_argument("ref")
    c.add_argument("--engine", help="Read with a different engine than the profile's")
    c.set_defaults(func=cmd_tls)

    c = add("dns", help="Check whether DNS lookups leak past the proxy")
    c.add_argument("ref")
    c.add_argument("--engine", help="Test with a different engine than the profile's")
    c.set_defaults(func=cmd_dns)

    c = add("trust", help="Score the profile the way a fingerprinter would")
    c.add_argument("ref")
    c.add_argument("--engine", help="Inspect with a different engine than the profile's")
    c.set_defaults(func=cmd_trust)

    c = add("align",
            help="Auto-adjust: match the profile to what its browser really shows")
    c.add_argument("ref")
    c.add_argument("--engine", help="Align against a different engine than the profile's")
    c.set_defaults(func=cmd_align)

    c = add("bulk-create", help="Generate many coherent profiles at once")
    c.add_argument("--count", type=int, required=True, help="How many to create")
    c.add_argument("--os", action="append", choices=["windows", "macos", "linux", "android"],
                   help="OS to spread the batch across (repeatable; default windows)")
    c.add_argument("--locale", action="append",
                   help="Locale to spread across (repeatable; e.g. --locale en-US --locale de-DE)")
    c.add_argument("--chrome", action="append",
                   help="Chrome version to draw from (repeatable; e.g. 128.0.0.0)")
    c.add_argument("--engine", help="Launch backend for the batch")
    c.add_argument("--prefix", default="Profile", help="Name prefix (default: Profile)")
    c.add_argument("--start", type=int, default=1, help="First number in the sequence")
    c.add_argument("--pad", type=int, default=4, help="Zero-pad the number to this width")
    c.add_argument("--tag", action="append", help="Tag every profile (repeatable)")
    c.add_argument("--mobile", action="store_true", help="Generate mobile devices")
    c.add_argument("--seed", type=int, help="Fixed seed for a reproducible batch")
    c.set_defaults(func=cmd_bulk_create)

    c = add("warmup", help="Browse normally for a while to age the profile")
    c.add_argument("ref")
    c.add_argument("--minutes", type=float, default=5, help="How long to browse (default: 5)")
    c.add_argument("--site", action="append",
                   help="Site to visit (repeatable; replaces the preset/default list)")
    c.add_argument("--preset", choices=["general", "ads", "ecommerce", "crypto", "social", "news"],
                   help="Per-vertical warm-up set to browse (default: general)")
    c.add_argument("--headless", action="store_true")
    c.add_argument("--engine")
    c.set_defaults(func=cmd_warmup)

    c = add("schedule", help="Recurring unattended warm-up (add/list/update/remove/run-due)")
    sc = c.add_subparsers(dest="schedule_command", required=True)
    s = sc.add_parser("list", parents=[_AGENT],
                      help="Show all warm-up schedules and when they next run")
    s.set_defaults(func=cmd_schedule)
    s = sc.add_parser("add", parents=[_AGENT], help="Schedule a recurring warm-up for a profile")
    s.add_argument("ref")
    s.add_argument("--every", type=float, required=True, help="Interval in hours (e.g. 12)")
    s.add_argument("--minutes", type=float, default=5, help="How long each warm-up runs")
    s.add_argument("--preset", choices=["general", "ads", "ecommerce", "crypto", "social", "news"],
                   help="Per-vertical warm-up set (default: general)")
    s.set_defaults(func=cmd_schedule)
    s = sc.add_parser("update", parents=[_AGENT], help="Change an existing schedule")
    s.add_argument("id")
    s.add_argument("--every", type=float, help="New interval in hours")
    s.add_argument("--minutes", type=float, help="New warm-up length")
    s.add_argument("--preset", choices=["general", "ads", "ecommerce", "crypto", "social", "news"])
    s.add_argument("--enabled", choices=["on", "off"], help="Pause or resume it")
    s.set_defaults(func=cmd_schedule)
    s = sc.add_parser("remove", parents=[_AGENT], help="Delete a schedule by id")
    s.add_argument("id")
    s.set_defaults(func=cmd_schedule)
    s = sc.add_parser("run-due", parents=[_AGENT],
                      help="Run every schedule that's due now (for cron/Task Scheduler)")
    s.set_defaults(func=cmd_schedule)

    c = add("attach", help="Open the profile with DevTools for your own automation")
    c.add_argument("ref")
    c.add_argument("--port", type=int, default=9222, help="DevTools port (default: 9222)")
    c.add_argument("--engine")
    c.add_argument("--headless", action="store_true",
                   help="Attach without a visible window (for servers/CI)")
    c.set_defaults(func=cmd_attach)

    c = add("ext", help="Manage a profile's browser extensions")
    ex = c.add_subparsers(dest="ext_command", required=True)
    s = ex.add_parser("list", parents=[_AGENT], help="Show the extensions this profile loads")
    s.add_argument("ref")
    s.set_defaults(func=cmd_ext)
    for verb, helptext in (("add", "Load an unpacked extension folder"),
                           ("remove", "Stop loading an extension")):
        s = ex.add_parser(verb, parents=[_AGENT], help=helptext)
        s.add_argument("ref"); s.add_argument("path")
        s.set_defaults(func=cmd_ext)
    s = ex.add_parser("install", parents=[_AGENT],
                      help="Install from a Chrome Web Store URL (downloads and unpacks it)")
    s.add_argument("url", help="Web Store URL, or the 32-character extension id")
    s.add_argument("--force", action="store_true", help="Re-download even if installed")
    s.add_argument("--no-default", action="store_true",
                   help="Do not add it to the set new profiles start with")
    s.set_defaults(func=cmd_ext)
    s = ex.add_parser("installed", parents=[_AGENT],
                      help="List extensions installed from the Web Store")
    s.set_defaults(func=cmd_ext)
    s = ex.add_parser("uninstall", parents=[_AGENT],
                      help="Delete an extension installed from the Web Store")
    s.add_argument("url", help="Web Store URL or extension id")
    s.set_defaults(func=cmd_ext)

    s = ex.add_parser("default-list", parents=[_AGENT],
                      help="Show the extensions every NEW profile starts with")
    s.set_defaults(func=cmd_ext)
    for verb, helptext in (("default-add", "Add to the set every NEW profile starts with"),
                           ("default-remove", "Drop one from that set")):
        s = ex.add_parser(verb, parents=[_AGENT], help=helptext)
        s.add_argument("path")
        s.set_defaults(func=cmd_ext)

    c = add("apply", help="Apply one change to many profiles at once")
    c.add_argument("ref", nargs="*", help="Profile names or ids")
    c.add_argument("--tag", help="Every profile carrying this tag")
    c.add_argument("--all", action="store_true", help="Every profile")
    c.add_argument("--engine", help="Switch them to this launch engine")
    c.add_argument("--proxy", help="scheme://[user:pass@]host:port  (empty string = go direct)")
    c.add_argument("--country", help="ISO code to align locale/timezone with the proxy")
    c.add_argument("--locale", help="Locale (also moves timezone + languages)")
    c.add_argument("--timezone")
    c.add_argument("--notes")
    c.add_argument("--startup-url", action="append", help="Startup URL (repeatable, replaces)")
    c.add_argument("--extension", action="append",
                   help="Unpacked extension folder (repeatable, replaces; pass \"\" to clear)")
    c.add_argument("--add-tag", action="append")
    c.add_argument("--remove-tag", action="append")
    c.add_argument("--regen", action="store_true",
                   help="Give each one a fresh fingerprint (same OS, locale and session)")
    c.add_argument("-y", "--yes", action="store_true", help="Skip confirmation")
    c.set_defaults(func=cmd_apply)

    c = add("vault", help="Encrypt proxy passwords at rest with a master password")
    vt = c.add_subparsers(dest="vault_command", required=True)
    vt.add_parser("status", parents=[_AGENT],
                  help="Show whether the vault is on, locked or unlocked").set_defaults(func=cmd_vault)
    vt.add_parser("enable", parents=[_AGENT],
                  help="Turn on encryption and encrypt existing secrets").set_defaults(func=cmd_vault)
    v = vt.add_parser("change", parents=[_AGENT],
                      help="Rotate the master password; re-encrypts every secret")
    v.add_argument("--new-password", dest="new_password", default=None,
                   help="The new master password (prompted for when omitted)")
    v.set_defaults(func=cmd_vault)
    # The way out of an interrupted rotation. It needs both passwords because
    # both keys are on disk as ciphertext and nothing but a trial decryption
    # tells an already-moved secret from a waiting one - so this cannot be
    # folded into the ordinary `--password`.
    r = vt.add_parser("resume", parents=[_AGENT],
                      help="Finish a master-password rotation that was interrupted")
    r.add_argument("--new-password", dest="new_password", default=None,
                   help="The password the rotation was moving TO (prompted for "
                        "when omitted)")
    r.set_defaults(func=cmd_vault)

    c = add("folder", help="Folders: list, create, rename, delete")
    fs = c.add_subparsers(dest="folder_command", required=True)
    fs.add_parser("list", parents=[_AGENT], help="List every folder")
    f = fs.add_parser("create", parents=[_AGENT], help="Create a folder")
    f.add_argument("name")
    f = fs.add_parser("rename", parents=[_AGENT],
                      help="Rename a folder and move its profiles with it")
    f.add_argument("old")
    f.add_argument("new")
    f = fs.add_parser("delete", parents=[_AGENT],
                      help="Delete a folder; its profiles are reassigned, not deleted")
    f.add_argument("name")
    f.add_argument("--reassign-to", default=None,
                   help="Where its profiles go (default: Unfiled)")
    c.set_defaults(func=cmd_folder)

    c = add("mcp", help="Speak MCP on stdin/stdout, exposing every command as a tool")
    c.set_defaults(func=cmd_mcp)

    c = add("serve", help="Run Numera (serves the dashboard and the API)")
    c.add_argument("--host", default="127.0.0.1")
    c.add_argument("--port", type=int, default=8787)
    c.add_argument("--verbose", action="store_true",
                   help="Debug-level detail in the run log (credentials stay redacted)")
    c.set_defaults(func=cmd_serve)

    return p
