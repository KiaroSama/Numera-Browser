use super::{
    cookie_routes::{now, prepare},
    profile_errors,
    profile_routes::{Profiles, decorate},
    request_body::OptionalObjectBody,
};
use crate::{
    browser::{warmup, warmup_policy},
    conversion::{truth, value_text},
    ownership::registry::{BackgroundOptions, Kind},
    storage::repo::RepoError,
};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::{Map, Value};
use std::sync::{Arc, atomic::Ordering};

fn start(state: &Profiles, id: &str, body: Map<String, Value>) -> Result<Response, RepoError> {
    let minutes = body.get("minutes").cloned().unwrap_or(5.into());
    let minutes = match warmup_policy::validate(&minutes, body.get("preset"), None) {
        Ok(minutes) => minutes,
        Err(error) => {
            return Ok(profile_errors::detail(
                StatusCode::BAD_REQUEST,
                error.to_string(),
            ));
        }
    };
    if let Some(response) = prepare(state, id)? {
        return Ok(response);
    }
    let record = state
        .dashboard
        .get(id)?
        .ok_or_else(|| RepoError::Message("Prepared profile disappeared".into()))?;
    let preset = body
        .get("preset")
        .filter(|value| truth(value))
        .and_then(Value::as_str)
        .unwrap_or("general")
        .to_lowercase();
    let result = start_owned(
        state,
        id,
        &value_text(&record["name"]),
        minutes,
        preset,
        None,
    )?;
    match result {
        Ok(true) => Ok(Json(decorate(
            state,
            state.dashboard.get(id)?.unwrap_or(record),
            now()?,
        )?)
        .into_response()),
        Ok(false) => Ok(profile_errors::detail(
            StatusCode::CONFLICT,
            "stop the profile first - its session is in use",
        )),
        Err(error) => Ok(profile_errors::detail(
            match error.kind.as_str() {
                "busy" => StatusCode::CONFLICT,
                "locked" => StatusCode::LOCKED,
                "gone" => StatusCode::NOT_FOUND,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            },
            error.message,
        )),
    }
}

pub(super) fn start_owned(
    state: &Profiles,
    id: &str,
    name: &str,
    minutes: f64,
    preset: String,
    schedule: Option<String>,
) -> Result<Result<bool, crate::ownership::ready::StartupFailed>, RepoError> {
    let urls = warmup_policy::sites(Some(&preset));
    let browser = state
        .browser
        .as_ref()
        .ok_or_else(|| RepoError::Message("Native browser runtime is not configured".into()))?;
    let kind = if schedule.is_some() {
        Kind::ScheduledWarmup
    } else {
        Kind::ManualWarmup
    };
    let started = std::time::Instant::now();
    let bookkeeping = schedule.clone();
    Ok(state.sessions.start_background_checked(
        BackgroundOptions {
            id,
            requested_name: name,
            kind,
            schedule,
        },
        |session, mut current| {
            current.startup_urls.clear();
            browser.open(
                session,
                current,
                true,
                now().map_err(|error| crate::ownership::ready::StartupFailed {
                    kind: "failed".into(),
                    message: error.to_string(),
                })?,
            )
        },
        move |adapter, cancelled| {
            let mut control = warmup::Live::new(move || cancelled.load(Ordering::Acquire))?;
            let startup = started.elapsed().as_secs_f64();
            warmup::run(
                adapter,
                minutes,
                urls,
                &preset,
                -startup,
                &mut control,
                |_| Ok(()),
            )
            .map(|_| ())
        },
        || {
            if let Some(id) = bookkeeping {
                let schedules = crate::storage::schedules::ScheduleStore::new(state.root.clone());
                schedules
                    .record(
                        &id,
                        Map::from_iter([
                            (
                                "lastRun".into(),
                                now().map_err(std::io::Error::other)?.into(),
                            ),
                            ("lastOutcome".into(), "started".into()),
                            ("lastError".into(), Value::Null),
                        ]),
                    )
                    .map_err(std::io::Error::other)?;
            }
            Ok(())
        },
    ))
}
pub(super) async fn warmup_profile(
    State(state): State<Arc<Profiles>>,
    Path(id): Path<String>,
    OptionalObjectBody(body): OptionalObjectBody,
) -> Response {
    match tokio::task::spawn_blocking(move || start(&state, &id, body)).await {
        Ok(Ok(response)) => response,
        Ok(Err(error)) => profile_errors::storage(error),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response(),
    }
}
