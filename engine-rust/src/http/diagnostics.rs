use super::{cors, middleware::protect, request_auth::RequestAuth};
use axum::{Json, Router, http::StatusCode, response::IntoResponse, routing::get};
use serde_json::{Value, json};
use std::sync::Arc;

/// Only the real diagnostic routes, to be merged with the remaining application
/// routes. This router is not advertised as a complete replacement server.
pub fn router(auth: Arc<RequestAuth>) -> Router {
    protect(cors::apply(routes(auth.clone()), ""), auth)
}
pub(crate) fn routes(auth: Arc<RequestAuth>) -> Router {
    Router::new()
        .route(
            "/api/ready",
            get(move || ready(auth.clone())).head(method_not_allowed),
        )
        .route("/api/health", get(health).head(method_not_allowed))
        .method_not_allowed_fallback(method_not_allowed)
}
async fn ready(auth: Arc<RequestAuth>) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let required = tokio::task::spawn_blocking(move || auth.required())
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"detail":"Authentication state could not be verified"})),
            )
        })?;
    Ok(Json(
        json!({"ok":true,"service":"numera-browser","version":env!("CARGO_PKG_VERSION"),"authRequired":required}),
    ))
}
async fn health() -> Json<Value> {
    Json(json!({"ok":true,"version":env!("CARGO_PKG_VERSION")}))
}
async fn method_not_allowed() -> impl IntoResponse {
    (
        StatusCode::METHOD_NOT_ALLOWED,
        Json(json!({"detail":"Method Not Allowed"})),
    )
}
