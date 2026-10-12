use super::{
    profile_errors,
    profile_routes::{Profiles, decorate},
    request_body::OptionalObjectBody,
};
use crate::{
    conversion::{truth, value_text},
    storage::{config::ConfigStore, repo::RepoError},
};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::{Map, Value};
use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

fn now() -> Result<f64, RepoError> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(std::io::Error::other)?
        .as_secs_f64())
}
fn launch(state: &Profiles, id: &str, body: Map<String, Value>) -> Result<Response, RepoError> {
    let Some(record) = state.dashboard.get(id)?.filter(truth) else {
        return Ok(profile_errors::detail(
            StatusCode::NOT_FOUND,
            "profile not found",
        ));
    };
    if state.sessions.owns(id)? && state.sessions.running(id)? {
        return Ok(Json(decorate(state, record, now()?)?).into_response());
    }
    state.engine.materialize(id, now()?)?;
    let configured = ConfigStore::new(state.root.clone()).read();
    let headless = body
        .get("headless")
        .filter(|value| !value.is_null())
        .unwrap_or_else(|| configured.get("headless_launch").unwrap_or(&Value::Null));
    let runtime = match &state.browser {
        Some(runtime) => runtime,
        None => {
            return Ok(profile_errors::detail(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Native browser runtime is not configured",
            ));
        }
    };
    let result =
        state
            .sessions
            .open_browser(id, &value_text(&record["name"]), |session, profile| {
                runtime.open(
                    session,
                    profile,
                    truth(headless),
                    now().map_err(|error| crate::ownership::ready::StartupFailed {
                        kind: "failed".into(),
                        message: error.to_string(),
                    })?,
                )
            });
    if let Err(error) = result {
        let status = match error.kind.as_str() {
            "busy" => StatusCode::CONFLICT,
            "locked" => StatusCode::LOCKED,
            "gone" => StatusCode::NOT_FOUND,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        return Ok(profile_errors::detail(status, error.message));
    }
    let current = state.dashboard.get(id)?.filter(truth).unwrap_or(record);
    Ok(Json(decorate(state, current, now()?)?).into_response())
}
pub(super) async fn launch_profile(
    State(state): State<Arc<Profiles>>,
    Path(id): Path<String>,
    OptionalObjectBody(body): OptionalObjectBody,
) -> Response {
    match tokio::task::spawn_blocking(move || launch(&state, &id, body)).await {
        Ok(Ok(response)) => response,
        Ok(Err(error)) => profile_errors::storage(error),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response(),
    }
}
