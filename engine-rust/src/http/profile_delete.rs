use super::{profile_errors, profile_routes::Profiles, request_body::ObjectBody};
use crate::{conversion::truth, ownership::claims, storage::repo::RepoError};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::{Map, Value, json};
use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

fn exists(state: &Profiles, id: &str) -> Result<bool, RepoError> {
    Ok(state
        .dashboard
        .get(id)?
        .is_some_and(|record| truth(&record))
        || state.engine.get(id)?.is_some())
}
fn remove(state: &Profiles, id: &str) -> Result<bool, RepoError> {
    if !exists(state, id)? {
        return Ok(false);
    }
    if state.sessions.owns(id)? {
        state.sessions.stop(id, Duration::from_secs(10))?;
    }
    // Preserve the original busy refusal vocabulary; only actual owned trees
    // above may be stopped, and the storage lease remains the final race gate.
    if state.sessions.running(id)? {
        claims::refuse_if_busy(&state.root, id, &state.host)
            .map_err(|error| RepoError::SessionBusy(error.to_string()))?;
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(std::io::Error::other)?
        .as_secs_f64();
    state.dashboard.delete(id, &state.host, now)?;
    Ok(true)
}
pub(super) async fn single(State(state): State<Arc<Profiles>>, Path(id): Path<String>) -> Response {
    match tokio::task::spawn_blocking(move || remove(&state, &id)).await {
        Ok(Ok(true)) => Json(json!({"ok":true})).into_response(),
        Ok(Ok(false)) => profile_errors::detail(StatusCode::NOT_FOUND, "profile not found"),
        Ok(Err(error)) => profile_errors::storage(error),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response(),
    }
}
fn remove_many(state: &Profiles, body: Map<String, Value>) -> Response {
    let Some(ids) = body
        .get("ids")
        .and_then(Value::as_array)
        .filter(|rows| rows.iter().all(Value::is_string))
    else {
        return profile_errors::detail(
            StatusCode::BAD_REQUEST,
            "'ids' must be a list of profile ids",
        );
    };
    let mut removed = Vec::new();
    let mut missing = Vec::new();
    let mut failed = Vec::new();
    for id in ids.iter().filter_map(Value::as_str) {
        match remove(state,id) {
            Ok(true)=>removed.push(id),Ok(false)=>missing.push(id),
            Err(error)=>failed.push(json!({"id":id,"error":error.to_string().lines().next().unwrap_or("").chars().take(200).collect::<String>()})),
        }
    }
    Json(json!({"removed":removed,"missing":missing,"failed":failed})).into_response()
}
pub(super) async fn bulk(
    State(state): State<Arc<Profiles>>,
    ObjectBody(body): ObjectBody,
) -> Response {
    match tokio::task::spawn_blocking(move || remove_many(&state, body)).await {
        Ok(response) => response,
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response(),
    }
}
