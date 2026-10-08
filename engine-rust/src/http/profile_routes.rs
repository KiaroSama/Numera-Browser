use super::{cors, middleware::protect, request_auth::RequestAuth, tokens::TokenStore};
use crate::{
    conversion::{to_dashboard, truth},
    ownership::claims,
    storage::{
        dashboard::DashboardStore,
        locks::{DataLock, SessionLease},
        paths::safe_id,
        repo::{ProfileRepo, RepoError},
    },
};
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use serde_json::Value;
use std::{
    io,
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub(super) struct Profiles {
    pub(super) root: PathBuf,
    pub(super) dashboard: DashboardStore,
    engine: ProfileRepo,
    host: String,
    pub(super) installed: std::collections::BTreeMap<String, bool>,
}
pub fn router(root: PathBuf, environment: &str, host: &str) -> Router {
    let auth = Arc::new(RequestAuth::new(TokenStore::new(root.clone()), environment));
    protect(
        cors::apply(routes(root, None, host, Default::default()), ""),
        auth,
    )
}
pub(crate) fn routes(
    root: PathBuf,
    password: Option<String>,
    host: &str,
    installed: std::collections::BTreeMap<String, bool>,
) -> Router {
    let state = Arc::new(Profiles {
        dashboard: DashboardStore::new(root.clone(), password.clone()),
        engine: ProfileRepo::new(root.clone(), password),
        root,
        host: host.into(),
        installed,
    });
    Router::new()
        .route(
            "/api/profiles",
            get(list)
                .head(super::application::method_not_allowed)
                .post(super::profile_write::create),
        )
        .route(
            "/api/profiles/{pid}",
            axum::routing::put(super::profile_write::update),
        )
        .route(
            "/api/profiles/health",
            get(super::profile_overview::health).head(super::application::method_not_allowed),
        )
        .route(
            "/api/profiles/collisions",
            get(super::profile_overview::collisions).head(super::application::method_not_allowed),
        )
        .with_state(state)
}
const TEXT_FIELDS: &[&str] = &[
    "name",
    "notes",
    "folder",
    "engine",
    "os",
    "userAgent",
    "screen",
    "locale",
    "timezone",
    "browserVersion",
    "startupUrls",
    "extensions",
    "bookmarks",
    "launchArgs",
    "dns",
];
const LIST_FIELDS: &[&str] = &["tags", "ipBased", "clearOnStart", "switchedOff"];
pub(super) fn readable(mut profile: Value) -> Result<Value, RepoError> {
    let object = profile.as_object_mut().ok_or_else(|| {
        RepoError::InvalidRecord("Stored dashboard profile is not an object".into())
    })?;
    for (field, value) in object.iter_mut() {
        if TEXT_FIELDS.contains(&field.as_str()) {
            *value = match value {
                Value::String(_) => continue,
                Value::Bool(value) => value.to_string().into(),
                Value::Number(value) => value.to_string().into(),
                _ => "".into(),
            };
        } else if LIST_FIELDS.contains(&field.as_str()) {
            *value = Value::Array(
                value
                    .as_array()
                    .map(|values| {
                        values
                            .iter()
                            .filter(|value| value.is_string())
                            .cloned()
                            .collect()
                    })
                    .unwrap_or_default(),
            );
        }
    }
    object.entry("name").or_insert("".into());
    Ok(profile)
}
fn relative_time(value: &Value, now: f64) -> Result<String, RepoError> {
    if !truth(value) {
        return Ok("never".into());
    }
    let timestamp = value.as_f64().ok_or_else(|| {
        RepoError::InvalidRecord("Stored activity timestamp is not numeric".into())
    })?;
    let seconds = (now - timestamp).max(0.0);
    Ok(if seconds < 60.0 {
        "just now".into()
    } else if seconds < 3600.0 {
        format!("{}m ago", (seconds / 60.0).trunc())
    } else if seconds < 86400.0 {
        format!("{}h ago", (seconds / 3600.0).trunc())
    } else {
        format!("{}d ago", (seconds / 86400.0).trunc())
    })
}
pub(super) fn decorate(state: &Profiles, mut profile: Value, now: f64) -> Result<Value, RepoError> {
    let id = profile["id"]
        .as_str()
        .ok_or_else(|| RepoError::InvalidRecord("Stored profile ID is not text".into()))?;
    let held = match SessionLease::acquire(&SessionLease::path_for(&state.root, id)?) {
        Ok(lease) => {
            drop(lease);
            false
        }
        Err(error) if error.kind() == io::ErrorKind::WouldBlock => true,
        Err(error) => return Err(error.into()),
    };
    let running =
        held || claims::state(&claims::read(&state.root, id), &state.host) != claims::State::Free;
    let label = if running {
        "now".into()
    } else {
        relative_time(&profile["_last_active"], now)?
    };
    profile["status"] = if running { "running" } else { "stopped" }.into();
    profile["lastActive"] = label.into();
    Ok(profile)
}
fn listing(state: &Profiles) -> Result<Vec<Value>, RepoError> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_secs_f64();
    let lock = DataLock::transaction(&state.root)?;
    let _guard = lock.acquire(Duration::from_secs(30))?;
    let mut profiles = state.dashboard.list()?;
    let known = profiles
        .iter()
        .filter_map(|profile| profile["id"].as_str())
        .map(str::to_owned)
        .collect::<std::collections::BTreeSet<_>>();
    let entries = match std::fs::read_dir(state.root.join("profiles")) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return profiles
                .into_iter()
                .map(|profile| decorate(state, readable(profile)?, now))
                .collect();
        }
        Err(error) => return Err(error.into()),
    };
    let mut paths = entries
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()?;
    paths.sort();
    let mut engines = Vec::new();
    for path in paths {
        if path.extension().is_none_or(|extension| extension != "json") {
            continue;
        }
        let Some(id) = path
            .file_stem()
            .and_then(|id| id.to_str())
            .filter(|id| safe_id(id).is_ok())
        else {
            continue;
        };
        // Source union listing first creates the missing twin, then the HTTP
        // batch sees that revision. Do not materialize dashboard-known records.
        if !known.contains(id) {
            match state.engine.adopt(id, now) {
                Ok(_) => {}
                Err(RepoError::InvalidRecord(_)) => continue,
                Err(error) => return Err(error),
            }
            match state.engine.get(id) {
                Ok(Some(profile)) => engines.push(profile),
                Ok(None) | Err(RepoError::InvalidRecord(_)) => {}
                Err(error) => return Err(error),
            }
        }
    }
    engines.sort_by(|left, right| left.created_at.total_cmp(&right.created_at));
    let mut adopting = Vec::new();
    for profile in engines {
        if state
            .root
            .join("profiles")
            .join(format!("{}.json", profile.id))
            .try_exists()?
        {
            let mut record = to_dashboard(&profile);
            record["id"] = profile.id.into();
            adopting.push(record);
        }
    }
    if !adopting.is_empty() {
        profiles.extend(
            state
                .dashboard
                .save_many(adopting, Default::default(), None, now)?,
        );
    }
    profiles
        .into_iter()
        .map(|profile| decorate(state, readable(profile)?, now))
        .collect()
}
async fn list(State(state): State<Arc<Profiles>>) -> Response {
    match tokio::task::spawn_blocking(move || listing(&state)).await {
        Ok(Ok(profiles)) => Json(profiles).into_response(),
        Ok(Err(error)) => super::profile_errors::storage(error),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response(),
    }
}
