use axum::{
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode},
};
use numera_core::{cli, http::application::router, mcp};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};
use tower::ServiceExt;
mod common;

struct Surface {
    cli: &'static str,
    method: &'static str,
    endpoint: &'static str,
}

// This is the release contract, not a snapshot of what currently exists.
const SURFACES: &[Surface] = &[
    Surface {
        cli: "create",
        method: "POST",
        endpoint: "/api/profiles",
    },
    Surface {
        cli: "list",
        method: "GET",
        endpoint: "/api/profiles",
    },
    Surface {
        cli: "show",
        method: "GET",
        endpoint: "/api/profiles/parity-probe",
    },
    Surface {
        cli: "edit",
        method: "PUT",
        endpoint: "/api/profiles/parity-probe",
    },
    Surface {
        cli: "delete",
        method: "DELETE",
        endpoint: "/api/profiles/parity-probe",
    },
    Surface {
        cli: "pool test",
        method: "POST",
        endpoint: "/api/proxies/test",
    },
    Surface {
        cli: "cookies import",
        method: "POST",
        endpoint: "/api/profiles/parity-probe/cookies",
    },
    Surface {
        cli: "cookies export",
        method: "GET",
        endpoint: "/api/profiles/parity-probe/cookies/export?format=json",
    },
    Surface {
        cli: "cookies export",
        method: "GET",
        endpoint: "/api/profiles/parity-probe/cookies/export?format=netscape",
    },
    Surface {
        cli: "ext install",
        method: "POST",
        endpoint: "/api/extensions/install",
    },
    Surface {
        cli: "folder create",
        method: "POST",
        endpoint: "/api/folders",
    },
    Surface {
        cli: "schedule list",
        method: "GET",
        endpoint: "/api/schedules",
    },
    Surface {
        cli: "warmup",
        method: "POST",
        endpoint: "/api/profiles/parity-probe/warmup",
    },
    Surface {
        cli: "launch",
        method: "POST",
        endpoint: "/api/profiles/parity-probe/launch",
    },
    Surface {
        cli: "stop",
        method: "POST",
        endpoint: "/api/profiles/parity-probe/stop",
    },
    Surface {
        cli: "vault status",
        method: "GET",
        endpoint: "/api/vault/status",
    },
    Surface {
        cli: "config",
        method: "GET",
        endpoint: "/api/config",
    },
    Surface {
        cli: "update check",
        method: "GET",
        endpoint: "/api/updates/check",
    },
    Surface {
        cli: "update apply",
        method: "POST",
        endpoint: "/api/updates/apply",
    },
    Surface {
        cli: "update config",
        method: "PUT",
        endpoint: "/api/updates/preferences",
    },
    Surface {
        cli: "page navigate",
        method: "POST",
        endpoint: "/api/profiles/parity-probe/page/navigate",
    },
    Surface {
        cli: "page console",
        method: "POST",
        endpoint: "/api/profiles/parity-probe/page/console",
    },
    Surface {
        cli: "workflow list",
        method: "GET",
        endpoint: "/api/workflows",
    },
    Surface {
        cli: "fleet sync",
        method: "POST",
        endpoint: "/api/fleet/sync",
    },
    Surface {
        cli: "profile ip-check",
        method: "GET",
        endpoint: "/api/profiles/parity-probe/ip-check",
    },
    Surface {
        cli: "mcp",
        method: "POST",
        endpoint: "/api/mcp",
    },
];

fn tool_name(path: &str) -> String {
    path.replace([' ', '-'], "_")
}

fn catalog() -> Result<Vec<Value>, Box<dyn std::error::Error>> {
    let request = json!({"jsonrpc":"2.0","id":1,"method":"tools/list"});
    let response = mcp::handle(&request, |_| -> std::io::Result<_> {
        Err(std::io::Error::other(
            "tools/list must not execute a command",
        ))
    })?
    .ok_or("tools/list returned no response")?;
    assert_eq!(response["id"], 1);
    Ok(response["result"]["tools"]
        .as_array()
        .ok_or("tools/list returned no tools")?
        .clone())
}

fn usable_schema(schema: &Value) -> bool {
    let Some(properties) = schema["properties"].as_object() else {
        return false;
    };
    let required = schema.get("required");
    schema["type"] == "object"
        && required.is_none_or(|value| {
            value.as_array().is_some_and(|fields| {
                let mut seen = BTreeSet::new();
                fields.iter().all(|field| {
                    field
                        .as_str()
                        .is_some_and(|field| properties.contains_key(field) && seen.insert(field))
                })
            })
        })
        && properties
            .values()
            .all(|property| match property["type"].as_str() {
                Some("array") => property["items"].is_object(),
                Some("integer" | "number" | "string" | "boolean" | "object") => true,
                _ => false,
            })
}

