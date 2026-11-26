use super::{
    automation_routes::{automation, result},
    profile_routes::Profiles,
};
use crate::browser::workflows::{self, LocalWorkflow, Workflows};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

pub(super) fn routes(state: Arc<Profiles>) -> Router {
    Router::new()
        .route("/api/workflows", get(list).post(save))
        .route("/api/workflows/{id}", delete(remove))
        .route("/api/workflows/{id}/run", post(run))
        .route("/api/fleet/sync", post(fleet))
        .route(
            "/api/profiles/{id}/ip-check",
            get(ip_check).head(super::application::method_not_allowed),
        )
        .layer(DefaultBodyLimit::max(128 * 1024))
        .with_state(state)
}

async fn list(State(state): State<Arc<Profiles>>) -> Response {
    blocking(move || Ok(json!({"workflows":Workflows(&state.root).list()?}))).await
}

async fn save(State(state): State<Arc<Profiles>>, Json(workflow): Json<LocalWorkflow>) -> Response {
    blocking(move || {
        Workflows(&state.root).save(&workflow)?;
        Ok(json!({"workflow":workflow}))
    })
    .await
}

async fn remove(State(state): State<Arc<Profiles>>, Path(id): Path<String>) -> Response {
    blocking(move || {
        Workflows(&state.root).delete(&id)?;
        Ok(json!({"deleted":true,"id":id}))
    })
    .await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Run {
    profile_ids: Vec<String>,
}

async fn run(
    State(state): State<Arc<Profiles>>,
    Path(id): Path<String>,
    Json(input): Json<Run>,
) -> Response {
    blocking(move || {
        let workflow = Workflows(&state.root).load(&id)?;
        let report = workflows::run(
            &automation(&state),
            &workflow,
            &input.profile_ids,
            crate::native_entry::now()?,
        )?;
        serde_json::to_value(report)
            .map_err(std::io::Error::other)
            .map_err(Into::into)
    })
    .await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Fleet {
    profile_ids: Vec<String>,
    action: String,
    url: Option<String>,
}

async fn fleet(State(state): State<Arc<Profiles>>, Json(input): Json<Fleet>) -> Response {
    blocking(move || {
        workflows::fleet(
            &state.engine,
            &state.sessions,
            state.browser.as_ref(),
            &input.profile_ids,
            &input.action,
            input.url.as_deref(),
            crate::native_entry::now()?,
        )
    })
    .await
}

async fn ip_check(State(state): State<Arc<Profiles>>, Path(id): Path<String>) -> Response {
    blocking(move || workflows::profile_ip_check(&state.engine, &id, crate::native_entry::now()?))
        .await
}

async fn blocking(
    work: impl FnOnce() -> Result<Value, crate::storage::repo::RepoError> + Send + 'static,
) -> Response {
    match super::admission::blocking(work).await {
        Ok(outcome) => result(outcome),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}
