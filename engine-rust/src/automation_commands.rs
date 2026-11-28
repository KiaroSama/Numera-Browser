use crate::{
    browser::{
        automation::{Automation, PageAction, invalid},
        workflows::{self, LocalWorkflow, Workflows},
    },
    cli::{application::Application, profile_commands::Outcome, reflection},
    storage::repo::RepoError,
};
use serde_json::{Map, Value, json};
use std::io;

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
                                "update_available": Value::Null,
                            })
                        })
                        .collect();
                    json!({
                        "ok": false,
                        "state": "unavailable",
                        "checked": false,
                        "message": "Use Software updates in the portable desktop to check and review signed releases. Standalone CLI discovery is unavailable; no remote check was performed.",
                        "channel": channel,
                        "app": {
                            "current": env!("CARGO_PKG_VERSION"),
                            "latest": Value::Null,
                            "update_available": Value::Null,
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
                json!({"tools":reflection::tools(),"http_endpoint":"/api/mcp","http_tools":reflection::tools().into_iter().filter_map(|tool| tool["name"].as_str().filter(|name| crate::mcp_http::http_tool(name)).map(str::to_owned)).collect::<Vec<_>>() })
            }
            _ => return Err(invalid("Unknown QA command").into()),
        })
    })();
    Ok(match result {
        Ok(value) => Outcome {
            exit: i32::from(
                text("command") == Some("update")
                    && text("update_command") == Some("check")
                    && value["checked"] != true,
            ),
            payload: value,
        },
        Err(error) => Outcome {
            exit: 1,
            payload: json!({"ok":false,"error":error.to_string(),"hint":"Check the command help, profile ID, and local fixture. Use the running service for persistent windows."}),
        },
    })
}
