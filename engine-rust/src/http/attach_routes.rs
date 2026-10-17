use super::{profile_errors, profile_routes::Profiles, request_body::OptionalObjectBody};
use crate::{
    conversion::{truth, value_text},
    network::devtools,
    ownership::ready::StartupFailed,
    storage::{config::ConfigStore, repo::RepoError},
};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::{Map, Value, json};
use std::{net::TcpListener, sync::Arc, time::Duration};

fn attach(state: &Profiles, id: &str, body: Map<String, Value>) -> Result<Response, RepoError> {
    let Some(record) = state.dashboard.get(id)?.filter(truth) else {
        return Ok(profile_errors::detail(
            StatusCode::NOT_FOUND,
            "profile not found",
        ));
    };
    if state.sessions.running(id)? {
        return Ok(match state.sessions.attached(id)? {
            Some(details) => Json(details).into_response(),
            None => profile_errors::detail(
                StatusCode::CONFLICT,
                "stop the profile first - its session is in use",
            ),
        });
    }
    let config = ConfigStore::new(state.root.clone()).read();
    let engine = record
        .get("engine")
        .filter(|value| truth(value))
        .and_then(Value::as_str)
        .or_else(|| config.get("default_engine").and_then(Value::as_str))
        .unwrap_or("playwright");
    if !["playwright", "patchright", "cloak"].contains(&engine) {
        return Ok(profile_errors::detail(
            StatusCode::BAD_REQUEST,
            format!(
                "the '{engine}' engine has no DevTools to attach to — use one of: playwright, patchright, cloak"
            ),
        ));
    }
    state.engine.materialize(id, super::cookie_routes::now()?)?;
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    let headless = body.get("headless").is_some_and(truth);
    let runtime = state
        .browser
        .as_ref()
        .ok_or_else(|| RepoError::Message("Native browser runtime is not configured".into()))?;
    let result = state.sessions.open_attachment(
        id,
        &value_text(&record["name"]),
        |session, mut profile| {
            profile
                .launch_args
                .push(format!("--remote-debugging-port={port}"));
            runtime
                .open_lifetime(
                    session,
                    profile,
                    headless,
                    true,
                    super::cookie_routes::now().map_err(|error| StartupFailed {
                        kind: "failed".into(),
                        message: error.to_string(),
                    })?,
                )
        },
        || {
            let info = devtools::info(port, Duration::from_secs(20))?;
            let ws = info.get("webSocketDebuggerUrl").cloned().unwrap_or_else(|| "".into());
            let snippets = devtools::snippets(port, ws.as_str().unwrap_or(""));
            Ok(json!({"pid":id,"port":port,"wsUrl":ws,"headless":headless,
                "browser":info.get("Browser").cloned().unwrap_or_else(|| "".into()),"snippets":snippets}))
        },
    );
    match result {
        Ok(None) => Ok(profile_errors::detail(
            StatusCode::CONFLICT,
            "stop the profile first - its session is in use",
        )),
        Err(error) => {
            let status = match error.kind.as_str() {
                "busy" => StatusCode::CONFLICT,
                "locked" => StatusCode::LOCKED,
                "gone" => StatusCode::NOT_FOUND,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            Ok(profile_errors::detail(status, error.message))
        }
        Ok(Some(details)) => Ok(Json(details).into_response()),
    }
}
pub(super) async fn attach_profile(
    State(state): State<Arc<Profiles>>,
    Path(id): Path<String>,
    OptionalObjectBody(body): OptionalObjectBody,
) -> Response {
    match super::admission::blocking(move || attach(&state, &id, body)).await {
        Ok(Ok(response)) => response,
        Ok(Err(error)) => profile_errors::storage(error),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response(),
    }
}
