use super::{
    profile_errors,
    profile_routes::{Profiles, decorate, readable},
};
use crate::{
    collision,
    conversion::{to_engine, truth},
    identity,
    models::Profile,
    storage::{repo::RepoError, schedules::ScheduleStore},
};
use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

fn clock() -> Result<f64, RepoError> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_secs_f64())
}
fn shaped(records: &[Value], now: f64) -> Vec<Profile> {
    records
        .iter()
        .filter_map(|record| to_engine(record, now).ok())
        .collect()
}
fn collisions_view(state: &Profiles) -> Result<Value, RepoError> {
    let profiles = shaped(&state.dashboard.list()?, clock()?);
    Ok(json!({"total":profiles.len(),"groups":collision::find(&profiles)}))
}
fn health_view(state: &Profiles) -> Result<Value, RepoError> {
    let now = clock()?;
    let mut schedules: BTreeMap<String, Value> = BTreeMap::new();
    for row in ScheduleStore::new(state.root.clone()).list()? {
        let id = row["profileId"].as_str().ok_or_else(|| {
            RepoError::Io(io::Error::other("Stored schedule profile ID is not text"))
        })?;
        let interval = |row: &Value| -> Result<f64, RepoError> {
            row.get("everyHours").map_or(Ok(1e9), |value| {
                value.as_f64().ok_or_else(|| {
                    RepoError::Io(io::Error::other("Stored schedule interval is not numeric"))
                })
            })
        };
        if !schedules.contains_key(id) || interval(&row)? < interval(&schedules[id])? {
            schedules.insert(id.to_owned(), row);
        }
    }
    let records = state.dashboard.list()?;
    let mut engines = BTreeMap::new();
    for record in &records {
        let id = record["id"]
            .as_str()
            .ok_or_else(|| RepoError::Io(io::Error::other("Stored profile ID is not text")))?;
        engines.insert(id.to_owned(), to_engine(record, now).ok());
    }
    let profiles = engines.values().flatten().cloned().collect::<Vec<_>>();
    let groups = collision::find(&profiles);
    let colliding = groups
        .iter()
        .flat_map(|group| group["members"].as_array().into_iter().flatten())
        .filter_map(|member| member["id"].as_str())
        .collect::<BTreeSet<_>>();
    let mut summary = BTreeMap::from([
        ("total", 0),
        ("running", 0),
        ("coherent", 0),
        ("withProxy", 0),
        ("scheduled", 0),
        ("graded", 0),
        ("colliding", 0),
        ("needsAttention", 0),
    ]);
    let mut rows = Vec::new();
    for record in records {
        let id = record["id"]
            .as_str()
            .ok_or_else(|| RepoError::Io(io::Error::other("Stored profile ID is not text")))?;
        let issues = match engines.get(id).and_then(Option::as_ref) {
            Some(profile) => identity::validate(&profile.fingerprint).map_err(|error| RepoError::Io(io::Error::other(error)))?,
            None => vec!["this profile cannot be read; open it, correct the highlighted field and save it again".into()],
        };
        let proxy = record
            .get("proxy")
            .filter(|value| value.is_object())
            .cloned()
            .unwrap_or_else(|| json!({}));
        let has_proxy = truth(&proxy["host"]);
        let trust = record.get("_trust").cloned().unwrap_or(Value::Null);
        if truth(&trust) && !trust.is_object() {
            return Err(RepoError::Io(io::Error::other(
                "Stored trust grade is not an object",
            )));
        }
        let schedule = schedules.get(id);
        let decorated = decorate(state, record.clone(), now)?;
        let running = decorated["status"] == "running";
        let collision = colliding.contains(id);
        let attention =
            !issues.is_empty() || matches!(trust["grade"].as_str(), Some("C" | "D")) || collision;
        let read = readable(record.clone())?;
        rows.push(json!({"id":id,"name":read.get("name").filter(|value| truth(value)).cloned().unwrap_or_else(|| id.into()),
            "engine":record.get("engine").cloned().unwrap_or_else(|| "playwright".into()),"os":record.get("os").cloned().unwrap_or_else(|| "Windows".into()),
            "status":decorated["status"],"lastActive":decorated["lastActive"],"coherent":issues.is_empty(),"issues":issues,"collides":collision,
            "proxy":{"set":has_proxy,"country":proxy.get("country").filter(|value| truth(value)).cloned().unwrap_or(Value::Null),"type":if has_proxy {proxy["type"].clone()} else {Value::Null}},
            "trust":trust,"schedule":schedule.map(|row| json!({"enabled":row.get("enabled").cloned().unwrap_or(true.into()),"everyHours":row["everyHours"],"preset":row["preset"]})),"needsAttention":attention}));
        for (key, count) in [
            ("total", true),
            ("running", running),
            ("coherent", issues.is_empty()),
            ("withProxy", has_proxy),
            ("scheduled", schedule.is_some()),
            ("graded", !trust.is_null()),
            ("colliding", collision),
            ("needsAttention", attention),
        ] {
            *summary.get_mut(key).expect("fixed summary field exists") += i32::from(count);
        }
    }
    Ok(json!({"summary":summary,"profiles":rows}))
}
pub(super) async fn health(State(state): State<Arc<Profiles>>) -> Response {
    match super::admission::blocking(move || health_view(&state)).await {
        Ok(Ok(value)) => Json(value).into_response(),
        Ok(Err(error)) => profile_errors::storage(error),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response(),
    }
}
pub(super) async fn collisions(State(state): State<Arc<Profiles>>) -> Response {
    match super::admission::blocking(move || collisions_view(&state)).await {
        Ok(Ok(value)) => Json(value).into_response(),
        Ok(Err(error)) => profile_errors::storage(error),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response(),
    }
}
