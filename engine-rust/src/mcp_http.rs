//! HTTP MCP transport policy and tool dispatch.
use crate::{
    cli::{profile_commands::Outcome, reflection},
    conversion::truth,
    mcp::{MAX_FRAME, error_result, handle_tools, tool_result},
};
use axum::{Router, extract::DefaultBodyLimit, routing::MethodRouter};
use serde_json::{Value, json};
use std::io;

pub(crate) fn mcp_routes<S>(handler: MethodRouter<S>, state: S) -> Router
where
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/api/mcp", handler.layer(DefaultBodyLimit::max(MAX_FRAME)))
        .with_state(state)
}

pub(crate) fn input_error(message: String) -> io::Result<Value> {
    tool_result(Outcome {
        exit: 1,
        payload: json!({"ok":false,"error":message,"hint":"Check this tool's inputSchema in tools/list"}),
    })
}

pub(crate) fn handle_http(
    message: &Value,
    execute: impl FnMut(Vec<String>) -> io::Result<Outcome>,
) -> io::Result<Option<Value>> {
    let mut execute = execute;
    let mut result = handle_tools(message, |name, arguments| {
        if !http_tool(name) {
            return error_result(format!("no such tool '{name}'"));
        }
        if name == "cookies_export"
            && ["file", "out"]
                .iter()
                .any(|key| arguments.get(*key).is_some_and(|value| !value.is_null()))
        {
            return error_result(
                "HTTP cookie export returns content and cannot write a server file".into(),
            );
        }
        if arguments.get("detach").is_some_and(truth) {
            return input_error("HTTP MCP runs tools inline".into());
        }
        let argv = match reflection::argv(name, arguments) {
            Ok(argv) => argv,
            Err(error) => return input_error(error.to_string()),
        };
        match execute(argv) {
            Ok(outcome) if outcome.exit != 0 && outcome.payload.get("hint").is_none() => {
                let mut outcome = outcome;
                if let Some(payload) = outcome.payload.as_object_mut() {
                    payload.insert(
                        "hint".into(),
                        "Check this tool's parameters and profile state before retrying".into(),
                    );
                }
                tool_result(outcome)
            }
            Ok(outcome) => tool_result(outcome),
            Err(error) => input_error(error.to_string()),
        }
    })?;
    if message.get("method").and_then(Value::as_str) == Some("tools/list")
        && let Some(tools) = result
            .as_mut()
            .and_then(|result| result["result"]["tools"].as_array_mut())
    {
        tools.retain(|tool| tool["name"].as_str().is_some_and(http_tool));
        for tool in tools {
            let cookies = tool["name"] == "cookies_export";
            if let Some(properties) = tool["inputSchema"]["properties"].as_object_mut() {
                properties.remove("detach");
                if cookies {
                    properties.remove("file");
                    properties.remove("out");
                }
            }
            if cookies {
                tool["description"] =
                    "Return JSON or Netscape cookie content for the selected local profile".into();
            }
        }
    }
    Ok(result)
}

pub(crate) fn http_tool(tool: &str) -> bool {
    // HTTP must not grant a caller access to arbitrary files on the service host.
    matches!(
        tool,
        "list"
            | "check"
            | "health"
            | "engines"
            | "proxy_test"
            | "profile_ip_check"
            | "cookies_export"
            | "capabilities"
            | "page_navigate"
            | "page_click"
            | "page_fill"
            | "page_snapshot"
            | "page_console"
            | "page_network"
            | "page_eval"
            | "page_screenshot"
            | "page_wait"
            | "workflow_list"
            | "workflow_save"
            | "workflow_run"
            | "workflow_delete"
            | "fleet_sync"
    )
}
