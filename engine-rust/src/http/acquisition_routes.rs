//! Local acquisition offers and separately approved, finite owned CLI helpers.
use super::{request_auth::RequestAuth, request_body::ObjectBody, tokens::TokenStore};
use crate::{
    cloak_acquisition::{Acquisition, unavailable_offer},
    ownership::{capture, capture_units::Units, update_admission::Reservation},
};
use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
};
use serde_json::{Value, json};
use std::{
    io,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

/// The same local-metadata/owned-helper boundary used by the retained source
/// tests. Injected implementations replace vendor IO, never router validation.
pub trait Service: Send + Sync {
    fn offer(&self) -> io::Result<Value>;
    fn acquire(&self, root: &Path, argv: &[String]) -> io::Result<Value>;
}
pub struct NativeService {
    root: PathBuf,
}
impl Service for NativeService {
    fn offer(&self) -> io::Result<Value> {
        Ok(Acquisition::native_at(&self.root).offer())
    }
    fn acquire(&self, root: &Path, argv: &[String]) -> io::Result<Value> {
        run_helper(root, argv)
    }
}
struct StateData {
    root: PathBuf,
    service: Arc<dyn Service>,
    auth: Arc<RequestAuth>,
}

pub fn router_with_service(root: PathBuf, environment: &str, service: Arc<dyn Service>) -> Router {
    let auth = Arc::new(RequestAuth::new(TokenStore::new(root.clone()), environment));
    super::middleware::protect(
        super::cors::apply(routes_with_service(root, environment, service), ""),
        auth,
    )
}
pub(crate) fn routes(root: PathBuf, environment: &str) -> Router {
    let service = Arc::new(NativeService { root: root.clone() });
    routes_with_service(root, environment, service)
}
fn routes_with_service(root: PathBuf, environment: &str, service: Arc<dyn Service>) -> Router {
    let auth = Arc::new(RequestAuth::new(TokenStore::new(root.clone()), environment));
    Router::new()
        .route(
            "/api/engines/cloak/acquisition",
            get(offer)
                .head(super::application::method_not_allowed)
                .post(acquire),
        )
        .with_state(Arc::new(StateData {
            root,
            service,
            auth,
        }))
}
fn local_offer(state: &StateData) -> Value {
    state
        .service
        .offer()
        .unwrap_or_else(|_| unavailable_offer())
}
async fn offer(State(state): State<Arc<StateData>>) -> Response {
    match super::admission::blocking(move || local_offer(&state)).await {
        Ok(offer) => Json(offer).into_response(),
        Err(_) => Json(unavailable_offer()).into_response(),
    }
}
fn detail(code: StatusCode, message: &str) -> Response {
    (code, Json(json!({"detail":message}))).into_response()
}
async fn acquire(
    State(state): State<Arc<StateData>>,
    headers: HeaderMap,
    ObjectBody(body): ObjectBody,
) -> Response {
    if body.len() != 2
        || !body.contains_key("offer_id")
        || body.get("approve") != Some(&Value::Bool(true))
    {
        return detail(
            StatusCode::BAD_REQUEST,
            "approve the exact displayed browser acquisition offer",
        );
    }
    match super::admission::blocking(move || {
        let current = local_offer(&state);
        if current["available"] != true {
            return detail(
                StatusCode::CONFLICT,
                current["reason"]
                    .as_str()
                    .unwrap_or("Cloak acquisition is unavailable"),
            );
        }
        let Some(offer_id) = body["offer_id"]
            .as_str()
            .filter(|id| Some(*id) == current["offer_id"].as_str())
        else {
            return detail(
                StatusCode::CONFLICT,
                "browser acquisition offer changed; review it before approving again",
            );
        };
        // Re-read current token policy after metadata work and immediately
        // before process admission. A revoked token cannot start acquisition.
        let authorized = state.auth.authorize(
            "POST",
            "/api/engines/cloak/acquisition",
            headers
                .get("authorization")
                .and_then(|value| value.to_str().ok()),
            headers
                .get("x-numera-token")
                .and_then(|value| value.to_str().ok()),
            crate::native_entry::now().unwrap_or(0.0),
        );
        match authorized {
            Ok(true) => {}
            Ok(false) => {
                return (StatusCode::UNAUTHORIZED, Json(RequestAuth::denial_body()))
                    .into_response();
            }
            Err(_) => {
                return detail(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Authentication state could not be verified",
                );
            }
        }
        let _admission = match Reservation::begin(&state.root, "native-cloak-http-helper") {
            Ok(owner) => owner,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                return detail(
                    StatusCode::CONFLICT,
                    "An update has closed admission; finish update recovery before retrying",
                );
            }
            Err(_) => {
                return detail(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Cloak acquisition ownership could not be established",
                );
            }
        };
        let argv = vec![
            "engines".into(),
            "--acquire-cloak".into(),
            offer_id.into(),
            "--approve".into(),
            "--json".into(),
        ];
        match state.service.acquire(&state.root, &argv) {
            Ok(result) => Json(result).into_response(),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => detail(
                StatusCode::CONFLICT,
                "a Cloak acquisition is already in progress for this vendor cache",
            ),
            Err(_) => detail(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Cloak browser acquisition failed; review the current offer and vendor license",
            ),
        }
    })
    .await
    {
        Ok(result) => result,
        Err(_) => detail(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Cloak browser acquisition failed",
        ),
    }
}

fn run_helper(root: &Path, argv: &[String]) -> io::Result<Value> {
    let executable = std::env::current_exe()?;
    let mut admission = Reservation::begin(root, "native-cloak-cli-helper")?;
    let mut command =
        crate::mcp_command::child_command(&executable, Some(root), None, argv.to_vec());
    command
        .env_remove("NUMERA_PASSWORD")
        .env_remove("NUMERA_API_TOKEN");
    let mut units = Units::new(root, &mut command)?;
    let captured = capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(720),
            idle: Duration::from_secs(720),
            bytes: 65536,
            cleanup: Duration::from_secs(10),
        },
    );
    if units.settle(Duration::from_secs(10)).is_err() {
        admission.retain();
        return Err(io::Error::other("Cloak helper cleanup remains owned"));
    }
    let captured = match captured {
        Ok(value) => value,
        Err(mut failure) => {
            if failure.settle(Duration::from_secs(10)).is_err() {
                admission.retain();
            }
            return Err(io::Error::new(
                failure.kind(),
                "Cloak acquisition helper failed",
            ));
        }
    };
    if captured.exit != 0 {
        return Err(io::Error::other(
            "Cloak acquisition helper refused its exact offer",
        ));
    }
    let result: Value = serde_json::from_slice(&captured.stdout)
        .map_err(|_| io::Error::other("Cloak helper returned invalid structured output"))?;
    if result["engine"] != "cloak"
        || result["installed"] != true
        || result["offer_id"].as_str() != argv.get(2).map(String::as_str)
        || result["version"]
            .as_str()
            .is_none_or(|value| crate::cloak_acquisition::version(value).is_err())
    {
        return Err(io::Error::other(
            "Cloak helper did not confirm installation",
        ));
    }
    Ok(result)
}
