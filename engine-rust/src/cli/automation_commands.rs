use super::{application::Application, definitions::*, profile_commands::Outcome, reflection};
use crate::{
    browser::{
        automation::{ACTIONS, Automation, PageAction, invalid},
        workflows::{self, LocalWorkflow, Workflows},
    },
    storage::repo::RepoError,
};
use clap::Command;
use serde_json::{Map, Value, json};
use std::io;

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

pub(super) fn run(
    service: &Application<'_>,
    args: &Map<String, Value>,
    now: f64,
) -> Result<Outcome, RepoError> {
    let text = |key| args.get(key).and_then(Value::as_str);
    let need = |key| {
        text(key)
            .filter(|text| !text.is_empty())
            .ok_or_else(|| invalid(&format!("Provide {key}")))
    };
    let ids = || {
        args.get("profile_ids")
            .and_then(Value::as_array)
            .ok_or_else(|| invalid("Provide profile IDs"))?
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| invalid("Profile IDs must be text"))
            })
            .collect::<io::Result<Vec<_>>>()
    };
    let store = Workflows(service.root);
    let automation = Automation {
        repo: service.repo,
        sessions: service.sessions,
        runtime: service.browser,
    };
    let result = (|| -> Result<Value, RepoError> {
        Ok(match need("command")? {
            "page" => automation.run(
                need("profile")?,
                &PageAction::parse(need("page_command")?, args)?,
                now,
            )?,
            "workflow" => match need("workflow_command")? {
                "list" => json!({"workflows":store.list()?}),
                "save" => {
                    let input = need("workflow")?;
                    if input.len() > 128 * 1024 {
                        return Err(invalid("Workflow document exceeds 128 KiB").into());
                    }
                    let workflow: LocalWorkflow = serde_json::from_str(input)
                        .map_err(|_| invalid("Provide a valid workflow JSON object"))?;
                    store.save(&workflow)?;
                    json!({"workflow":workflow})
                }
                "delete" => {
                    let id = need("id")?;
                    store.delete(id)?;
                    json!({"deleted":true,"id":id})
                }
                "run" => serde_json::to_value(workflows::run(
                    &automation,
                    &store.load(need("id")?)?,
                    &ids()?,
                    now,
                )?)
                .map_err(io::Error::other)?,
                _ => return Err(invalid("Unknown workflow command").into()),
            },
            "fleet" => workflows::fleet(
                service.repo,
                service.sessions,
                service.browser,
                &ids()?,
                need("action")?,
                text("url"),
                now,
            )?,
            "profile" => workflows::profile_ip_check(service.repo, need("id")?, now)?,
            "update" => match text("update_command").unwrap_or("") {
                "check" => {
                    let channel = text("channel").unwrap_or("all");
                    let registry = crate::browser::provider::ProviderRegistry::default();
                    let browsers: Vec<Value> = registry
                        .list()
                        .into_iter()
                        .map(|provider| {
                            json!({
                                "id": provider.id(),
                                "display_name": provider.display_name(),
                                "capabilities": provider.capabilities(),
                                "update_available": false,
                            })
                        })
                        .collect();
                    json!({
                        "channel": channel,
                        "app": {
                            "current": env!("CARGO_PKG_VERSION"),
                            "latest": env!("CARGO_PKG_VERSION"),
                            "update_available": false,
                            "verified": false,
                        },
                        "browsers": browsers,
                    })
                }
                "config" => {
                    let current = crate::http::update_routes::preferences(service.root, None)
                        .map_err(|error| RepoError::Message(error.to_string()))?;
                    let parse_bool =
                        |key: &str, default: bool| match args.get(key).and_then(Value::as_str) {
                            Some("true") => true,
                            Some("false") => false,
                            _ => default,
                        };
                    let next = crate::http::update_routes::UpdatePreferences {
                        app_auto_update: parse_bool("app_auto_update", current.app_auto_update),
                        browser_core_auto_update: parse_bool(
                            "browser_core_auto_update",
                            current.browser_core_auto_update,
                        ),
                        clean_old_versions_on_update: parse_bool(
                            "clean_old_versions_on_update",
                            current.clean_old_versions_on_update,
                        ),
                    };
                    let changed = next != current;
                    let saved = if changed {
                        crate::http::update_routes::preferences(service.root, Some(next))
                            .map_err(|error| RepoError::Message(error.to_string()))?
                    } else {
                        current
                    };
                    json!({
                        "ok": true,
                        "app_auto_update": saved.app_auto_update,
                        "browser_core_auto_update": saved.browser_core_auto_update,
                        "clean_old_versions_on_update": saved.clean_old_versions_on_update,
                    })
                }
                "apply" => {
                    return Err(invalid(
                        "Signed update activation requires the desktop updater bridge or verified local release installer",
                    )
                    .into());
                }
                _ => return Err(invalid("Unknown update subcommand").into()),
            },
            "capabilities" => {
                json!({"tools":reflection::tools(),"http_endpoint":"/api/mcp","http_tools":reflection::tools().into_iter().filter_map(|tool| tool["name"].as_str().filter(|name| crate::mcp::http_tool(name)).map(str::to_owned)).collect::<Vec<_>>() })
            }
            _ => return Err(invalid("Unknown QA command").into()),
        })
    })();
    Ok(match result {
        Ok(value) => Outcome::success(value),
        Err(error) => Outcome {
            exit: 1,
            payload: json!({"ok":false,"error":error.to_string(),"hint":"Check the command help, profile ID, and local fixture. Use the running service for persistent windows."}),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
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
