use super::{profile_errors, profile_routes::Profiles, request_body::ObjectBody};
use crate::{
    browser::{cookie_normalize, cookies},
    conversion::truth,
    storage::repo::RepoError,
};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::{Map, Value, json};
use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

pub(super) fn now() -> Result<f64, RepoError> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(std::io::Error::other)?
        .as_secs_f64())
}
pub(super) fn prepare(state: &Profiles, id: &str) -> Result<Option<Response>, RepoError> {
    if !state.dashboard.get(id)?.as_ref().is_some_and(truth) {
        return Ok(Some(profile_errors::detail(
            StatusCode::NOT_FOUND,
            "profile not found",
        )));
    }
    if state.sessions.running(id)? {
        return Ok(Some(profile_errors::detail(
            StatusCode::CONFLICT,
            "stop the profile first - its session is in use",
        )));
    }
    state.engine.materialize(id, now()?)?;
    Ok(None)
}
fn with_browser<T>(
    state: &Profiles,
    id: &str,
    operation: impl FnOnce(&mut crate::browser::adapter::Adapter) -> std::io::Result<T>,
) -> std::io::Result<T> {
    let runtime = state
        .browser
        .as_ref()
        .ok_or_else(|| std::io::Error::other("Native browser runtime is not configured"))?;
    let time = now().map_err(std::io::Error::other)?;
    state.sessions.with_finite_browser(
        id,
        id,
        |session, mut profile| {
            profile.startup_urls.clear();
            runtime.open(session, profile, true, time)
        },
        operation,
    )
}
fn helper_error(error: std::io::Error) -> Response {
    profile_errors::detail(StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
}
fn export_sync(state: &Profiles, id: &str) -> Result<Response, RepoError> {
    if let Some(refusal) = prepare(state, id)? {
        return Ok(refusal);
    }
    let result = with_browser(state, id, |adapter| {
        cookies::read(adapter).and_then(|rows| cookie_normalize::normalize_many(&rows))
    });
    Ok(match result {
        Ok(rows) => Json(json!({"cookies":rows})).into_response(),
        Err(error) => helper_error(error),
    })
}
fn import_sync(
    state: &Profiles,
    id: &str,
    body: Map<String, Value>,
) -> Result<Response, RepoError> {
    if let Some(refusal) = prepare(state, id)? {
        return Ok(refusal);
    }
    let rows = match body.get("text").filter(|value| !value.is_null()) {
        Some(Value::String(text)) => cookie_normalize::parse(text),
        Some(_) => {
            return Ok((StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response());
        }
        None => match body.get("cookies").filter(|value| truth(value)) {
            Some(Value::Array(rows)) => cookie_normalize::normalize_many(rows),
            Some(value) => cookie_normalize::parse(
                &serde_json::to_string(&json!({"cookies":value})).map_err(std::io::Error::other)?,
            ),
            None => Ok(Vec::new()),
        },
    };
    let result = rows.and_then(|rows| {
        if rows.is_empty() {
            let mut temporary = crate::storage::archive_paths::Staging::new(
                &std::env::temp_dir(),
                "numera-cookie-",
            )?;
            let file = temporary.path()?.join("cookies.json");
            let error =
                std::io::Error::other(format!("No usable cookies found in {}.", file.display()));
            temporary.cleanup()?;
            return Err(error);
        }
        cookies::preflight(&rows)?;
        with_browser(state, id, |adapter| {
            cookies::write_open(adapter, &rows, body.get("clear").is_some_and(truth))
        })
    });
    Ok(match result {
        Ok(count) => Json(json!({"ok":true,"imported":count})).into_response(),
        Err(error) => helper_error(error),
    })
}
pub(super) async fn export(State(state): State<Arc<Profiles>>, Path(id): Path<String>) -> Response {
    match super::admission::blocking(move || export_sync(&state, &id)).await {
        Ok(Ok(response)) => response,
        Ok(Err(error)) => profile_errors::storage(error),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response(),
    }
}
pub(super) async fn import(
    State(state): State<Arc<Profiles>>,
    Path(id): Path<String>,
    ObjectBody(body): ObjectBody,
) -> Response {
    match super::admission::blocking(move || import_sync(&state, &id, body)).await {
        Ok(Ok(response)) => response,
        Ok(Err(error)) => profile_errors::storage(error),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response(),
    }
}
