use super::definitions::*;
use clap::Command;

pub(super) fn pool() -> Command {
    group(
        "pool",
        "The shared proxy pool: import, test and assign proxies",
        vec![
            leaf("list", "Every proxy in the pool"),
            leaf("add", "Import a proxy list").arg(positional(
                "source",
                "A file of proxy lines, or - to read stdin",
            )),
            leaf("remove", "Drop proxies from the pool")
                .arg(positional("id", "One or more proxy ids").num_args(1..)),
            leaf("edit", "Change one proxy in place")
                .arg(positional("id", ""))
                .arg(option("host", "host", ""))
                .arg(option("port", "port", ""))
                .arg(option("user", "user", ""))
                .arg(option("password", "pass", ""))
                .arg(option("type", "type", "").value_parser(["HTTP", "HTTPS", "SOCKS5", "SOCKS4"]))
                .arg(option(
                    "country",
                    "country",
                    "ISO code; aligns locale/timezone on assign",
                ))
                .arg(option(
                    "label",
                    "label",
                    "Free text to tell same-country proxies apart",
                ))
                .arg(option(
                    "rotate_url",
                    "rotate-url",
                    "A rotating proxy's change-IP endpoint; see `pool rotate`",
                )),
            leaf(
                "rotate",
                "Ask a rotating proxy for a new exit IP and prove it moved",
            )
            .arg(positional("id", "One or more proxy ids").num_args(1..)),
            leaf(
                "archive",
                "Retire proxies without deleting them (kept out of assign)",
            )
            .arg(positional("id", "One or more proxy ids").num_args(1..))
            .arg(flag("restore", "restore", "Put them back in rotation")),
            leaf("export", "Write the pool back out as importable lines")
                .arg(
                    positional("id", "Only these ids (default: the whole pool)")
                        .required(false)
                        .num_args(0..),
                )
                .arg(option("file", "file", "Write here instead of stdout")),
            leaf("test", "Route a real request through a proxy")
                .arg(
                    positional("id", "Proxy ids (omit with --all)")
                        .required(false)
                        .num_args(0..),
                )
                .arg(flag("all", "all", "Test the whole pool")),
            leaf(
                "assign",
                "Spread the pool across profiles, aligning locale to country",
            )
            .arg(
                positional("ref", "Profile names or ids")
                    .required(false)
                    .num_args(0..),
            )
            .arg(option("tag", "tag", "Every profile carrying this tag"))
            .arg(flag("all", "all", "Every profile"))
            .arg(repeated(
                "proxy_id",
                "proxy-id",
                "Use only these pool entries (repeatable; default: the whole pool)",
            )),
        ],
    )
}

pub(super) fn tokens() -> Command {
    group(
        "token",
        "Named API tokens for the HTTP surface",
        vec![
            leaf("list", "Every named token (never the value)"),
            leaf("create", "Mint one; shown once, then only hashed").arg(positional(
                "name",
                "What this token is for, so it can be revoked knowingly",
            )),
            leaf("revoke", "Revoke one without touching the rest").arg(positional("id", "")),
        ],
    )
}

pub(super) fn schedule() -> Command {
    group(
        "schedule",
        "Recurring unattended warm-up (add/list/update/remove/run-due)",
        vec![
            leaf("list", "Show all warm-up schedules and when they next run"),
            leaf("add", "Schedule a recurring warm-up for a profile")
                .arg(profile())
                .arg(real("every", "every", "Interval in hours (e.g. 12)").required(true))
                .arg(real("minutes", "minutes", "How long each warm-up runs").default_value("5"))
                .arg(preset("Per-vertical warm-up set (default: general)")),
            leaf("update", "Change an existing schedule")
                .arg(positional("id", ""))
                .arg(real("every", "every", "New interval in hours"))
                .arg(real("minutes", "minutes", "New warm-up length"))
                .arg(preset(""))
                .arg(
                    option("enabled", "enabled", "Pause or resume it").value_parser(["on", "off"]),
                ),
            leaf("remove", "Delete a schedule by id").arg(positional("id", "")),
            leaf(
                "run-due",
                "Run every schedule that's due now (for cron/Task Scheduler)",
            ),
        ],
    )
}

fn extensions() -> Command {
    group(
        "ext",
        "Manage a profile's browser extensions",
        vec![
            leaf("list", "Show the extensions this profile loads").arg(profile()),
            leaf("add", "Load an unpacked extension folder")
                .arg(profile())
                .arg(positional("path", "")),
            leaf("remove", "Stop loading an extension")
                .arg(profile())
                .arg(positional("path", "")),
            leaf(
                "install",
                "Install from a Chrome Web Store URL (downloads and unpacks it)",
            )
            .arg(positional(
                "url",
                "Web Store URL, or the 32-character extension id",
            ))
            .arg(flag("force", "force", "Re-download even if installed"))
            .arg(flag(
                "no_default",
                "no-default",
                "Do not add it to the set new profiles start with",
            )),
            leaf("installed", "List extensions installed from the Web Store"),
            leaf(
                "uninstall",
                "Delete an extension installed from the Web Store",
            )
            .arg(positional("url", "Web Store URL or extension id")),
            leaf(
                "default-list",
                "Show the extensions every NEW profile starts with",
            ),
            leaf(
                "default-add",
                "Add to the set every NEW profile starts with",
            )
            .arg(positional("path", "")),
            leaf("default-remove", "Drop one from that set").arg(positional("path", "")),
        ],
    )
}

pub(super) fn commands() -> Vec<Command> {
    vec![
        extensions(),
        super::profiles::apply(),
        group(
            "vault",
            "Encrypt proxy passwords at rest with a master password",
            vec![
                leaf("status", "Show whether the vault is on, locked or unlocked"),
                leaf("enable", "Turn on encryption and encrypt existing secrets"),
                leaf(
                    "change",
                    "Rotate the master password; re-encrypts every secret",
                )
                .arg(option(
                    "new_password",
                    "new-password",
                    "The new master password (prompted for when omitted)",
                )),
                leaf(
                    "resume",
                    "Finish a master-password rotation that was interrupted",
                )
                .arg(option(
                    "new_password",
                    "new-password",
                    "The password the rotation was moving TO (prompted for when omitted)",
                )),
            ],
        ),
        group(
            "folder",
            "Folders: list, create, rename, delete",
            vec![
                command("list", "List every folder"),
                command("create", "Create a folder").arg(positional("name", "")),
                command("rename", "Rename a folder and move its profiles with it")
                    .arg(positional("old", ""))
                    .arg(positional("new", "")),
                command(
                    "delete",
                    "Delete a folder; its profiles are reassigned, not deleted",
                )
                .arg(positional("name", ""))
                .arg(option(
                    "reassign_to",
                    "reassign-to",
                    "Where its profiles go (default: Unfiled)",
                )),
            ],
        ),
        leaf(
            "mcp",
            "Speak MCP on stdin/stdout, exposing every command as a tool",
        ),
        leaf("serve", "Run Numera (serves the dashboard and the API)")
            .arg(option("host", "host", "").default_value("127.0.0.1"))
            .arg(whole("port", "port", "").default_value("8787"))
            .arg(flag(
                "verbose",
                "verbose",
                "Debug-level detail in the run log (credentials stay redacted)",
            )),
    ]
}
