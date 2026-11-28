use super::*;
use crate::release_controls::{self as release, PREFERENCES};

pub(super) async fn refresh_release_preferences() {
    let result = request("GET", "/api/updates/preferences", None).await;
    change(|a| match result {
        Ok(value) if release::preferences(&value) => {
            a.release_preferences = value;
            a.release_preferences_error = false;
        }
        _ => {
            a.release_preferences = Value::Null;
            a.release_preferences_error = true;
        }
    });
}

pub(super) async fn dispatch(act: &str, id: &str, a: &App) -> Result<bool, ApiError> {
    if !a.live() {
        return Ok(false);
    }
    match act {
        "profile-ip-check" => {
            if !a.profiles.iter().any(|p| s(p, "id") == id) {
                return Err(fail("Select a saved profile to check its IP."));
            }
            let path = release::profile_path(id, "/ip-check")
                .ok_or_else(|| fail("Invalid profile ID."))?;
            change(|a| {
                a.ip_checks.remove(id);
            });
            draw();
            let value = request("GET", &path, None).await?;
            if value["profile_id"] != id || !value["ready"].is_boolean() {
                return Err(fail("The engine returned an invalid IP readiness result."));
            }
            change(|a| {
                if a.profiles.iter().any(|p| s(p, "id") == id) {
                    a.ip_checks.insert(id.into(), value);
                }
            });
        }
        "export-cookies" => export_cookies(a, id).await?,
        "fleet-stop" => stop_windows(a).await?,
        "release-retry" => refresh_release_preferences().await,
        "release-preference" => {
            if !PREFERENCES.iter().any(|(key, _)| *key == id)
                || !release::preferences(&a.release_preferences)
            {
                return Err(fail("Load update preferences before changing them."));
            }
            let mut preferences = json!({});
            for (key, _) in PREFERENCES {
                preferences[key] = a.release_preferences[key].clone();
            }
            preferences[id] = json!(preferences[id] != true);
            let value = request("PUT", "/api/updates/preferences", Some(preferences)).await?;
            if !release::preferences(&value) {
                return Err(fail("The engine did not confirm the update preferences."));
            }
            change(|a| a.release_preferences = value);
        }
        "release-check" if matches!(id, "app" | "browsers" | "all") => {
            change(|a| a.release_status = Value::Null);
            draw();
            let status = request("GET", &format!("/api/updates/check?channel={id}"), None).await?;
            if status["channel"] != id
                || (id != "browsers" && !status["app"]["update_available"].is_boolean())
                || (id != "app" && !status["browsers"].is_array())
            {
                return Err(fail("The engine returned an invalid update report."));
            }
            change(|a| a.release_status = status);
        }
        "release-apply" if release::can_apply(&a.release_status, id) => {
            let (title, body) = match id {
                "all" => (
                    "Apply the App + Browsers update?",
                    "Save your work and close browser windows before updating. The app may restart.",
                ),
                "app" => (
                    "Apply the app update?",
                    "Save your work before updating. The app may restart.",
                ),
                _ => (
                    "Apply browser core updates?",
                    "Close browser windows and save your work before updating their cores.",
                ),
            };
            confirm(
                "release-apply",
                vec![id.into()],
                title,
                body,
                "Apply update",
            );
        }
        "mcp-copy" => {
            if let Some(command) = &a.mcp_bridge_command {
                copy(command).await?;
            }
        }
        "mcp-check" => {
            if a.mcp_bridge_command.is_none() {
                return Err(fail("The MCP helper requires a local engine address."));
            }
            if auth().is_empty() {
                return Err(fail("Create an API token before checking the MCP bridge."));
            }
            let deadline = Deadline::new(5000)?;
            let body = json!({"jsonrpc":"2.0","id":"dashboard-ping","method":"ping"});
            let response = send(
                "POST",
                &contracts::api_url("/api/mcp"),
                Some(&body),
                &deadline,
                &auth(),
                true,
            )
            .await?;
            let ok = response.ok()
                && parse(&response).await.is_ok_and(|body| {
                    body["jsonrpc"] == "2.0"
                        && body["id"] == "dashboard-ping"
                        && body["result"].is_object()
                });
            if !ok {
                return Err(fail(
                    "The MCP bridge did not respond. Check the engine and API token.",
                ));
            }
            change(|a| a.say("MCP bridge responded.", false));
        }
        _ => return Ok(false),
    }
    Ok(true)
}

