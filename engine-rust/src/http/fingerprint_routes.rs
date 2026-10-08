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
    match tokio::task::spawn_blocking(move || {
        let profile = match crate::conversion::to_engine(&Value::Object(body), 0.0) {
            Ok(profile) => profile,
            Err(error) => return Ok(vec![format!("could not validate: {error}")]),
        };
        crate::identity::validate(&profile.fingerprint)
    })
    .await
    {
        Ok(Ok(issues)) => Json(json!({"issues":issues})).into_response(),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response(),
    }
}
