use super::{
    profile_errors, profile_fields,
    profile_routes::{Profiles, decorate},
    request_body::ObjectBody,
};
use crate::{
    conversion::{secret_intent, truth},
    storage::{archive_paths::random_hex, config::ConfigStore, paths::safe_id, repo::RepoError},
};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::{Map, Value};
use std::{
    io,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

fn now() -> Result<f64, RepoError> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_secs_f64())
}
pub(super) fn check(record: &Value, merged: bool) -> Result<(), RepoError> {
    let mut client = record.clone();
    if merged {
        profile_fields::strip_server_owned(
            client.as_object_mut().ok_or_else(|| {
                RepoError::UnconvertibleProfile("profile must be an object".into())
            })?,
        )
        .map_err(|error| RepoError::Io(io::Error::other(error)))?;
    }
    let problems = profile_fields::check(&client, "profile", &[])
        .map_err(|error| RepoError::Io(io::Error::other(error)))?;
    if problems.is_empty() {
        Ok(())
    } else {
        Err(RepoError::UnconvertibleProfile(
            problems.into_iter().take(8).collect::<Vec<_>>().join("; "),
        ))
    }
}
fn strip(record: &mut Map<String, Value>) -> Result<(), RepoError> {
    profile_fields::strip_server_owned(record)
        .map_err(|error| RepoError::Io(io::Error::other(error)))?;
    Ok(())
}
pub(super) fn extensions(record: &mut Value, config: &ConfigStore) -> Result<(), RepoError> {
    if record["extensions"]
        .as_str()
        .is_none_or(|text| text.trim().is_empty())
    {
        let defaults = config
            .default_extensions()
            .map_err(|error| RepoError::Io(io::Error::other(error)))?;
        if !defaults.is_empty() {
            let text = defaults
                .iter()
                .map(|value| {
                    value.as_str().ok_or_else(|| {
                        RepoError::Io(io::Error::other("Stored default extension is not text"))
                    })
                })
                .collect::<Result<Vec<_>, _>>()?
                .join("\n");
            record["extensions"] = text.into();
        }
    }
    Ok(())
}
fn create_record(state: &Profiles, mut body: Map<String, Value>) -> Result<Value, RepoError> {
    let chose = body.get("engine").is_some_and(truth);
    strip(&mut body)?;
    let body = Value::Object(body);
    check(&body, false)?;
    let time = now()?;
    let config = ConfigStore::new(state.root.clone());
    let default = config.default_engine(&state.installed);
    let mut profile = profile_fields::complete(body, &random_hex(6)?, Some(&default), time)
        .map_err(RepoError::UnconvertibleProfile)?;
    extensions(&mut profile, &config)?;
    let changes = if chose {
        Map::from_iter([("default_engine".into(), profile["engine"].clone())])
    } else {
        Map::new()
    };
    let intent = secret_intent(&profile);
    let saved = state
        .dashboard
        .save(profile, changes, None, Some(intent), time)?;
    decorate(state, saved, time)
}
fn update_record(
    state: &Profiles,
    id: &str,
    mut body: Map<String, Value>,
) -> Result<Option<Value>, RepoError> {
    // Lookup uses raw state: the decrypted read view withholds locked secrets.
    if state.dashboard.raw(id)?.is_none_or(|value| !truth(&value)) {
        return Ok(None);
    }
    let asked = body.get("engine").cloned().unwrap_or(Value::Null);
    let revision = body.get("_rev").filter(|value| !value.is_null()).cloned();
    strip(&mut body)?;
    let patch = Value::Object(body.clone());
    check(&patch, false)?;
    let intent = secret_intent(&patch);
    let time = now()?;
    safe_id(id).map_err(|error| RepoError::UnconvertibleProfile(error.to_string()))?;
    let saved = state.dashboard.patch(
        id,
        |current| {
            let previous_engine = current["engine"].clone();
            let mut merged = current;
            merged
                .as_object_mut()
                .ok_or_else(|| RepoError::Io(io::Error::other("Stored profile is not an object")))?
                .extend(body);
            check(&merged, true)?;
            let merged = profile_fields::complete(merged, id, None, time)
                .map_err(RepoError::UnconvertibleProfile)?;
            let changes = if truth(&asked) && asked != previous_engine {
                Map::from_iter([("default_engine".into(), merged["engine"].clone())])
            } else {
                Map::new()
            };
            Ok((merged, changes))
        },
        revision.as_ref(),
        Some(intent),
        time,
    )?;
    saved
        .map(|record| decorate(state, record, time))
        .transpose()
}
pub(super) async fn create(
    State(state): State<Arc<Profiles>>,
    ObjectBody(body): ObjectBody,
) -> Response {
    match tokio::task::spawn_blocking(move || create_record(&state, body)).await {
        Ok(Ok(profile)) => Json(profile).into_response(),
        Ok(Err(error)) => profile_errors::storage(error),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response(),
    }
}
pub(super) async fn update(
    State(state): State<Arc<Profiles>>,
    Path(id): Path<String>,
    ObjectBody(body): ObjectBody,
) -> Response {
    match tokio::task::spawn_blocking(move || update_record(&state, &id, body)).await {
        Ok(Ok(Some(profile))) => Json(profile).into_response(),
        Ok(Ok(None)) => profile_errors::detail(StatusCode::NOT_FOUND, "profile not found"),
        Ok(Err(error)) => profile_errors::storage(error),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response(),
    }
}
