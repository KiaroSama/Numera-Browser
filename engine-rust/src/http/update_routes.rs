//! Public update discovery and preferences, plus shell-private activation transactions.
use crate::ownership::{
    registry::Registry,
    update_admission::{Admission, GATE},
};
use axum::{
    Extension, Json, Router,
    body::to_bytes,
    extract::{Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    io::{self, Read},
    path::Path,
    sync::Arc,
    time::Duration,
};
use subtle::ConstantTimeEq;

pub(crate) fn private_path(path: &str) -> bool {
    matches!(
        path.trim_end_matches('/'),
        "/api/updates/prepare"
            | "/api/updates/quiesce"
            | "/api/updates/resume"
            | "/api/updates/private-preferences"
    )
}
pub(crate) fn protect(router: Router, token: String) -> Router {
    let desktop = Desktop::new(token);
    router.layer(middleware::from_fn_with_state(desktop, private_auth))
}
async fn private_auth(
    State(desktop): State<Arc<Desktop>>,
    mut request: Request,
    next: Next,
) -> Response {
    if !private_path(request.uri().path()) {
        request.extensions_mut().insert(desktop);
        return next.run(request).await;
    }
    let token = &desktop.token;
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
        .route(
            "/api/updates/preferences",
            get(get_preferences).put(put_preferences),
        )
        .route("/api/updates/private-preferences", get(get_preferences))
        .route("/api/updates/check", get(check))
        .route("/api/updates/apply", post(apply))
        .with_state(Arc::new(Updates {
            admission,
            sessions,
        }))
}
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct UpdatePreferences {
    pub(crate) app_auto_update: bool,
    pub(crate) browser_core_auto_update: bool,
    pub(crate) clean_old_versions_on_update: bool,
}
impl Default for UpdatePreferences {
    fn default() -> Self {
        Self {
            app_auto_update: false,
            browser_core_auto_update: false,
            clean_old_versions_on_update: true,
        }
    }
}
#[derive(Clone, Copy, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
enum Channel {
    App,
    Browsers,
    #[default]
    All,
}
impl Channel {
    fn parse(query: Option<&str>) -> Option<Self> {
        match query {
            Some("channel=app") => Some(Self::App),
            Some("channel=browsers") => Some(Self::Browsers),
            None | Some("" | "channel=all") => Some(Self::All),
            _ => None,
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Apply {
    #[serde(default)]
    channel: Channel,
    clean_old: Option<bool>,
    #[serde(rename = "offerId")]
    offer_id: String,
}
struct Desktop {
    token: String,
    endpoint: Option<(u16, reqwest::Client)>,
}
impl Desktop {
    fn new(token: String) -> Arc<Self> {
        let endpoint = std::env::var("NUMERA_UPDATE_BRIDGE_PORT")
            .ok()
            .and_then(|port| port.parse::<u16>().ok())
            .filter(|port| *port != 0 && !token.is_empty())
            .and_then(|port| {
                reqwest::Client::builder()
                    .no_proxy()
                    .redirect(reqwest::redirect::Policy::none())
                    .timeout(Duration::from_secs(3))
                    .build()
                    .ok()
                    .map(|client| (port, client))
            });
        Arc::new(Self { token, endpoint })
    }

    async fn call(
        &self,
        action: &'static str,
        body: &Value,
    ) -> Result<(StatusCode, Value), StatusCode> {
        let (port, client) = self
            .endpoint
            .as_ref()
            .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
        let mut response = client
            .post(format!("http://127.0.0.1:{port}/api/{action}"))
            .bearer_auth(&self.token)
            .header("content-type", "application/json")
            .body(serde_json::to_vec(body).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?)
            .send()
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
        let status = response.status();
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| StatusCode::BAD_GATEWAY)?
        {
            if chunk.len() > 8192usize.saturating_sub(bytes.len()) {
                return Err(StatusCode::BAD_GATEWAY);
            }
            bytes.extend_from_slice(&chunk);
        }
        let body = serde_json::from_slice(&bytes).map_err(|_| StatusCode::BAD_GATEWAY)?;
        Ok((status, body))
    }
}

fn linked(meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    meta.is_symlink()
}

pub(crate) fn preferences(
    root: &Path,
    replacement: Option<UpdatePreferences>,
) -> io::Result<UpdatePreferences> {
    let path = root.join("update-preferences.json");
    let mut file = match fs::symlink_metadata(&path) {
        Ok(meta) if linked(&meta) || !meta.is_file() => {
            return Err(io::Error::other("Invalid update preferences file"));
        }
        Ok(_) => Some(fs::File::open(&path)?),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    if let Some(value) = replacement {
        crate::storage::atomic::write_bytes(&path, &serde_json::to_vec(&value)?)?;
        return Ok(value);
    }
    let Some(file) = file.take() else {
        return Ok(UpdatePreferences::default());
    };
    let mut bytes = Vec::new();
    file.take(4097).read_to_end(&mut bytes)?;
    if bytes.len() > 4096 {
        return Err(io::Error::other(
            "Update preferences file exceeds its limit",
        ));
    }
    serde_json::from_slice(&bytes).map_err(io::Error::other)
}

async fn preferences_response(
    state: Arc<Updates>,
    replacement: Option<UpdatePreferences>,
) -> Response {
    match super::admission::blocking(move || preferences(state.admission.root(), replacement)).await
    {
        Ok(Ok(value)) => Json(value).into_response(),
        _ => detail(
            StatusCode::CONFLICT,
            "Update preferences could not be accessed safely",
        ),
    }
}
async fn get_preferences(State(state): State<Arc<Updates>>) -> Response {
    preferences_response(state, None).await
}
fn approval(request: &Request) -> Option<String> {
    request
        .headers()
        .get("x-numera-update-approval")?
        .to_str()
        .ok()
        .filter(|value| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .map(str::to_owned)
}
async fn put_preferences(
    State(state): State<Arc<Updates>>,
    Extension(desktop): Extension<Arc<Desktop>>,
    request: Request,
) -> Response {
    let approve = approval(&request);
    let value: UpdatePreferences = match typed_payload(request).await {
        Ok(value) => value,
        Err(error) => return *error,
    };
    if (value.app_auto_update || value.browser_core_auto_update)
        && !matches!(
            desktop.call("approval", &json!({"approval":approve})).await,
            Ok((StatusCode::OK, _))
        )
    {
        return detail(
            StatusCode::FORBIDDEN,
            "Enable automatic updates from the local desktop",
        );
    }
    preferences_response(state, Some(value)).await
}
async fn check(Extension(desktop): Extension<Arc<Desktop>>, request: Request) -> Response {
    let Some(channel) = Channel::parse(request.uri().query()) else {
        return detail(
            StatusCode::BAD_REQUEST,
            "Update channel must be app, browsers or all",
        );
    };
    let end = tokio::time::Instant::now() + Duration::from_secs(95);
    let mut refresh = true;
    loop {
        match desktop.call("check", &json!({"channel":channel,"refresh":refresh})).await {
            Ok((status, value)) if value["state"] != "checking" || tokio::time::Instant::now() >= end => {
                return (status, Json(value)).into_response();
            }
            Ok(_) => {
                refresh = false;
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
            Err(status) => return (status, Json(json!({"channel":channel,
                "app":(channel != Channel::Browsers).then(|| json!({"current":env!("CARGO_PKG_VERSION"),
                    "latest":null,"update_available":false,"verified":false,"notes":null})),
                "browsers":[],"state":"unavailable","message":"Signed update checks require the local desktop."}))).into_response(),
        }
    }
}
async fn apply(
    State(state): State<Arc<Updates>>,
    Extension(desktop): Extension<Arc<Desktop>>,
    request: Request,
) -> Response {
    let approve = approval(&request);
    let request: Apply = match typed_payload(request).await {
        Ok(value) => value,
        Err(error) => return *error,
    };
    if request.offer_id.len() != 64
        || !request
            .offer_id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return detail(
            StatusCode::BAD_REQUEST,
            "A valid reviewed update offer is required",
        );
    }
    let clean_old = if let Some(clean_old) = request.clean_old {
        clean_old
    } else {
        match super::admission::blocking(move || preferences(state.admission.root(), None)).await {
            Ok(Ok(value)) => value.clean_old_versions_on_update,
            _ => {
                return detail(
                    StatusCode::CONFLICT,
                    "Update preferences could not be accessed safely",
                );
            }
        }
    };
    let body = json!({"channel":request.channel,"clean_old":clean_old,"offerId":request.offer_id,"approval":approve});
    match desktop.call("apply", &body).await {
        Ok((status, value)) => (status, Json(value)).into_response(),
        Err(status) => (
            status,
            Json(
                json!({"ok":false,"channel":request.channel,"cleaned_bytes":0,
            "data_preserved":true,"message":"Apply verified updates from the local desktop."}),
            ),
        )
            .into_response(),
    }
}

async fn typed_payload<T: serde::de::DeserializeOwned>(
    request: Request,
) -> Result<T, Box<Response>> {
    let bytes = to_bytes(request.into_body(), 4096)
        .await
        .map_err(|_| Box::new(detail(StatusCode::BAD_REQUEST, "invalid update request")))?;
    serde_json::from_slice(&bytes)
        .map_err(|_| Box::new(detail(StatusCode::BAD_REQUEST, "invalid update request")))
}

async fn payload(request: Request) -> Result<Value, Box<Response>> {
    typed_payload(request).await
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

#[cfg(test)]
#[path = "update_offer_tests.rs"]
mod offer_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferences_are_opt_in_and_persist_under_the_selected_root() -> io::Result<()> {
        let staging = crate::storage::archive_paths::Staging::new(
            &std::env::temp_dir(),
            "numera-update-prefs-",
        )?;
        let root = staging.path()?;
        let defaults = preferences(root, None)?;
        assert!(!defaults.app_auto_update && !defaults.browser_core_auto_update);
        assert!(defaults.clean_old_versions_on_update);
        let chosen = UpdatePreferences {
            app_auto_update: true,
            browser_core_auto_update: false,
            clean_old_versions_on_update: false,
        };
        preferences(root, Some(chosen))?;
        assert_eq!(preferences(root, None)?, chosen);
        fs::write(root.join("update-preferences.json"), b"not JSON")?;
        assert!(preferences(root, None).is_err());
        Ok(())
    }

    #[test]
    fn public_routes_never_replace_the_private_activation_capability() {
        assert!(!private_path("/api/updates/apply"));
        assert!(!private_path("/api/updates/preferences"));
        assert!(private_path("/api/updates/private-preferences"));
        assert!(private_path("/api/updates/quiesce"));
        assert!(matches!(
            Channel::parse(Some("channel=browsers")),
            Some(Channel::Browsers)
        ));
        assert!(Channel::parse(Some("channel=app&channel=browsers")).is_none());
    }
}
