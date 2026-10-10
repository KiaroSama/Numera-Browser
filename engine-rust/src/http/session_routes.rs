use super::{
    profile_errors,
    profile_routes::{Profiles, decorate},
};
use crate::{conversion::truth, storage::repo::RepoError};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const NOT_OURS: &str = "another process has this profile's browser open - this server did not start it and cannot stop it. Close it where it was started.";

fn stop(state: &Profiles, id: &str) -> Result<Response, RepoError> {
    let Some(record) = state.dashboard.get(id)?.filter(truth) else {
        return Ok(profile_errors::detail(
            StatusCode::NOT_FOUND,
            "profile not found",
        ));
    };
    let stopped = match state.sessions.stop(id, Duration::from_secs(10)) {
        Ok(stopped) => stopped,
        Err(error) => {
            return Ok(profile_errors::detail(
                StatusCode::INTERNAL_SERVER_ERROR,
                error.to_string(),
            ));
        }
    };
    if !stopped && state.sessions.running(id)? {
        return Ok(profile_errors::detail(StatusCode::CONFLICT, NOT_OURS));
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(std::io::Error::other)?
        .as_secs_f64();
    Ok(Json(decorate(state, record, now)?).into_response())
}

pub(super) async fn stop_profile(
    State(state): State<Arc<Profiles>>,
    Path(id): Path<String>,
) -> Response {
    match tokio::task::spawn_blocking(move || stop(&state, &id)).await {
        Ok(Ok(response)) => response,
        Ok(Err(error)) => profile_errors::storage(error),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response(),
    }
}