pub(super) async fn apply_release(a: &App, channel: &str) -> Result<(), ApiError> {
    if a.update_bridge.is_none() || a.mcp_bridge_command.is_none() {
        return Err(fail("Open the desktop app to apply this update."));
    }
    if !a.live()
        || !release::can_apply(&a.release_status, channel)
        || !release::preferences(&a.release_preferences)
    {
        return Err(fail(
            "Check for updates and load preferences before applying an update.",
        ));
    }
    let body = json!({"channel":channel,"clean_old":a.release_preferences["clean_old_versions_on_update"],"offerId":a.release_status["offerId"]});
    change(|a| a.release_status = Value::Null);
    let result = request("POST", "/api/updates/apply", Some(body)).await?;
    if result["ok"] != true || result["data_preserved"] != true || result["channel"] != channel {
        return Err(fail("The engine did not confirm a successful update."));
    }
    let message = if s(&result, "message").is_empty() {
        "Update applied."
    } else {
        s(&result, "message")
    };
    change(|a| a.say(message, false));
    Ok(())
}

async fn export_cookies(a: &App, format: &str) -> Result<(), ApiError> {
    let mime = match format {
        "json" => "application/json",
        "netscape" => "text/plain",
        _ => return Err(fail("Select JSON or Netscape format.")),
    };
    let profile = a
        .editor
        .as_ref()
        .filter(|e| e.draft["_new"] != true)
        .ok_or_else(|| fail("Save this profile before exporting its cookies."))?;
    let id = s(&profile.draft, "id");
    let suffix = format!("/cookies/export?format={format}");
    let path = release::profile_path(id, &suffix).ok_or_else(|| fail("Invalid profile ID."))?;
    let value = request("GET", &path, None).await?;
    let (Some(content), Some(count)) = (value["content"].as_str(), value["count"].as_u64()) else {
        return Err(fail("The engine returned an invalid cookie export."));
    };
    if value["profile_id"] != id || value["format"] != format {
        return Err(fail(
            "The cookie export did not match the selected profile and format.",
        ));
    }
    if !alive() || !snapshot().live() {
        return Ok(());
    }
    let extension = if format == "json" { "json" } else { "txt" };
    let name = filename(s(&profile.draft, "name"));
    download(&format!("{name}-cookies.{extension}"), content, mime)?;
    change(|a| a.say(format!("Exported {count} cookies."), false));
    Ok(())
}

async fn stop_windows(a: &App) -> Result<(), ApiError> {
    let ids: Vec<_> = a
        .selected_profiles()
        .iter()
        .filter(|p| p["status"] == "running")
        .map(|p| s(p, "id").to_owned())
        .collect();
    if ids.is_empty() {
        return Err(fail("Select at least one running window."));
    }
    let body = json!({"profile_ids":ids,"action":"stop","url":null});
    let result = request("POST", "/api/fleet/sync", Some(body)).await?;
    let results = list(&result["results"]);
    let settled: Vec<_> = ids
        .into_iter()
        .map(|id| {
            let result = results.iter().find(|row| row["profile_id"] == id);
            let status = match result {
                Some(row) if row["ok"] == true => Ok(()),
                Some(row) if !s(row, "error").is_empty() => Err(s(row, "error").to_owned()),
                _ => Err("The engine did not confirm the window stopped".into()),
            };
            (id, status)
        })
        .collect();
    change(|a| a.bulk_result("Stopped", &settled, 0));
    refresh().await
}
