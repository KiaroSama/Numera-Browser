use super::{
    cors, middleware::protect, request_auth::RequestAuth, request_body::ObjectBody,
    tokens::TokenStore,
};
use axum::{
    Json, Router,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
};
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc};

pub fn router(root: PathBuf, environment: &str) -> Router {
    let auth = Arc::new(RequestAuth::new(TokenStore::new(root), environment));
    protect(cors::apply(routes(), ""), auth)
}
pub(crate) fn routes() -> Router {
    Router::new().route("/api/fingerprint/validate", post(validate))
}
async fn validate(ObjectBody(body): ObjectBody) -> Response {
    match super::admission::blocking(move || {
        crate::conversion::dashboard_coherence(&Value::Object(body), 0.0)
    })
    .await
    {
        Ok(issues) => Json(json!({"issues":issues})).into_response(),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response(),
    }
}
