use crate::storage::repo::RepoError;
use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

pub(crate) fn storage(error: RepoError) -> Response {
    let message = error.to_string();
    match error {
        RepoError::NameTaken(_) | RepoError::Diverged(_) | RepoError::SessionBusy(_) => {
            detail(StatusCode::CONFLICT, message)
        }
        RepoError::StaleProfile(_) => (
            StatusCode::CONFLICT,
            Json(json!({"detail":message,"kind":"StaleProfile"})),
        )
            .into_response(),
        RepoError::UnconvertibleProfile(_) => detail(StatusCode::UNPROCESSABLE_ENTITY, message),
        RepoError::VaultLocked(_) => detail(StatusCode::LOCKED, message),
        RepoError::Io(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
            detail(StatusCode::CONFLICT, message)
        }
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response(),
    }
}
pub(crate) fn detail(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(json!({"detail":message.into()}))).into_response()
}
