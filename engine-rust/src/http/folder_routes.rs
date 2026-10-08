use super::{
    cors, middleware::protect, request_auth::RequestAuth, request_body::ObjectBody,
    tokens::TokenStore,
};
use crate::storage::folders::Folders;
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
};
use serde_json::{Map, Value, json};
use std::{io, path::PathBuf, sync::Arc};

pub fn router(root: PathBuf, environment: &str) -> Router {
    let auth = Arc::new(RequestAuth::new(TokenStore::new(root.clone()), environment));
    protect(cors::apply(routes(root), ""), auth)
}
pub(crate) fn routes(root: PathBuf) -> Router {
    Router::new()
        .route("/api/folders", post(create))
        .route("/api/folders/rename", post(rename))
        .route("/api/folders/delete", post(delete))
        .with_state(Arc::new(Folders::new(root)))
}
async fn create(State(folders): State<Arc<Folders>>, ObjectBody(body): ObjectBody) -> Response {
    mutate(folders, body, "create").await
}
async fn rename(State(folders): State<Arc<Folders>>, ObjectBody(body): ObjectBody) -> Response {
    mutate(folders, body, "rename").await
}
async fn delete(State(folders): State<Arc<Folders>>, ObjectBody(body): ObjectBody) -> Response {
    mutate(folders, body, "delete").await
}
fn text(body: &Map<String, Value>, key: &str) -> io::Result<String> {
    match body
        .get(key)
        .filter(|value| crate::conversion::truth(value))
    {
        None => Ok(String::new()),
        Some(Value::String(text)) => Ok(text.clone()),
        _ => Err(io::Error::other("Folder name is not text")),
    }
}
async fn mutate(
    folders: Arc<Folders>,
    body: Map<String, Value>,
    operation: &'static str,
) -> Response {
    let result = tokio::task::spawn_blocking(move || match operation {
        "create" => folders.create(&text(&body, "name")?),
        "rename" => folders.rename(&text(&body, "from")?, &text(&body, "to")?),
        "delete" => folders.delete(&text(&body, "name")?, Some(&text(&body, "reassignTo")?)),
        _ => Err(io::Error::other("Unknown folder operation")),
    })
    .await;
    match result {
        Ok(Ok(names)) => Json(json!({"folders":names})).into_response(),
        Ok(Err(error)) if error.kind() == io::ErrorKind::InvalidInput => (
            StatusCode::BAD_REQUEST,
            Json(json!({"detail":error.to_string()})),
        )
            .into_response(),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response(),
    }
}
