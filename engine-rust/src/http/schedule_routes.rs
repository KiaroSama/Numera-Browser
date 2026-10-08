use super::{
    cors, middleware::protect, request_auth::RequestAuth, request_body::ObjectBody,
    tokens::TokenStore,
};
use crate::storage::{
    dashboard::DashboardStore,
    repo::RepoError,
    schedules::{ScheduleStore, next_run_at, validate_schedule},
};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, put},
};
use serde_json::{Map, Value, json};
use std::{
    path::PathBuf,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

struct ScheduleState {
    schedules: ScheduleStore,
    dashboard: DashboardStore,
}
pub fn router(root: PathBuf, environment: &str) -> Router {
    let auth = Arc::new(RequestAuth::new(TokenStore::new(root.clone()), environment));
    protect(cors::apply(routes(root, None), ""), auth)
}
pub(crate) fn routes(root: PathBuf, password: Option<String>) -> Router {
    let state = Arc::new(ScheduleState {
        schedules: ScheduleStore::new(root.clone()),
        dashboard: DashboardStore::new(root, password),
    });
    Router::new()
        .route(
            "/api/schedules",
            get(list)
                .head(super::application::method_not_allowed)
                .post(add),
        )
        .route("/api/schedules/{id}", put(update).delete(delete))
        .with_state(state)
}
fn now() -> Result<f64, RepoError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs_f64())
        .map_err(|_| RepoError::InvalidRecord("System clock precedes the Unix epoch".into()))
}
fn decorate(state: &ScheduleState, mut row: Value) -> Result<Value, RepoError> {
    let id = row["profileId"]
        .as_str()
        .ok_or_else(|| RepoError::InvalidRecord("Schedule profile ID is not text".into()))?;
    let profile = state.dashboard.get(id)?;
    let name = profile
        .as_ref()
        .and_then(|profile| profile.get("name"))
        .cloned()
        .unwrap_or_else(|| Value::String(id.into()));
    let next = next_run_at(&row)?;
    row["profileName"] = name;
    row["nextRun"] = next.into();
    row["due"] = (next <= now()?).into();
    Ok(row)
}
async fn list(State(state): State<Arc<ScheduleState>>) -> Response {
    match tokio::task::spawn_blocking(move || {
        state
            .schedules
            .list()?
            .into_iter()
            .map(|row| decorate(&state, row))
            .collect::<Result<Vec<_>, RepoError>>()
    })
    .await
    {
        Ok(Ok(rows)) => Json(rows).into_response(),
        _ => failure(),
    }
}
async fn add(State(state): State<Arc<ScheduleState>>, ObjectBody(body): ObjectBody) -> Response {
    match tokio::task::spawn_blocking(move || {
        let Some(id) = body
            .get("profileId")
            .filter(|value| crate::conversion::truth(value))
            .and_then(Value::as_str)
        else {
            return Ok(error(StatusCode::NOT_FOUND, "profile not found"));
        };
        if state.dashboard.get(id)?.is_none() {
            return Ok(error(StatusCode::NOT_FOUND, "profile not found"));
        }
        let mut clean = match validate_schedule(&Value::Object(body.clone()), false) {
            Ok(clean) => clean,
            Err(RepoError::Message(message)) => {
                return Ok(error(StatusCode::BAD_REQUEST, &message));
            }
            Err(error) => return Err(error),
        };
        if clean
            .get("preset")
            .is_none_or(|value| !crate::conversion::truth(value))
        {
            clean.insert(
                "preset".into(),
                body.get("preset").cloned().unwrap_or("general".into()),
            );
        }
        clean.insert(
            "enabled".into(),
            body.get("enabled").cloned().unwrap_or(true.into()),
        );
        let row = state
            .schedules
            .add(id, Value::Object(clean), Some(now()?))?;
        Ok(Json(decorate(&state, row)?).into_response())
    })
    .await
    {
        Ok(Ok(response)) => response,
        _ => failure(),
    }
}
async fn update(
    State(state): State<Arc<ScheduleState>>,
    Path(id): Path<String>,
    ObjectBody(body): ObjectBody,
) -> Response {
    match tokio::task::spawn_blocking(move || update_row(&state, &id, body)).await {
        Ok(Ok(response)) => response,
        _ => failure(),
    }
}
fn update_row(
    state: &ScheduleState,
    id: &str,
    body: Map<String, Value>,
) -> Result<Response, RepoError> {
    let patch = match validate_schedule(&Value::Object(body), true) {
        Ok(patch) => patch,
        Err(RepoError::Message(message)) => return Ok(error(StatusCode::BAD_REQUEST, &message)),
        Err(error) => return Err(error),
    };
    if patch.is_empty() {
        return Ok(error(
            StatusCode::BAD_REQUEST,
            "nothing to change - send everyHours, minutes, preset or enabled",
        ));
    }
    match state.schedules.update(id, &Value::Object(patch))? {
        Some(row) => Ok(Json(decorate(state, row)?).into_response()),
        None => Ok(error(StatusCode::NOT_FOUND, "schedule not found")),
    }
}
async fn delete(State(state): State<Arc<ScheduleState>>, Path(id): Path<String>) -> Response {
    match tokio::task::spawn_blocking(move || state.schedules.delete(&id)).await {
        Ok(Ok(true)) => Json(json!({"ok":true})).into_response(),
        Ok(Ok(false)) => error(StatusCode::NOT_FOUND, "schedule not found"),
        _ => failure(),
    }
}
fn error(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({"detail":message}))).into_response()
}
fn failure() -> Response {
    (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response()
}
