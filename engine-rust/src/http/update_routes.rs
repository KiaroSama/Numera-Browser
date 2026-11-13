//! Shell-private update transactions; API tokens never grant this capability.
use crate::ownership::{
    registry::Registry,
    update_admission::{Admission, GATE},
};
use axum::{
    Json, Router,
    body::to_bytes,
    extract::{Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::post,
};
use serde_json::{Value, json};
use std::{io, sync::Arc};
use subtle::ConstantTimeEq;

pub(crate) fn private_path(path: &str) -> bool {
    matches!(
        path.trim_end_matches('/'),
        "/api/updates/prepare" | "/api/updates/quiesce" | "/api/updates/resume"
    )
}
pub(crate) fn protect(router: Router, token: String) -> Router {
    router.layer(middleware::from_fn_with_state(
        Arc::new(token),
        private_auth,
    ))
}
async fn private_auth(State(token): State<Arc<String>>, request: Request, next: Next) -> Response {
    if !private_path(request.uri().path()) {
        return next.run(request).await;
    }
    let sent = request
        .headers()
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    let supplied = if sent
        .get(..7)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("bearer "))
    {
        &sent[7..]
    } else {
        ""
    };
    if token.is_empty() || token.as_bytes().ct_eq(supplied.as_bytes()).unwrap_u8() != 1 {
        return detail(
            StatusCode::UNAUTHORIZED,
            "invalid or missing private update token",
        );
    }
    if request
        .headers()
        .get("origin")
        .is_some_and(|origin| !origin.as_bytes().is_empty())
    {
        return detail(
            StatusCode::FORBIDDEN,
            "private update calls must originate from the shell",
        );
    }
    next.run(request).await
}
fn detail(status: StatusCode, message: &'static str) -> Response {
    (status, Json(json!({"detail":message}))).into_response()
}
fn failed(error: io::Error) -> Response {
    (StatusCode::CONFLICT,Json(json!({"detail":if error.kind()==io::ErrorKind::WouldBlock{"Active or uncertain work defers this update"}else{"Update state could not be accessed safely"},"kind":"UpdateBusy"}))).into_response()
}
struct Updates {
    admission: Admission,
    sessions: Arc<Registry>,
}
pub(crate) fn routes(admission: Admission, sessions: Arc<Registry>) -> Router {
    Router::new()
        .route("/api/updates/prepare", post(prepare))
        .route("/api/updates/quiesce", post(quiesce))
        .route("/api/updates/resume", post(resume))
        .with_state(Arc::new(Updates {
            admission,
            sessions,
        }))
}
async fn payload(request: Request) -> Result<Value, Box<Response>> {
    let bytes = to_bytes(request.into_body(), 4096)
        .await
        .map_err(|_| Box::new(detail(StatusCode::BAD_REQUEST, "invalid update request")))?;
    serde_json::from_slice(&bytes)
        .map_err(|_| Box::new(detail(StatusCode::BAD_REQUEST, "invalid update request")))
}
async fn prepare(State(state): State<Arc<Updates>>, request: Request) -> Response {
    let body = match payload(request).await {
        Ok(body) => body,
        Err(error) => return *error,
    };
    if !body.as_object().is_some_and(|body| body.is_empty()) {
        return detail(StatusCode::BAD_REQUEST, "prepare requires an empty object");
    }
    Json(json!({"root":state.admission.root(),"gate":GATE})).into_response()
}
async fn quiesce(State(state): State<Arc<Updates>>, request: Request) -> Response {
    let body = match payload(request).await {
        Ok(body) => body,
        Err(error) => return *error,
    };
    if !body
        .as_object()
        .is_some_and(|body| body.len() == 1 && body.contains_key("ticket"))
    {
        return detail(
            StatusCode::BAD_REQUEST,
            "quiesce requires the precommitted recovery ticket",
        );
    }
    let Some(ticket) = body["ticket"].as_str().map(str::to_owned) else {
        return failed(io::Error::new(
            io::ErrorKind::WouldBlock,
            "invalid update ticket",
        ));
    };
    match super::admission::blocking(move || {
        state
            .admission
            .quiesce(Some(&ticket), || state.sessions.update_busy())
            .map(|ticket| json!({"ticket":ticket,"root":state.admission.root(),"gate":GATE}))
    })
    .await
    {
        Ok(Ok(value)) => Json(value).into_response(),
        Ok(Err(error)) => failed(error),
        Err(_) => failed(io::Error::other("Update worker failed")),
    }
}
async fn resume(State(state): State<Arc<Updates>>, request: Request) -> Response {
    let body = match payload(request).await {
        Ok(body) => body,
        Err(error) => return *error,
    };
    if !body
        .as_object()
        .is_some_and(|body| body.len() == 1 && body.contains_key("ticket"))
    {
        return detail(
            StatusCode::BAD_REQUEST,
            "resume requires only the recovery ticket",
        );
    }
    let Some(ticket) = body["ticket"].as_str().map(str::to_owned) else {
        return failed(io::Error::new(
            io::ErrorKind::WouldBlock,
            "invalid update ticket",
        ));
    };
    match super::admission::blocking(move || state.admission.resume(&ticket)).await {
        Ok(Ok(())) => Json(json!({"ok":true})).into_response(),
        Ok(Err(error)) => failed(error),
        Err(_) => failed(io::Error::other("Update worker failed")),
    }
}
