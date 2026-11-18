use super::definitions::*;
use clap::Command;

pub(super) fn commands() -> Vec<Command> {
    let mut commands = vec![
        group(
            "cookies",
            "List, export or import a profile's cookies",
            vec![
                leaf("list", "Show what's in the profile's cookie jar")
                    .arg(profile())
                    .arg(engine("Open with a different engine than the profile's")),
                leaf("export", "Write the profile's cookies to a file")
                    .arg(profile())
                    .arg(positional("file", ""))
                    .arg(
                        option(
                            "format",
                            "format",
                            "Output format (default: netscape for .txt, else json)",
                        )
                        .value_parser(["json", "netscape"]),
                    )
                    .arg(engine("Open with a different engine than the profile's")),
                leaf("import", "Load cookies from a file into the profile")
                    .arg(profile())
                    .arg(positional("file", ""))
                    .arg(flag(
                        "clear",
                        "clear",
                        "Empty the existing jar first (replace instead of merge)",
                    ))
                    .arg(engine("Open with a different engine than the profile's")),
            ],
        ),
        leaf("check", "Validate fingerprint coherence").arg(profile()),
        group(
            "proxy",
            "Proxy tools",
            vec![
                leaf("test", "Check the exit IP, country and WebRTC leaks")
                    .arg(profile())
                    .arg(engine("Test with a different engine than the profile's")),
            ],
        ),
        super::administration::pool(),
        super::administration::tokens(),
    ];
    for (name, help, engine_help) in [
        (
            "tls",
            "Show the profile's TLS/JA3 handshake fingerprint",
            "Read with a different engine than the profile's",
        ),
        (
            "dns",
            "Check whether DNS lookups leak past the proxy",
            "Test with a different engine than the profile's",
        ),
        (
            "trust",
            "Score the profile the way a fingerprinter would",
            "Inspect with a different engine than the profile's",
        ),
        (
            "align",
            "Auto-adjust: match the profile to what its browser really shows",
            "Align against a different engine than the profile's",
        ),
    ] {
        commands.push(leaf(name, help).arg(profile()).arg(engine(engine_help)));
    }
    commands.push(super::profiles::bulk());
    commands.push(
        leaf("warmup", "Browse normally for a while to age the profile")
            .arg(profile())
            .arg(real("minutes", "minutes", "How long to browse (default: 5)").default_value("5"))
            .arg(repeated(
                "site",
                "site",
                "Site to visit (repeatable; replaces the preset/default list)",
            ))
            .arg(preset(
                "Per-vertical warm-up set to browse (default: general)",
            ))
            .arg(flag("headless", "headless", ""))
            .arg(engine("")),
    );
    commands.push(super::administration::schedule());
    commands.push(
        leaf(
            "attach",
            "Open the profile with DevTools for your own automation",
        )
        .arg(profile())
        .arg(whole("port", "port", "DevTools port (default: 9222)").default_value("9222"))
        .arg(engine(""))
        .arg(flag(
            "headless",
            "headless",
            "Attach without a visible window (for servers/CI)",
        )),
    );
    commands
}
