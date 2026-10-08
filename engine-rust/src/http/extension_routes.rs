use super::{cors, middleware::protect, request_auth::RequestAuth, tokens::TokenStore};
use crate::{
    conversion::{truth, value_text},
    storage::{config::ConfigStore, extensions},
};
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use serde_json::{Value, json};
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
        .with_state(Arc::new(root))
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
    let config = ConfigStore::new(root).read();
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
    match tokio::task::spawn_blocking(move || listing(root.as_ref().clone())).await {
        Ok(Ok(value)) => Json(value).into_response(),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response(),
    }
}
