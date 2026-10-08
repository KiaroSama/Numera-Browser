use super::{profile_errors, profile_fields, profile_routes::Profiles, request_body::ObjectBody};
use crate::{
    conversion::{to_engine, truth},
    identity, importers,
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
    io,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

fn named(record: &Value) -> Option<String> {
    ["engine", "launcher", "browserEngine"]
        .into_iter()
        .filter_map(|key| record[key].as_str())
        .map(|name| name.trim().to_lowercase())
        .find(|name| ["camoufox", "cloak", "patchright", "playwright"].contains(&name.as_str()))
}
fn native(raw: &Value) -> Result<Option<Vec<Value>>, String> {
    let Some(version) = raw
        .as_object()
        .and_then(|record| record.get("numeraProfiles"))
    else {
        return Ok(None);
    };
    if version != &json!(1) && version != &json!(1.0) && version != &json!(true) {
        let next = if validation::kind(version) == "int"
            && version.as_i64().is_some_and(|version| version > 1)
        {
            "It was written by a newer Numera - update this install, or re-export from that one."
        } else {
            "Re-export it from the install that wrote it."
        };
        return Err(format!(
            "this file says it is Numera export format {}; this build reads format 1. {next}",
            validation::repr(version)
        ));
    }
    let entries = raw["profiles"]
        .as_array()
        .filter(|entries| entries.iter().all(Value::is_object))
        .ok_or("a Numera export must carry 'profiles' as a list of objects")?;
    Ok(Some(entries.clone()))
}
fn unwrap(raw: &Value) -> Vec<Value> {
    if let Some(entries) = raw.as_array() {
        return entries
            .iter()
            .filter(|entry| entry.is_object())
            .cloned()
            .collect();
    }
    if raw.is_object() {
        for key in ["data", "profiles", "list", "items", "rows"] {
            if let Some(entries) = raw[key].as_array() {
                return entries
                    .iter()
                    .filter(|entry| entry.is_object())
                    .cloned()
                    .collect();
            }
        }
        return vec![raw.clone()];
    }
    Vec::new()
}
fn looks_native(record: &Value) -> bool {
    [
        "webglVendor",
        "webglRenderer",
        "clientRects",
        "webglImage",
        "mediaDevices",
        "mediaVideo",
        "mediaAudio",
        "mediaSpeakers",
        "fontList",
        "startupUrls",
        "launchArgs",
        "clearOnStart",
        "switchedOff",
        "ipBased",
        "devicePixelRatio",
        "maxTouchPoints",
        "osVersion",
        "deviceModel",
        "lastActive",
    ]
    .iter()
    .filter(|key| record.get(**key).is_some())
    .count()
        >= 6
}
enum Failure {
    Bad(String),
    Store(RepoError),
}
impl From<RepoError> for Failure {
    fn from(error: RepoError) -> Self {
        Self::Store(error)
    }
}
fn parse(error: serde_json::Error) -> Failure {
    Failure::Store(RepoError::Io(io::Error::other(error)))
}
fn read(state: &Profiles, body: Map<String, Value>) -> Result<Value, Failure> {
    let text = body
        .get("text")
        .filter(|value| truth(value))
        .cloned()
        .unwrap_or_else(|| "".into());
    let text = text.as_str().ok_or_else(|| {
        Failure::Bad(format!(
            "'text' must be a string, got {}",
            validation::kind(&text)
        ))
    })?;
    let raw: Value = serde_json::from_str(text)
        .map_err(|_| Failure::Bad("that file isn't valid JSON".into()))?;
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| RepoError::Io(io::Error::other(error)))?
        .as_secs_f64();
    let config = ConfigStore::new(state.root.clone());
    if let Some(mut entries) = native(&raw).map_err(Failure::Bad)? {
        let mut problems = Vec::new();
        for (index, entry) in entries.iter_mut().enumerate() {
            profile_fields::strip_server_owned(
                entry.as_object_mut().expect("native object checked"),
            )
            .map_err(parse)?;
            problems.extend(
                profile_fields::check(entry, &format!("profiles[{index}]"), &[]).map_err(parse)?,
            );
        }
        if !problems.is_empty() {
            return Err(Failure::Bad(
                problems.into_iter().take(8).collect::<Vec<_>>().join("; "),
            ));
        }
        let mut results = Vec::new();
        for entry in &mut entries {
            let redacted = truth(&entry["proxy"]["host"]) && entry["proxy"]["pass"].is_null();
            let chose = truth(&entry["engine"]);
            *entry = profile_fields::complete(
                entry.take(),
                &random_hex(6).map_err(RepoError::from)?,
                None,
                time,
            )
            .map_err(RepoError::UnconvertibleProfile)?;
            if !chose {
                entry["engine"] = config.default_engine(&state.installed);
            }
            let mut issues = identity::validate(
                &to_engine(entry, time)
                    .map_err(|error| RepoError::UnconvertibleProfile(error.to_string()))?
                    .fingerprint,
            )
            .map_err(parse)?;
            if redacted {
                issues.push("the proxy password was not in this file - the vault was holding it when the export was written; set it again on this profile".into());
            }
            results.push(json!({"name":entry["name"],"issues":issues}));
        }
        state.dashboard.save_many(entries, Map::new(), None, time)?;
        return Ok(json!({"imported":results.len(),"format":"numera","results":results}));
    }
    let exports = unwrap(&raw);
    if exports.is_empty() {
        return Err(Failure::Bad("no profiles found in that file".into()));
    }
    if exports.iter().any(looks_native) {
        return Err(Failure::Bad("this looks like a Numera profile export written before the format carried a version, and reading it with the vendor importer would drop its fingerprint seed, signal modes, startup URLs, extensions and folder. Re-export it from the install that wrote it.".into()));
    }
    let mut results = Vec::new();
    let mut pending = Vec::new();
    let mut explicit = None;
    let mut problems = Vec::new();
    for (index, export) in exports.iter().enumerate() {
        let mut bytes = [0u8; 4];
        getrandom::fill(&mut bytes).map_err(|error| RepoError::Io(io::Error::other(error)))?;
        let seed = i64::from((u32::from_ne_bytes(bytes) & 0x7fff_ffff).max(1));
        let (mut record, issues) = match importers::profile(export, seed, time) {
            Ok(value) => value,
            Err(error) => {
                problems.push(format!(
                    "profiles[{index}]: this export cannot be read: {error}"
                ));
                continue;
            }
        };
        let mut checked = record
            .as_object()
            .cloned()
            .ok_or_else(|| RepoError::Io(io::Error::other("Imported profile is not an object")))?;
        profile_fields::strip_server_owned(&mut checked).map_err(parse)?;
        problems.extend(
            profile_fields::check(&Value::Object(checked), &format!("profiles[{index}]"), &[])
                .map_err(parse)?,
        );
        if let Some(engine) = named(export) {
            record["engine"] = engine.clone().into();
            explicit = Some(engine);
        }
        results.push(json!({"name":record["name"],"issues":issues}));
        pending.push(record);
    }
    if !problems.is_empty() {
        return Err(Failure::Bad(
            problems.into_iter().take(8).collect::<Vec<_>>().join("; "),
        ));
    }
    let changes = explicit
        .map(|engine| Map::from_iter([("default_engine".into(), engine.into())]))
        .unwrap_or_default();
    state.dashboard.save_many(pending, changes, None, time)?;
    Ok(json!({"imported":results.len(),"format":"vendor","results":results}))
}
pub(super) async fn import(
    State(state): State<Arc<Profiles>>,
    ObjectBody(body): ObjectBody,
) -> Response {
    match tokio::task::spawn_blocking(move || read(&state, body)).await {
        Ok(Ok(value)) => Json(value).into_response(),
        Ok(Err(Failure::Bad(message))) => profile_errors::detail(StatusCode::BAD_REQUEST, message),
        Ok(Err(Failure::Store(error))) => profile_errors::storage(error),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response(),
    }
}
