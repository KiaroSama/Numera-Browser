use super::definitions::*;
use crate::browser::automation::ACTIONS;
use clap::Command;

pub(super) fn commands() -> Vec<Command> {
    let descriptions = [
        "Open a loopback fixture in the profile's QA tab",
        "Click a selector in the QA tab",
        "Fill an input in the QA tab",
        "Return an accessibility snapshot",
        "Return recent console messages",
        "Return recent local request URLs and status codes",
        "Evaluate JavaScript in the loopback QA tab",
        "Return a PNG screenshot as base64",
        "Wait for a visible element or a bounded duration",
    ];
    let pages = ACTIONS
        .into_iter()
        .zip(descriptions)
        .map(|(name, description)| {
            let mut command = leaf(name, description)
                .arg(option("profile", "profile", "ID of the owned browser profile").required(true))
                .arg(
                    option(
                        "url",
                        "url",
                        "Loopback fixture URL with an explicit port; reuse the QA tab if omitted",
                    )
                    .required(name == "navigate"),
                )
                .arg(whole(
                    "timeout_ms",
                    "timeout-ms",
                    "Action deadline in milliseconds (1-10000, default 3000)",
                ));
            if matches!(name, "click" | "fill" | "wait") {
                command = command.arg(
                    option(
                        "selector",
                        "selector",
                        "Playwright selector for the target element",
                    )
                    .required(name != "wait"),
                );
            }
            match name {
                "fill" => command.arg(
                    option(
                        "value",
                        "value",
                        "Text to enter; an empty string clears the field",
                    )
                    .required(true),
                ),
                "eval" => command.arg(
                    option(
                        "expression",
                        "expression",
                        "JavaScript to evaluate in the loopback QA tab",
                    )
                    .required(true),
                ),
                "wait" => command.arg(
                    whole(
                        "ms",
                        "ms",
                        "Duration in milliseconds; omit when waiting for a selector",
                    )
                    .conflicts_with("selector")
                    .required_unless_present("selector"),
                ),
                _ => command,
            }
        })
        .collect();
    vec![
        group("page", "Inspect or interact with a local QA fixture", pages)
            .after_help("Use only a trusted local test fixture and a dedicated QA profile. A standalone action needs a URL; service sessions can reuse the QA tab.\nExample: numera-engine page snapshot --profile qa --url http://127.0.0.1:8080/form --json"),
        group("workflow", "Save and run ordered local QA steps", vec![
            leaf("list", "List saved workflows"),
            leaf("save", "Save a workflow from an inline JSON document").arg(option("workflow", "workflow", "JSON object with id, name and steps; start with a loopback navigation").required(true)),
            leaf("delete", "Delete a saved workflow").arg(positional("id", "Workflow slug")),
            leaf("run", "Run a saved workflow and return a report for each profile")
                .arg(positional("id", "Workflow slug")).arg(repeated("profile_ids", "profile", "Profile ID; repeat up to four times").required(true)),
        ]).after_help("Example: numera-engine workflow run smoke --profile qa --json"),
        group("fleet", "Control owned QA windows through the running service", vec![
            leaf("sync", "Launch, stop, or navigate up to four profile windows")
                .arg(option("action", "action", "Action to apply to each profile").value_parser(["launch", "stop", "navigate"]).required(true))
                .arg(repeated("profile_ids", "profile", "Profile ID; repeat up to four times").required(true))
                .arg(option("url", "url", "Loopback fixture URL for navigation")),
        ]).after_help("Set NUMERA_API_URL and NUMERA_API_TOKEN for the running server.\nExample: numera-engine fleet sync --action launch --profile qa --json"),
        group("profile", "Inspect a single browser profile", vec![leaf("ip-check", "Check a profile's configured proxy exit and report local settings").arg(positional("id", "Profile ID"))]),
        group("update", "Check, configure, or apply signed app and browser core updates", vec![
            leaf("check", "Check installed app and browser core versions and signed update availability")
                .arg(option("channel", "channel", "Update channel: app, browsers, or all").value_parser(["app", "browsers", "all"])),
            leaf("apply", "Request applying a verified signed update (requires desktop approval bridge)")
                .arg(option("channel", "channel", "Update channel: app, browsers, or all").value_parser(["app", "browsers", "all"])),
            leaf("config", "Read or update automatic update and old-version cleanup preferences")
                .arg(option("app_auto_update", "app-auto-update", "Enable automatic app updates (true or false)").value_parser(["true", "false"]))
                .arg(option("browser_core_auto_update", "browser-core-auto-update", "Enable automatic browser core updates (true or false)").value_parser(["true", "false"]))
                .arg(option("clean_old_versions_on_update", "clean-old-versions", "Remove superseded runtime files after update (true or false)").value_parser(["true", "false"])),
        ]).after_help("Example: numera-engine update check --channel all --json"),
        leaf("capabilities", "List supported commands and their MCP input schemas"),
    ]
}

#[cfg(test)]
mod tests {
    use crate::cli::reflection;
    use serde_json::{Map, json};
    use std::io;
    #[test]
    fn page_and_workflow_are_reflected_with_required_parameters() -> io::Result<()> {
        let tools = reflection::tools();
        let find = |name| {
            tools
                .iter()
                .find(|tool| tool["name"].as_str() == Some(name))
        };
        let fill = find("page_fill").ok_or_else(|| io::Error::other("Missing page_fill"))?;
        assert_eq!(fill["inputSchema"]["additionalProperties"], false);
        assert!(
            fill["inputSchema"]["required"]
                .as_array()
                .is_some_and(|keys| ["profile", "selector", "value"]
                    .iter()
                    .all(|key| keys.contains(&json!(key))))
        );
        assert!(find("workflow_save").is_some());
        assert!(find("profile_ip_check").is_some());
        let argv = reflection::argv(
            "page_fill",
            &Map::from_iter([
                ("profile".into(), "qa".into()),
                ("selector".into(), "#input".into()),
                ("value".into(), "--json".into()),
            ]),
        )?;
        let matches = crate::cli::parse(std::iter::once("numera".to_owned()).chain(argv))
            .map_err(io::Error::other)?;
        let mut registry = crate::cli::registry();
        registry.build();
        assert_eq!(reflection::values(&registry, &matches)["value"], "--json");
        Ok(())
    }
}
