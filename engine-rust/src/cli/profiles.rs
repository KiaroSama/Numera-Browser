use super::definitions::*;
use clap::Command;

pub(super) fn primary() -> Vec<Command> {
    vec![
        leaf("create", "Create a new profile")
            .arg(positional("name", ""))
            .arg(os()).arg(option("locale", "locale", ""))
            .arg(option("proxy", "proxy", "scheme://[user:pass@]host:port"))
            .arg(option("country", "country", "ISO code to align locale/timezone (e.g. US, PK)"))
            .arg(whole("seed", "seed", "Fixed seed for a reproducible fingerprint"))
            .arg(flag("mobile", "mobile", "Generate a mobile device"))
            .arg(engine("Launch backend: cloak | camoufox | patchright | playwright (default: saved default engine)"))
            .arg(option("notes", "notes", "").default_value(""))
            .arg(repeated("tag", "tag", "Add a tag (repeatable)")),
        leaf("list", "List profiles").arg(option("tag", "tag", "Filter by tag")),
        leaf("show", "Show full profile details").arg(positional("ref", "Profile name or id")),
        leaf("launch", "Launch a profile in a browser").arg(profile())
            .arg(flag("headless", "headless", ""))
            .arg(engine("Override the profile's launch engine for this run")),
        leaf("stop", "End the sessions a profile has running").arg(profile()),
        group("jobs", "Background work started with --detach", vec![
            leaf("list", "Every job and its state"),
            leaf("show", "One job, with the tail of its output").arg(positional("id", ""))
                .arg(whole("lines", "lines", "How much output to show").default_value("20")),
            leaf("stop", "Terminate a job and anything it started").arg(positional("id", "")),
            leaf("clear", "Forget the jobs that have finished"),
        ]),
        leaf("engines", "List available launch engines"),
        leaf("default-engine", "Show or set the engine used for new profiles")
            .arg(positional("name", "Engine to make default (omit to show the current one)").required(false).num_args(0..=1)),
        leaf("config", "Show or change settings (the dashboard's /api/config)")
            .arg(option("default_engine", "default-engine", "Engine new profiles are created with"))
            .arg(option("headless_launch", "headless-launch", "Whether the API launches profiles without a window").value_parser(["on","off"]))
            .arg(repeated("default_extension", "default-extension", "Unpacked extension folder for new profiles (repeatable, replaces the set; pass \"\" to clear)")),
        leaf("edit", "Edit notes / tags / proxy").arg(profile())
            .arg(option("notes", "notes", "")).arg(repeated("add_tag", "add-tag", ""))
            .arg(option("proxy", "proxy", "")).arg(option("country", "country", "")),
        leaf("regen", "Regenerate the fingerprint").arg(profile()).arg(os()).arg(option("locale", "locale", "")),
        leaf("delete", "Delete one or more profiles").arg(profile().num_args(1..))
            .arg(flag("yes", "yes", "Skip confirmation").short('y')),
        leaf("export", "Export a profile to JSON").arg(profile()).arg(positional("file", "")),
        leaf("import", "Import a profile from JSON").arg(positional("file", "")),
        leaf("backup", "Zip a profile + its whole session to a portable file").arg(profile()).arg(positional("file", "Destination .zip")),
        leaf("restore", "Recreate a profile + session from a backup zip")
            .arg(positional("file", "A backup made with `numera backup`"))
            .arg(flag("new_id", "new-id", "Import as a fresh copy instead of overwriting the original id")),
        leaf("collisions", "Find profiles that share a fingerprint (a linkage risk)"),
        leaf("health", "One row per profile: coherence, proxy, collisions, schedule"),
        leaf("import-from", "Import profiles exported from GoLogin / AdsPower / Multilogin")
            .arg(positional("file", "Their exported JSON (a single profile or a list)")),
    ]
}

pub(super) fn bulk() -> Command {
    leaf("bulk-create", "Generate many coherent profiles at once")
        .arg(whole("count", "count", "How many to create").required(true))
        .arg(
            repeated(
                "os",
                "os",
                "OS to spread the batch across (repeatable; default windows)",
            )
            .value_parser(["windows", "macos", "linux", "android"]),
        )
        .arg(repeated(
            "locale",
            "locale",
            "Locale to spread across (repeatable; e.g. --locale en-US --locale de-DE)",
        ))
        .arg(repeated(
            "chrome",
            "chrome",
            "Chrome version to draw from (repeatable; e.g. 128.0.0.0)",
        ))
        .arg(engine("Launch backend for the batch"))
        .arg(option("prefix", "prefix", "Name prefix (default: Profile)").default_value("Profile"))
        .arg(whole("start", "start", "First number in the sequence").default_value("1"))
        .arg(whole("pad", "pad", "Zero-pad the number to this width").default_value("4"))
        .arg(repeated("tag", "tag", "Tag every profile (repeatable)"))
        .arg(flag("mobile", "mobile", "Generate mobile devices"))
        .arg(whole("seed", "seed", "Fixed seed for a reproducible batch"))
}

pub(super) fn apply() -> Command {
    leaf("apply", "Apply one change to many profiles at once")
        .arg(
            positional("ref", "Profile names or ids")
                .required(false)
                .num_args(0..),
        )
        .arg(option("tag", "tag", "Every profile carrying this tag"))
        .arg(flag("all", "all", "Every profile"))
        .arg(engine("Switch them to this launch engine"))
        .arg(option(
            "proxy",
            "proxy",
            "scheme://[user:pass@]host:port  (empty string = go direct)",
        ))
        .arg(option(
            "country",
            "country",
            "ISO code to align locale/timezone with the proxy",
        ))
        .arg(option(
            "locale",
            "locale",
            "Locale (also moves timezone + languages)",
        ))
        .arg(option("timezone", "timezone", ""))
        .arg(option("notes", "notes", ""))
        .arg(repeated(
            "startup_url",
            "startup-url",
            "Startup URL (repeatable, replaces)",
        ))
        .arg(repeated(
            "extension",
            "extension",
            "Unpacked extension folder (repeatable, replaces; pass \"\" to clear)",
        ))
        .arg(repeated("add_tag", "add-tag", ""))
        .arg(repeated("remove_tag", "remove-tag", ""))
        .arg(flag(
            "regen",
            "regen",
            "Give each one a fresh fingerprint (same OS, locale and session)",
        ))
        .arg(flag("yes", "yes", "Skip confirmation").short('y'))
}
