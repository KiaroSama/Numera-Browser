//! Composition of implemented routes, not a complete executable HTTP server.
use super::{
    config_routes, cors, diagnostics, extension_routes, fingerprint_routes, folder_routes,
    middleware::protect, profile_routes, proxy_routes, request_auth::RequestAuth, schedule_routes,
    token_routes, tokens::TokenStore,
};
use axum::{
    Json, Router,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;
use std::{
    collections::BTreeMap,
    io,
    path::PathBuf,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

pub async fn router(
    root: PathBuf,
    environment: &str,
    origins: &str,
    password: Option<String>,
    host: &str,
    installed: BTreeMap<String, bool>,
) -> io::Result<Router> {
    if host.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Session host identity is missing",
        ));
    }
    let auth = Arc::new(RequestAuth::new(TokenStore::new(root.clone()), environment));
    let adopting = auth.clone();
    tokio::task::spawn_blocking(move || {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| io::Error::other("System clock precedes the Unix epoch"))?
            .as_secs_f64();
        adopting.adopt(now)
    })
    .await
    .map_err(|_| io::Error::other("Authentication startup task failed"))??;
    let routes = diagnostics::routes(auth.clone())
        .merge(token_routes::routes(root.clone(), environment))
        .merge(folder_routes::routes(root.clone()))
        .merge(fingerprint_routes::routes())
        .merge(profile_routes::routes(
            root.clone(),
            password.clone(),
            host,
            installed.clone(),
        ))
        .merge(extension_routes::routes(root.clone()))
        .merge(proxy_routes::routes(root.clone(), password.clone()))
        .merge(config_routes::routes(
            root.clone(),
            password.clone(),
            installed,
        ))
        .merge(schedule_routes::routes(root, password))
        .method_not_allowed_fallback(method_not_allowed);
    Ok(protect(cors::apply(routes, origins), auth))
}
pub(crate) async fn method_not_allowed() -> Response {
    (
        StatusCode::METHOD_NOT_ALLOWED,
        Json(json!({"detail":"Method Not Allowed"})),
    )
        .into_response()
}
