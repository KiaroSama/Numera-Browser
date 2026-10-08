use super::{
    profile_errors, profile_fields,
    profile_routes::{Profiles, decorate},
    profile_write,
    request_body::ObjectBody,
};
use crate::{
    conversion::{secret_intent, truth},
    devices,
    storage::{archive_paths::random_hex, config::ConfigStore, repo::RepoError},
    validation,
};
use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::{Map, Value, json};
use std::{
    collections::BTreeSet,
    io,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

enum Failure {
    Bad(String),
    Store(RepoError),
}
impl From<RepoError> for Failure {
    fn from(error: RepoError) -> Self {
        Self::Store(error)
    }
}
fn schema(error: serde_json::Error) -> Failure {
    Failure::Store(RepoError::Io(io::Error::other(error)))
}
fn clock() -> Result<f64, RepoError> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_secs_f64())
}
fn create(state: &Profiles, body: Map<String, Value>) -> Result<Value, Failure> {
    let incoming = body
        .get("profiles")
        .filter(|value| truth(value))
        .cloned()
        .unwrap_or_else(|| json!([]));
    let mut incoming = match incoming {
        Value::Array(rows) => rows,
        value => {
            return Err(Failure::Bad(format!(
                "'profiles' must be a list, got {}",
                validation::kind(&value)
            )));
        }
    };
    if incoming.iter().any(|value| !value.is_object()) {
        return Err(Failure::Bad(
            "every entry in 'profiles' must be an object".into(),
        ));
    }
    let mut problems = Vec::new();
    for (index, record) in incoming.iter_mut().enumerate() {
        profile_fields::strip_server_owned(record.as_object_mut().expect("entry object checked"))
            .map_err(schema)?;
        problems.extend(
            profile_fields::check(record, &format!("profiles[{index}]"), &[]).map_err(schema)?,
        );
    }
    if !problems.is_empty() {
        return Err(Failure::Bad(
            problems.into_iter().take(8).collect::<Vec<_>>().join("; "),
        ));
    }
    let last = incoming
        .iter()
        .filter_map(|record| record.get("engine").filter(|value| truth(value)))
        .next_back()
        .cloned();
    let config = ConfigStore::new(state.root.clone());
    let default = config.default_engine(&state.installed);
    let time = clock()?;
    for record in &mut incoming {
        *record = profile_fields::complete(
            record.take(),
            &random_hex(6).map_err(RepoError::from)?,
            Some(&default),
            time,
        )
        .map_err(RepoError::UnconvertibleProfile)?;
        profile_write::extensions(record, &config)?;
    }
    let changes = last
        .map(|engine| Map::from_iter([("default_engine".into(), engine)]))
        .unwrap_or_default();
    let saved = state.dashboard.save_many(incoming, changes, None, time)?;
    let ids = saved
        .iter()
        .map(|record| record["id"].clone())
        .collect::<Vec<_>>();
    Ok(json!({"created":ids.len(),"ids":ids}))
}
fn update(state: &Profiles, body: Map<String, Value>) -> Result<Value, Failure> {
    let ids = body
        .get("ids")
        .filter(|value| truth(value))
        .cloned()
        .unwrap_or_else(|| json!([]));
    let ids = ids
        .as_array()
        .filter(|rows| rows.iter().all(Value::is_string))
        .ok_or_else(|| Failure::Bad("'ids' must be a list of profile ids".into()))?;
    let ids = ids
        .iter()
        .map(|value| value.as_str().expect("ID string checked").to_owned())
        .collect::<Vec<_>>();
    let patch = body
        .get("patch")
        .filter(|value| truth(value))
        .cloned()
        .unwrap_or_else(|| json!({}));
    let mut patch = match patch {
        Value::Object(patch) => patch,
        value => {
            return Err(Failure::Bad(format!(
                "'patch' must be an object, got {}",
                validation::kind(&value)
            )));
        }
    };
    profile_fields::strip_server_owned(&mut patch).map_err(schema)?;
    let mut problems = profile_fields::check(
        &Value::Object(patch.clone()),
        "patch",
        &["addTags", "removeTags"],
    )
    .map_err(schema)?;
    for field in ["addTags", "removeTags"] {
        if let Some(value) = patch.get(field) {
            match value.as_array() {
                None => problems.push(format!("patch: '{field}' must be a list")),
                Some(rows) => {
                    if let Some(value) = rows.iter().find(|value| !value.is_string()) {
                        problems.push(format!(
                            "patch: every entry in '{field}' must be a string, got {}",
                            validation::kind(value)
                        ));
                    }
                }
            }
        }
    }
    if !problems.is_empty() {
        return Err(Failure::Bad(problems.join("; ")));
    }
    let changes = patch
        .get("engine")
        .filter(|value| truth(value))
        .map(|engine| Map::from_iter([("default_engine".into(), engine.clone())]))
        .unwrap_or_default();
    let intent = secret_intent(&Value::Object(patch.clone()));
    let time = clock()?;
    let catalog = devices::catalog().map_err(schema)?;
    let saved = state.dashboard.patch_many(
        &ids,
        |id, mut record| {
            for (field, value) in &patch {
                match field.as_str() {
                    "addTags" => {
                        let mut tags = record["tags"].as_array().cloned().unwrap_or_default();
                        tags.extend(value.as_array().expect("patch tags checked").clone());
                        let tags = tags
                            .iter()
                            .map(|tag| {
                                tag.as_str().ok_or_else(|| {
                                    RepoError::Io(io::Error::other("Stored tag is not text"))
                                })
                            })
                            .collect::<Result<BTreeSet<_>, _>>()?;
                        record["tags"] = json!(tags);
                    }
                    "removeTags" => {
                        let removed = value.as_array().expect("patch tags checked");
                        let tags = record["tags"]
                            .as_array()
                            .cloned()
                            .unwrap_or_default()
                            .into_iter()
                            .filter(|tag| !removed.contains(tag))
                            .collect::<Vec<_>>();
                        record["tags"] = json!(tags);
                    }
                    "locale" => {
                        record["locale"] = value.clone();
                        if let Some(locale) = value.as_str()
                            && let Some(zone) = catalog["catalog"]["LOCALES"][locale].get(0)
                        {
                            record["timezone"] = zone.clone();
                        }
                    }
                    _ => {
                        record[field] = value.clone();
                    }
                }
            }
            profile_write::check(&record, true)?;
            profile_fields::complete(record, id, None, time)
                .map(Some)
                .map_err(RepoError::UnconvertibleProfile)
        },
        changes,
        Some(intent),
        time,
    )?;
    Ok(Value::Array(
        saved
            .into_iter()
            .map(|record| decorate(state, record, time))
            .collect::<Result<Vec<_>, _>>()?,
    ))
}
fn response(result: Result<Value, Failure>) -> Response {
    match result {
        Ok(value) => Json(value).into_response(),
        Err(Failure::Bad(message)) => profile_errors::detail(StatusCode::BAD_REQUEST, message),
        Err(Failure::Store(error)) => profile_errors::storage(error),
    }
}
pub(super) async fn batch(
    State(state): State<Arc<Profiles>>,
    ObjectBody(body): ObjectBody,
) -> Response {
    match tokio::task::spawn_blocking(move || create(&state, body)).await {
        Ok(result) => response(result),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response(),
    }
}
pub(super) async fn bulk(
    State(state): State<Arc<Profiles>>,
    ObjectBody(body): ObjectBody,
) -> Response {
    match tokio::task::spawn_blocking(move || update(&state, body)).await {
        Ok(result) => response(result),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response(),
    }
}
