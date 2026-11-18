use super::{
    cors, middleware::protect, request_auth::RequestAuth, request_body::ObjectBody,
    token_errors::TokenError, tokens::TokenStore,
};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get},
};
use serde_json::json;
use std::{path::PathBuf, sync::Arc};

struct TokenState {
    tokens: TokenStore,
    environment: bool,
}
pub fn router(root: PathBuf, environment: &str) -> Router {
    let auth = Arc::new(RequestAuth::new(TokenStore::new(root.clone()), environment));
    protect(cors::apply(routes(root, environment), ""), auth)
}
pub(crate) fn routes(root: PathBuf, environment: &str) -> Router {
    let state = Arc::new(TokenState {
        tokens: TokenStore::new(root),
        environment: !environment.trim().is_empty(),
    });
    Router::new()
        .route(
            "/api/tokens",
            get(list)
                .head(super::application::method_not_allowed)
                .post(create),
        )
        .route("/api/tokens/{id}", delete(revoke))
        .with_state(state)
}
async fn list(State(state): State<Arc<TokenState>>) -> Response {
    match super::admission::blocking(move || {
        let mut rows = state.tokens.list();
        for row in &mut rows {
            let object = row.as_object_mut().ok_or(())?;
            object.remove("hash");
        }
        Ok::<_, ()>(json!({"tokens":rows,"envToken":state.environment}))
    })
    .await
    {
        Ok(Ok(value)) => Json(value).into_response(),
        _ => failure(),
    }
}
async fn create(State(state): State<Arc<TokenState>>, ObjectBody(body): ObjectBody) -> Response {
    let name = body
        .get("name")
        .filter(|value| crate::conversion::truth(value))
        .map(crate::conversion::value_text)
        .unwrap_or_default();
    let name = name.trim().to_owned();
    if name.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({"detail":"give the token a name - an unnamed token is one nobody can safely revoke later"}))).into_response();
    }
    let now = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(duration) => duration.as_secs_f64(),
        Err(_) => return failure(),
    };
    match super::admission::blocking(move || state.tokens.create(&name, now)).await {
        Ok(Ok(value)) => Json(value).into_response(),
        Ok(Err(error @ TokenError::DuplicateName(_))) => (
            StatusCode::CONFLICT,
            Json(json!({"detail":error.to_string()})),
        )
            .into_response(),
        _ => failure(),
    }
}
async fn revoke(State(state): State<Arc<TokenState>>, Path(id): Path<String>) -> Response {
    match super::admission::blocking(move || {
        state
            .tokens
            .revoke(&id, !state.environment)
            .map(|removed| (removed, state.tokens.list().len()))
    })
    .await
    {
        Ok(Ok((true, remaining))) => Json(json!({"ok":true,"remaining":remaining})).into_response(),
        Ok(Ok((false, _))) => (
            StatusCode::NOT_FOUND,
            Json(json!({"detail":"token not found"})),
        )
            .into_response(),
        Ok(Err(error @ TokenError::Lockout)) => (
            StatusCode::CONFLICT,
            Json(json!({"detail":error.to_string()})),
        )
            .into_response(),
        _ => failure(),
    }
}
fn failure() -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"detail":"Token storage operation failed"})),
    )
        .into_response()
}
