use super::{cookie_routes, profile_errors, profile_routes::Profiles};
use crate::{cli::probe_commands::Probes, storage::repo::RepoError};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use std::sync::Arc;

fn run(state: &Profiles, id: &str, operation: &str) -> Result<Response, RepoError> {
    if let Some(refusal) = cookie_routes::prepare(state, id)? {
        return Ok(refusal);
    }
    let runtime = state
        .browser
        .as_ref()
        .ok_or_else(|| RepoError::Message("Native browser runtime is not configured".into()))?;
    let time = cookie_routes::now()?;
    let result = if operation == "align" {
        crate::cli::align_commands::Align {
            repo: &state.engine,
            sessions: &state.sessions,
            runtime,
        }
        .run(id, None, time)
    } else {
        Probes {
            repo: &state.engine,
            sessions: &state.sessions,
            runtime,
        }
        .run(id, None, operation, time)
    };
    Ok(match result {
        Ok(mut outcome) => {
            if operation == "align" {
                let saved = state.dashboard.patch(id, |mut current| {
                    let object = current.as_object_mut().ok_or_else(|| RepoError::Message("Dashboard profile is not an object".into()))?;
                    let fp = &outcome.payload["fingerprint"];
                    if let Some(os) = fp["os"].as_str() {
                        let name = match os {"windows"=>Some("Windows"),"macos"=>Some("macOS"),"linux"=>Some("Linux"),"android"=>Some("Android"),"ios"=>Some("iOS"),_=>None};
                        if let Some(name) = name { object.insert("os".into(),name.into()); }
                    }
                    for (source,target) in [("userAgent","userAgent"),("browserVersion","browserVersion"),
                        ("cores","cores"),("memory","memory"),("screen","screen"),("webglVendor","webglVendor"),
                        ("webglRenderer","webglRenderer"),("cameras","mediaVideo"),("microphones","mediaAudio"),
                        ("locale","locale"),("timezone","timezone")] {
                        if let Some(value) = fp.get(source).filter(|value| !value.is_null()) { object.insert(target.into(),value.clone()); }
                    }
                    if let Some(grade) = outcome.payload["after"]["grade"].as_str().filter(|grade| !grade.is_empty()) {
                        object.insert("_trust".into(),serde_json::json!({"grade":grade,"score":outcome.payload["after"]["score"],"at":cookie_routes::now()?}));
                    }
                    Ok((current,Default::default()))
                },None,Some("preserve"),cookie_routes::now()?)?;
                outcome.payload["profile"] = saved
                    .map(|record| {
                        super::profile_routes::decorate(state, record, cookie_routes::now()?)
                    })
                    .transpose()?
                    .unwrap_or(serde_json::Value::Null);
            }
            if operation == "trust" {
                let stamp = serde_json::json!({"grade":outcome.payload["grade"],"score":outcome.payload["score"],"at":cookie_routes::now()?});
                state.dashboard.patch(
                    id,
                    |mut current| {
                        current
                            .as_object_mut()
                            .ok_or_else(|| {
                                RepoError::Message("Dashboard profile is not an object".into())
                            })?
                            .insert("_trust".into(), stamp);
                        Ok((current, Default::default()))
                    },
                    None,
                    Some("preserve"),
                    cookie_routes::now()?,
                )?;
            }
            Json(outcome.payload).into_response()
        }
        Err(error) => profile_errors::detail(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    })
}
async fn execute(state: Arc<Profiles>, id: String, operation: &'static str) -> Response {
    match tokio::task::spawn_blocking(move || run(&state, &id, operation)).await {
        Ok(Ok(response)) => response,
        Ok(Err(error)) => profile_errors::storage(error),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response(),
    }
}
pub(super) async fn align(State(state): State<Arc<Profiles>>, Path(id): Path<String>) -> Response {
    execute(state, id, "align").await
}
pub(super) async fn trust(State(state): State<Arc<Profiles>>, Path(id): Path<String>) -> Response {
    execute(state, id, "trust").await
}
pub(super) async fn proxy(State(state): State<Arc<Profiles>>, Path(id): Path<String>) -> Response {
    execute(state, id, "proxy").await
}
pub(super) async fn tls(State(state): State<Arc<Profiles>>, Path(id): Path<String>) -> Response {
    execute(state, id, "tls").await
}
pub(super) async fn dns(State(state): State<Arc<Profiles>>, Path(id): Path<String>) -> Response {
    execute(state, id, "dns").await
}