#[test]
fn cli_and_mcp_expose_every_release_operation() -> Result<(), Box<dyn std::error::Error>> {
    let cli = cli::registry();
    let catalog = catalog()?;
    let tools: BTreeMap<_, _> = catalog
        .iter()
        .filter_map(|tool| Some((tool["name"].as_str()?, tool)))
        .collect();
    assert_eq!(
        tools.len(),
        catalog.len(),
        "tool names must be present and unique"
    );
    let mut gaps = Vec::new();
    for path in SURFACES
        .iter()
        .map(|surface| surface.cli)
        .chain(["capabilities"])
        .collect::<BTreeSet<_>>()
    {
        let command = path
            .split_ascii_whitespace()
            .try_fold(&cli, |command, part| command.find_subcommand(part));
        match command {
            Some(command)
                if command
                    .get_about()
                    .is_some_and(|text| !text.to_string().trim().is_empty()) =>
            {
                let mut command = command.clone();
                let help = command.render_long_help().to_string();
                if !help.to_ascii_lowercase().contains("example") {
                    gaps.push(format!("CLI help has no example: {path}"));
                }
            }
            _ => gaps.push(format!("CLI command or description missing: {path}")),
        }
        if path == "mcp" {
            // The bridge is transport, not a recursively callable tool.
            assert!(!tools.contains_key("mcp"));
            assert!(cli.find_subcommand("mcp").is_some_and(|command| {
                command
                    .get_arguments()
                    .any(|arg| arg.get_long() == Some("bridge"))
            }));
            continue;
        }
        match tools.get(tool_name(path).as_str()) {
            Some(tool)
                if tool["description"]
                    .as_str()
                    .is_some_and(|text| !text.trim().is_empty())
                    && usable_schema(&tool["inputSchema"]) => {}
            _ => gaps.push(format!("MCP tool, description or schema missing: {path}")),
        }
    }
    assert!(
        gaps.is_empty(),
        "Surface contract gaps:\n{}",
        gaps.join("\n")
    );
    Ok(())
}

#[test]
fn gui_http_registers_every_release_operation() -> Result<(), Box<dyn std::error::Error>> {
    let root = common::TempDirectory::new("surface parity")?;
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?
        .block_on(async {
            let budget = Duration::from_secs(5);
            let app = tokio::time::timeout(
                budget,
                router(
                    root.path().to_owned(),
                    "test",
                    "",
                    None,
                    "127.0.0.1",
                    BTreeMap::new(),
                ),
            )
            .await??;
            let mut gaps = Vec::new();
            for surface in SURFACES {
                let method: Method = surface.method.parse()?;
                let request = Request::builder()
                    .method(method)
                    .uri(surface.endpoint)
                    .header("content-type", "application/json")
                    .body(Body::from("{"))?;
                let response = tokio::time::timeout(budget, app.clone().oneshot(request)).await??;
                let status = response.status();
                if status == StatusCode::METHOD_NOT_ALLOWED
                    || (status == StatusCode::NOT_FOUND
                        && to_bytes(response.into_body(), 64 * 1024).await?.is_empty())
                {
                    gaps.push(format!(
                        "{} {} ({status})",
                        surface.method, surface.endpoint
                    ));
                }
            }
            let unauth = Request::post("/api/mcp")
                .header("content-type", "application/json")
                .body(Body::from("[]"))?;
            let unauth_resp = tokio::time::timeout(budget, app.clone().oneshot(unauth)).await??;
            assert_eq!(unauth_resp.status(), StatusCode::UNAUTHORIZED);
            let auth_app = tokio::time::timeout(
                budget,
                router(
                    root.path().to_owned(),
                    "parity-secret-token",
                    "",
                    None,
                    "127.0.0.1",
                    BTreeMap::new(),
                ),
            )
            .await??;
            let request = Request::post("/api/mcp")
                .header("content-type", "application/json")
                .header("authorization", "Bearer parity-secret-token")
                .body(Body::from("[]"))?;
            let response = tokio::time::timeout(budget, auth_app.oneshot(request)).await??;
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            let body: Value = serde_json::from_slice(
                &tokio::time::timeout(budget, to_bytes(response.into_body(), 64 * 1024)).await??,
            )?;
            assert_eq!(body["jsonrpc"], "2.0");
            assert_eq!(body["error"]["code"], -32600);
            assert!(gaps.is_empty(), "HTTP contract gaps:\n{}", gaps.join("\n"));
            Ok(())
        })
}

#[test]
fn mcp_results_keep_machine_readable_success_and_error_payloads()
-> Result<(), Box<dyn std::error::Error>> {
    let request = |tool| json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":tool,"arguments":{}}});
    let success = mcp::handle(&request("list"), |argv| {
        assert!(argv.iter().any(|argument| argument == "list"));
        Ok(cli::profile_commands::Outcome::success(
            json!({"profiles":[]}),
        ))
    })?
    .ok_or("missing MCP success response")?;
    assert_eq!(success["result"]["isError"], false);
    assert_eq!(
        success["result"]["structuredContent"],
        json!({"profiles":[]})
    );
    let failure = mcp::handle(&request("unregistered_parity_probe"), |_| {
        Err(std::io::Error::other("unknown tools must not execute"))
    })?
    .ok_or("missing MCP error response")?;
    assert_eq!(failure["result"]["isError"], true);
    let payload: Value = serde_json::from_str(
        failure["result"]["content"][0]["text"]
            .as_str()
            .ok_or("missing MCP error text")?,
    )?;
    assert!(
        payload["error"]
            .as_str()
            .is_some_and(|message| message.contains("unregistered_parity_probe"))
    );
    Ok(())
}
