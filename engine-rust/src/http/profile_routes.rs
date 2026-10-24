use super::{cors, middleware::protect, request_auth::RequestAuth, tokens::TokenStore};
use crate::{
    conversion::{to_dashboard, truth},
    storage::{
        dashboard::DashboardStore,
        locks::DataLock,
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
    time::{SystemTime, UNIX_EPOCH},
};

pub(super) struct Profiles {
    pub(super) root: PathBuf,
    pub(super) dashboard: DashboardStore,
    pub(super) engine: ProfileRepo,
    pub(super) host: String,
    pub(super) sessions: Arc<crate::ownership::registry::Registry>,
    pub(super) installed: std::collections::BTreeMap<String, bool>,
    pub(super) browser: Option<crate::browser::runtime::Runtime>,
}
pub fn router(root: PathBuf, environment: &str, host: &str) -> io::Result<Router> {
    let auth = Arc::new(RequestAuth::new(TokenStore::new(root.clone()), environment));
    let sessions = Arc::new(crate::ownership::registry::Registry::new(
        root.clone(),
        host.into(),
        None,
    )?);
    Ok(protect(
        cors::apply(routes(root, None, host, Default::default(), sessions), ""),
        auth,
    ))
}
pub(crate) fn routes(
    root: PathBuf,
    password: Option<String>,
    host: &str,
    installed: std::collections::BTreeMap<String, bool>,
    sessions: Arc<crate::ownership::registry::Registry>,
) -> Router {
    routes_with_browser(root, password, host, installed, sessions, None)
}
pub(crate) fn routes_with_browser(
    root: PathBuf,
    password: Option<String>,
    host: &str,
    installed: std::collections::BTreeMap<String, bool>,
    sessions: Arc<crate::ownership::registry::Registry>,
    browser: Option<crate::browser::runtime::Runtime>,
) -> Router {
    let state = Arc::new(Profiles {
        dashboard: DashboardStore::new(root.clone(), password.clone()),
        engine: ProfileRepo::new(root.clone(), password),
        root,
        host: host.into(),
        installed,
        sessions,
        browser,
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
            axum::routing::put(super::profile_write::update).delete(super::profile_delete::single),
        )
        .route(
            "/api/profiles/health",
            get(super::profile_overview::health).head(super::application::method_not_allowed),
        )
        .route(
            "/api/profiles/collisions",
            get(super::profile_overview::collisions).head(super::application::method_not_allowed),
        )
        .route(
            "/api/profiles/batch",
            axum::routing::post(super::profile_batch::batch),
        )
        .route(
            "/api/profiles/bulk",
            axum::routing::post(super::profile_batch::bulk),
        )
        .route(
            "/api/profiles/import",
            axum::routing::post(super::profile_import::import),
        )
        .route(
            "/api/profiles/{pid}/cookies",
            get(super::cookie_routes::export)
                .head(super::application::method_not_allowed)
                .post(super::cookie_routes::import),
        )
        .route(
            "/api/profiles/{pid}/align",
            axum::routing::post(super::probe_routes::align),
        )
        .route(
            "/api/profiles/{pid}/trust",
            axum::routing::post(super::probe_routes::trust),
        )
        .route(
            "/api/profiles/{pid}/proxy-test",
            axum::routing::post(super::probe_routes::proxy),
        )
        .route(
            "/api/profiles/{pid}/tls",
            axum::routing::post(super::probe_routes::tls),
        )
        .route(
            "/api/profiles/{pid}/dns",
            axum::routing::post(super::probe_routes::dns),
        )
        .route(
            "/api/profiles/{pid}/attach",
            axum::routing::post(super::attach_routes::attach_profile),
        )
        .route(
            "/api/profiles/{pid}/warmup",
            axum::routing::post(super::warmup_routes::warmup_profile),
        )
        .route(
            "/api/profiles/{pid}/launch",
            axum::routing::post(super::launch_routes::launch_profile),
        )
        .route(
            "/api/profiles/{pid}/stop",
            axum::routing::post(super::session_routes::stop_profile),
        )
        .route(
            "/api/profiles/delete",
            axum::routing::post(super::profile_delete::bulk),
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
pub(super) fn readable(profile: Value) -> Result<Value, RepoError> {
    if !profile.is_object() {
        return Err(RepoError::InvalidRecord(
            "Stored dashboard profile is not an object".into(),
        ));
    }
    Ok(crate::read_view::readable_with(
        &profile,
        TEXT_FIELDS,
        LIST_FIELDS,
    ))
}
fn relative_time(value: &Value, now: f64) -> Result<String, RepoError> {
    if !truth(value) {
        return Ok("never".into());
    }
    let timestamp = match value {
        Value::Bool(true) => 1.0,
        _ => value.as_f64().ok_or_else(|| {
            RepoError::InvalidRecord("Stored activity timestamp is not numeric".into())
        })?,
    };
    let seconds = (now - timestamp).max(0.0);
    if seconds < 60.0 {
        return Ok("just now".into());
    }
    let minutes = seconds / 60.0;
    if minutes < 60.0 {
        return Ok(format!("{:.0}m ago", minutes.trunc()));
    }
    let hours = minutes / 60.0;
    Ok(if hours < 24.0 {
        format!("{:.0}h ago", hours.trunc())
    } else {
        format!("{:.0}d ago", (hours / 24.0).trunc())
    })
}
pub(super) fn decorate(state: &Profiles, mut profile: Value, now: f64) -> Result<Value, RepoError> {
    let id = profile["id"]
        .as_str()
        .ok_or_else(|| RepoError::InvalidRecord("Stored profile ID is not text".into()))?;
    let running = state.sessions.running(id)?;
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
    let _guard = lock.acquire_configured()?;
    let mut profiles = state.dashboard.list()?;
    let known = profiles
        .iter()
        .filter_map(|profile| profile["id"].as_str())
        .map(str::to_owned)
        .collect::<std::collections::BTreeSet<_>>();
    let entries = match std::fs::read_dir(state.root.join("profiles")) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            drop(_guard);
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
        if !crate::storage::paths::json_filename(&path) {
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
    drop(_guard);
    profiles
        .into_iter()
        .map(|profile| decorate(state, readable(profile)?, now))
        .collect()
}
async fn list(State(state): State<Arc<Profiles>>) -> Response {
    match super::admission::blocking(move || listing(&state)).await {
        Ok(Ok(profiles)) => Json(profiles).into_response(),
        Ok(Err(error)) => super::profile_errors::storage(error),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response(),
    }
}
