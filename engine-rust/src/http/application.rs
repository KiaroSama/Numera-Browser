//! Composition of implemented routes, not a complete executable HTTP server.
use super::{
    config_routes, cors, diagnostics, extension_routes, fingerprint_routes, folder_routes,
    middleware::protect, profile_routes, proxy_routes, request_auth::RequestAuth, schedule_routes,
    token_routes, tokens::TokenStore,
};
use axum::{
    Json, Router,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;
use std::{
    collections::BTreeMap,
    io,
    path::PathBuf,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

pub async fn router(
    root: PathBuf,
    environment: &str,
    origins: &str,
    password: Option<String>,
    host: &str,
    installed: BTreeMap<String, bool>,
) -> io::Result<Router> {
    let sessions = Arc::new(crate::ownership::registry::Registry::new(
        root.clone(),
        host.into(),
        password.clone(),
    )?);
    router_with_sessions(
        root,
        environment,
        origins,
        password,
        host,
        SessionApplication {
            installed,
            sessions,
        },
    )
    .await
}

pub struct SessionApplication {
    pub installed: BTreeMap<String, bool>,
    pub sessions: Arc<crate::ownership::registry::Registry>,
}

pub async fn router_with_sessions(
    root: PathBuf,
    environment: &str,
    origins: &str,
    password: Option<String>,
    host: &str,
    application: SessionApplication,
) -> io::Result<Router> {
    router_with_browser(
        root,
        environment,
        origins,
        password,
        host,
        application,
        None,
    )
    .await
}

pub async fn router_with_browser(
    root: PathBuf,
    environment: &str,
    origins: &str,
    password: Option<String>,
    host: &str,
    application: SessionApplication,
    browser: Option<crate::browser::runtime::Runtime>,
) -> io::Result<Router> {
    router_with_dashboard(
        root,
        environment,
        origins,
        password,
        host,
        application,
        (browser, None),
    )
    .await
}

pub async fn router_with_dashboard(
    root: PathBuf,
    environment: &str,
    origins: &str,
    password: Option<String>,
    host: &str,
    application: SessionApplication,
    resources: (
        Option<crate::browser::runtime::Runtime>,
        Option<&std::path::Path>,
    ),
) -> io::Result<Router> {
    let (browser, dashboard) = resources;
    let SessionApplication {
        installed,
        sessions,
    } = application;
    if host.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Session host identity is missing",
        ));
    }
    if sessions.root() != root || sessions.host() != host {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Session registry does not belong to this application root and host",
        ));
    }
    let auth = Arc::new(RequestAuth::new(TokenStore::new(root.clone()), environment));
    let adopting = auth.clone();
    tokio::task::spawn_blocking(move || {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| io::Error::other("System clock precedes the Unix epoch"))?
            .as_secs_f64();
        adopting.adopt(now)
    })
    .await
    .map_err(|_| io::Error::other("Authentication startup task failed"))??;
    let routes = diagnostics::routes(auth.clone())
        .merge(super::documentation::routes()?)
        .merge(token_routes::routes(root.clone(), environment))
        .merge(folder_routes::routes(root.clone()))
        .merge(fingerprint_routes::routes())
        .merge(profile_routes::routes_with_browser(
            root.clone(),
            password.clone(),
            host,
            installed.clone(),
            sessions,
            browser,
        ))
        .merge(extension_routes::routes(root.clone()))
        .merge(proxy_routes::routes(root.clone(), password.clone()))
        .merge(config_routes::routes(
            root.clone(),
            password.clone(),
            installed,
        ))
        .merge(schedule_routes::routes(root, password))
        .method_not_allowed_fallback(method_not_allowed);
    let routes = super::dashboard::mount(routes, dashboard)?;
    Ok(protect(cors::apply(routes, origins), auth))
}
pub(crate) async fn method_not_allowed() -> Response {
    (
        StatusCode::METHOD_NOT_ALLOWED,
        Json(json!({"detail":"Method Not Allowed"})),
    )
        .into_response()
}
