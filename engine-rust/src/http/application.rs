//! Native HTTP application composition, startup admission and shared policies.
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
    router_with_recovery(
        root,
        environment,
        origins,
        password,
        host,
        application,
        resources,
        RecoveryOptions {
            ticket: std::env::var("NUMERA_UPDATE_TICKET").ok(),
            update_token: std::env::var("NUMERA_UPDATE_TOKEN").unwrap_or_default(),
        },
    )
    .await
}

pub struct RecoveryOptions {
    pub ticket: Option<String>,
    pub update_token: String,
}

#[allow(clippy::too_many_arguments)]
pub async fn router_with_recovery(
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
    recovery: RecoveryOptions,
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
    let startup = crate::ownership::update_admission::Admission::new(&root, host)?;
    let startup_root = root.clone();
    let startup_password = password.clone();
    super::admission::blocking(move || {
        startup.with_startup(false, recovery.ticket.as_deref(), |recovering| {
            if recovering {
                return Ok(());
            }
            #[cfg(feature = "test-support")]
            crate::vault::testing::event("before-update-store-initialize", &startup_root)?;
            crate::storage::repo::ProfileRepo::open(startup_root.clone(), startup_password, None)
                .map_err(io::Error::other)?;
            std::fs::create_dir_all(startup_root.join("dashboard"))?;
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| io::Error::other("System clock precedes the Unix epoch"))?
                .as_secs_f64();
            adopting.adopt(now)?;
            Ok(())
        })
    })
    .await
    .map_err(|_| io::Error::other("Authentication startup task failed"))??;
    let profiles = Arc::new(profile_routes::Profiles {
        dashboard: crate::storage::dashboard::DashboardStore::new(root.clone(), password.clone()),
        engine: crate::storage::repo::ProfileRepo::new(root.clone(), password.clone()),
        root: root.clone(),
        host: host.into(),
        installed: installed.clone(),
        sessions: sessions.clone(),
        browser: browser.clone(),
    });
    let routes = diagnostics::routes(auth.clone())
        .merge(super::documentation::routes())
        .merge(crate::mcp_http::mcp_routes(
            axum::routing::post(mcp_http),
            profiles.clone(),
        ))
        .merge(super::cookie_routes::routes(profiles.clone()))
        .merge(super::automation_routes::routes(profiles.clone()))
        .merge(super::workflow_routes::routes(profiles))
        .merge(token_routes::routes(root.clone(), environment))
        .merge(folder_routes::routes(root.clone()))
        .merge(fingerprint_routes::routes())
        .merge(profile_routes::routes_with_browser(
            root.clone(),
            password.clone(),
            host,
            installed.clone(),
            sessions.clone(),
            browser,
        ))
        .merge(extension_routes::routes(root.clone()))
        .merge(super::acquisition_routes::routes(root.clone(), environment))
        .merge(proxy_routes::routes(root.clone(), password.clone()))
        .merge(config_routes::routes(
            root.clone(),
            password.clone(),
            installed,
        ))
        .merge(schedule_routes::routes(root.clone(), password))
        .merge(super::update_routes::routes(
            crate::ownership::update_admission::Admission::new(&root, host)?,
            sessions,
        ))
        .method_not_allowed_fallback(method_not_allowed);
    let routes = super::dashboard::mount(routes, dashboard)?;
    Ok(super::update_routes::protect(
        protect(cors::apply(routes, origins), auth),
        recovery.update_token,
    ))
}
async fn mcp_http(
    axum::extract::State(profiles): axum::extract::State<Arc<super::profile_routes::Profiles>>,
    message: Result<axum::Json<serde_json::Value>, axum::extract::rejection::JsonRejection>,
) -> axum::response::Response {
    use axum::{
        Json,
        http::{StatusCode, header},
        response::IntoResponse,
    };
    let Json(message) = match message {
        Ok(message) => message,
        Err(error) if error.status() == StatusCode::BAD_REQUEST => {
            return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"Parse error"}}))).into_response();
        }
        Err(error) => return error.into_response(),
    };
    if !message.is_object() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"jsonrpc":"2.0","id":null,"error":{"code":-32600,"message":"Invalid Request"}}))).into_response();
    }
    let result = super::admission::blocking(move || {
        let application = crate::cli::application::Application {
            root: &profiles.root,
            home: &profiles.root,
            repo: &profiles.engine,
            installed: &profiles.installed,
            host: &profiles.host,
            password: profiles.engine.password.as_deref(),
            environment_token: true,
            sessions: &profiles.sessions,
            browser: profiles.browser.as_ref(),
        };
        let now = crate::native_entry::now()?;
        crate::mcp_http::handle_http(&message, |argv| {
            crate::mcp::execute_native(&application, argv, now)
        })
    })
    .await;
    let mut response = match result {
        Ok(Ok(Some(value))) => match crate::mcp::encode(&value) {
            Ok(bytes) => ([(header::CONTENT_TYPE, "application/json")], bytes).into_response(),
            Err(_) => StatusCode::PAYLOAD_TOO_LARGE.into_response(),
        },
        Ok(Ok(None)) => StatusCode::NO_CONTENT.into_response(),
        _ => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-store"),
    );
    response
}

pub(crate) async fn method_not_allowed() -> Response {
    (
        StatusCode::METHOD_NOT_ALLOWED,
        Json(json!({"detail":"Method Not Allowed"})),
    )
        .into_response()
}
