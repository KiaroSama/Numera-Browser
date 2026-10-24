use super::request_body::ObjectBody;
use super::{cors, middleware::protect, request_auth::RequestAuth, tokens::TokenStore};
use crate::{
    conversion::{truth, value_text},
    storage::{config::ConfigStore, extension_install, extensions, locks::DataLock},
};
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::{Map, Value, json};
use std::{collections::HashSet, io, path::PathBuf, sync::Arc};

pub fn router(root: PathBuf, environment: &str) -> Router {
    let auth = Arc::new(RequestAuth::new(TokenStore::new(root.clone()), environment));
    protect(cors::apply(routes(root), ""), auth)
}
pub(crate) fn routes(root: PathBuf) -> Router {
    Router::new()
        .route(
            "/api/extensions/installed",
            get(list).head(super::application::method_not_allowed),
        )
        .route("/api/extensions/install", post(install))
        .with_state(Arc::new(root))
}
fn install_sync(root: &std::path::Path, body: Map<String, Value>) -> Result<Value, Box<Response>> {
    let reference = body
        .get("url")
        .filter(|value| truth(value))
        .or_else(|| body.get("id").filter(|value| truth(value)))
        .and_then(Value::as_str)
        .unwrap_or("");
    let result = extension_install::install(
        root,
        reference,
        body.get("force").is_some_and(truth),
        crate::network::extension_download::download_sync,
    )
    .map_err(|error| super::profile_errors::detail(StatusCode::BAD_REQUEST, error.to_string()))?;
    let failure = |_| (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response();
    let lock = DataLock::transaction(root).map_err(failure)?;
    let _guard = lock.acquire_configured().map_err(failure)?;
    let config = ConfigStore::new(root.into());
    let mut defaults = config.default_extensions().map_err(|_| {
        (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response()
    })?;
    let path = serde_json::to_value(&result.path).map_err(|_| {
        (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response()
    })?;
    if body.get("addToDefaults").is_none_or(truth) && !defaults.contains(&path) {
        defaults.push(path);
        config
            .set_many(Map::from_iter([(
                "default_extensions".into(),
                json!(defaults),
            )]))
            .map_err(|_| {
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response()
            })?;
    }
    let mut result = serde_json::to_value(result).map_err(|_| {
        (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response()
    })?;
    result["default_extensions"] = json!(defaults);
    Ok(result)
}
async fn install(State(root): State<Arc<PathBuf>>, ObjectBody(body): ObjectBody) -> Response {
    match super::admission::blocking(move || install_sync(&root, body)).await {
        Ok(Ok(result)) => Json(result).into_response(),
        Ok(Err(response)) => *response,
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response(),
    }
}

fn listing(root: PathBuf) -> io::Result<Value> {
    let installed = extensions::installed(&root)?;
    let seen: HashSet<_> = installed
        .iter()
        .map(|entry| entry.description.path.clone())
        .collect();
    let mut rows = installed
        .into_iter()
        .map(serde_json::to_value)
        .collect::<Result<Vec<_>, _>>()
        .map_err(io::Error::other)?;
    let config = ConfigStore::new(root).read().map_err(|error| match error {
        crate::storage::config::ConfigError::Io(error) => error,
        error => io::Error::other(error),
    })?;
    let defaults = match config
        .get("default_extensions")
        .filter(|value| truth(value))
    {
        None => Vec::new(),
        Some(Value::Array(rows)) => rows.clone(),
        Some(Value::String(text)) => text.chars().map(|c| Value::String(c.to_string())).collect(),
        Some(Value::Object(object)) => object.keys().cloned().map(Value::String).collect(),
        _ => {
            return Err(io::Error::other(
                "Stored extension defaults are not iterable",
            ));
        }
    };
    for entry in defaults {
        let text = value_text(&entry);
        let path = PathBuf::from(&text);
        if seen.contains(&path) || text.contains("://") {
            continue;
        }
        if path.join("manifest.json").is_file() {
            let mut row =
                serde_json::to_value(extensions::describe(&path)?).map_err(io::Error::other)?;
            row["id"] = path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| io::Error::other("Extension folder name is not UTF8"))?
                .into();
            rows.push(row);
        }
    }
    Ok(json!({"extensions":rows}))
}
async fn list(State(root): State<Arc<PathBuf>>) -> Response {
    match super::admission::blocking(move || listing(root.as_ref().clone())).await {
        Ok(Ok(value)) => Json(value).into_response(),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response(),
    }
}
