use super::{
    cors, middleware::protect, request_auth::RequestAuth, request_body::ObjectBody,
    tokens::TokenStore,
};
use crate::{
    conversion::truth,
    storage::{config::ConfigStore, dashboard::DashboardStore, pool::ProxyPool},
};
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use serde_json::{Map, Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    path::PathBuf,
    sync::Arc,
};

struct Settings {
    root: PathBuf,
    config: ConfigStore,
    dashboard: DashboardStore,
    pool: ProxyPool,
    installed: BTreeMap<String, bool>,
}
pub fn router(root: PathBuf, environment: &str, installed: BTreeMap<String, bool>) -> Router {
    let auth = Arc::new(RequestAuth::new(TokenStore::new(root.clone()), environment));
    protect(cors::apply(routes(root, None, installed), ""), auth)
}
pub(crate) fn routes(
    root: PathBuf,
    password: Option<String>,
    installed: BTreeMap<String, bool>,
) -> Router {
    let state = Arc::new(Settings {
        config: ConfigStore::new(root.clone()),
        dashboard: DashboardStore::new(root.clone(), password.clone()),
        pool: ProxyPool::new(root.clone(), password),
        root,
        installed,
    });
    Router::new()
        .route(
            "/api/config",
            get(read)
                .head(super::application::method_not_allowed)
                .put(write),
        )
        .route(
            "/api/engines",
            get(engines).head(super::application::method_not_allowed),
        )
        .with_state(state)
}
async fn engines(State(state): State<Arc<Settings>>) -> Json<Value> {
    Json(json!(state.installed))
}
fn view(state: &Settings) -> Result<Value, ()> {
    let config = state.config.read().map_err(|_| ())?;
    let engine = state
        .config
        .default_engine(&state.installed)
        .map_err(|_| ())?;
    let extensions = state.config.default_extensions().map_err(|_| ())?;
    let meta = state.dashboard.get_meta().map_err(|_| ())?;
    let metadata = meta.as_object().ok_or(())?;
    let mut folders = match metadata.get("folders") {
        None => Vec::new(),
        Some(Value::Array(rows))
            if rows
                .iter()
                .all(|value| !value.is_array() && !value.is_object()) =>
        {
            rows.clone()
        }
        Some(Value::String(text)) => text.chars().map(|c| Value::String(c.to_string())).collect(),
        Some(Value::Object(object)) => object.keys().cloned().map(Value::String).collect(),
        _ => return Err(()),
    };
    let used = state
        .dashboard
        .list()
        .map_err(|_| ())?
        .into_iter()
        .filter_map(|profile| profile.get("folder").filter(|value| truth(value)).cloned())
        .map(|value| value.as_str().map(str::to_owned).ok_or(()))
        .collect::<Result<BTreeSet<_>, _>>()?;
    folders.extend(used.into_iter().map(Value::String));
    let mut seen = HashSet::new();
    folders.retain(|value| {
        let key = match value {
            Value::Bool(true) => "number:1".into(),
            Value::Bool(false) => "number:0".into(),
            Value::Number(number) => {
                let raw = number.to_string();
                if !raw.contains(['.', 'e', 'E']) {
                    format!("number:{raw}")
                } else if let Some(float) = number
                    .as_f64()
                    .filter(|float| float.is_finite() && float.fract() == 0.0)
                {
                    if float == 0.0 {
                        "number:0".into()
                    } else {
                        format!("number:{float:.0}")
                    }
                } else {
                    format!("float:{number}")
                }
            }
            _ => format!("value:{value}"),
        };
        seen.insert(key)
    });
    Ok(
        json!({"default_engine":engine,"default_extensions":extensions,
        "headless_launch":config.get("headless_launch").is_some_and(truth),
        "folders":folders,"root":state.root.to_string_lossy(),"version":env!("CARGO_PKG_VERSION"),
        "proxyCount":state.pool.list().map_err(|_| ())?.len()}),
    )
}
async fn read(State(state): State<Arc<Settings>>) -> Response {
    match super::admission::blocking(move || view(&state)).await {
        Ok(Ok(value)) => Json(value).into_response(),
        _ => failure(),
    }
}
fn patch(
    body: Map<String, Value>,
    installed: &BTreeMap<String, bool>,
) -> Result<Map<String, Value>, String> {
    let mut unknown = body
        .keys()
        .filter(|key| {
            !["default_engine", "default_extensions", "headless_launch"].contains(&key.as_str())
        })
        .cloned()
        .collect::<Vec<_>>();
    unknown.sort();
    if !unknown.is_empty() {
        return Err(format!("unknown config key(s): {}", unknown.join(", ")));
    }
    let mut clean = Map::new();
    if let Some(engine) = body.get("default_engine").filter(|value| !value.is_null()) {
        if !engine
            .as_str()
            .is_some_and(|name| installed.contains_key(name))
        {
            return Err(format!(
                "unknown engine '{}'",
                crate::conversion::value_text(engine)
            ));
        }
        clean.insert("default_engine".into(), engine.clone());
    }
    if let Some(raw) = body
        .get("default_extensions")
        .filter(|value| !value.is_null())
    {
        let extensions = match raw {
            Value::String(text) => text
                .lines()
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .map(|text| Value::String(text.into()))
                .collect(),
            Value::Array(rows) => rows.clone(),
            _ => return Err("default_extensions must be a list of folder paths".into()),
        };
        let entries = extensions
            .iter()
            .map(crate::conversion::value_text)
            .collect::<Vec<_>>();
        if let Some(problem) = crate::storage::extensions::unloadable(&entries)
            .into_iter()
            .next()
        {
            return Err(problem);
        }
        clean.insert(
            "default_extensions".into(),
            Value::Array(entries.into_iter().map(Value::String).collect()),
        );
    }
    if let Some(headless) = body.get("headless_launch").filter(|value| !value.is_null()) {
        if !headless.is_boolean() {
            return Err(format!(
                "'headless_launch' must be true or false, got {}",
                crate::validation::kind(headless)
            ));
        }
        clean.insert("headless_launch".into(), headless.clone());
    }
    Ok(clean)
}
async fn write(State(state): State<Arc<Settings>>, ObjectBody(body): ObjectBody) -> Response {
    match super::admission::blocking(move || {
        let clean = match patch(body, &state.installed) {
            Ok(clean) => clean,
            Err(message) => {
                return Ok(
                    (StatusCode::BAD_REQUEST, Json(json!({"detail":message}))).into_response()
                );
            }
        };
        state.config.set_many(clean).map_err(|_| ())?;
        Ok::<_, ()>(Json(view(&state)?).into_response())
    })
    .await
    {
        Ok(Ok(response)) => response,
        _ => failure(),
    }
}
fn failure() -> Response {
    (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response()
}
