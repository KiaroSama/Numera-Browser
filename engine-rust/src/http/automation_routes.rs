use super::{profile_errors, profile_routes::Profiles};
use crate::{
    browser::automation::{Automation, PageAction},
    storage::repo::RepoError,
};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::post,
};
use serde_json::{Map, Value, json};
use std::{io, sync::Arc};

pub(super) fn routes(state: Arc<Profiles>) -> Router {
    Router::new()
        .route("/api/profiles/{id}/page/{action}", post(page))
        .with_state(state)
}

pub(super) fn automation(state: &Profiles) -> Automation<'_> {
    Automation {
        repo: &state.engine,
        sessions: &state.sessions,
        runtime: state.browser.as_ref(),
    }
}

pub(super) fn result(result: Result<Value, RepoError>) -> Response {
    match result {
        Ok(value) => ([(header::CACHE_CONTROL, "no-store")], Json(value)).into_response(),
        Err(RepoError::Io(error))
            if matches!(
                error.kind(),
                io::ErrorKind::InvalidInput
                    | io::ErrorKind::NotFound
                    | io::ErrorKind::WouldBlock
                    | io::ErrorKind::TimedOut
            ) =>
        {
            let status = match error.kind() {
                io::ErrorKind::InvalidInput => StatusCode::BAD_REQUEST,
                io::ErrorKind::NotFound => StatusCode::NOT_FOUND,
                io::ErrorKind::WouldBlock => StatusCode::CONFLICT,
                _ => StatusCode::GATEWAY_TIMEOUT,
            };
            (status, Json(json!({"detail":error.to_string(),"hint":"Check the profile ID, action parameters, and loopback fixture"}))).into_response()
        }
        Err(error) => profile_errors::storage(error),
    }
}

async fn page(
    State(state): State<Arc<Profiles>>,
    Path((id, action)): Path<(String, String)>,
    Json(fields): Json<Map<String, Value>>,
) -> Response {
    match super::admission::blocking(move || {
        let action = PageAction::parse(&action, &fields)?;
        automation(&state).run(&id, &action, crate::native_entry::now()?)
    })
    .await
    {
        Ok(outcome) => result(outcome),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}
