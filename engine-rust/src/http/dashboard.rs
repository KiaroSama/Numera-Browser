use axum::{
    Json, Router,
    body::Body,
    extract::{Request, State},
    http::{Method, StatusCode},
    response::{IntoResponse, Response},
};
use std::{
    io,
    path::{Path, PathBuf},
    sync::Arc,
};
use tower::ServiceExt;
use tower_http::services::ServeFile;

/// Resolve an optional build from its supplied application/package location,
/// never by probing the caller's working directory or starting another server.
pub fn discover(app: &Path) -> Option<PathBuf> {
    let packaged = app.join("web");
    if packaged.join("index.html").is_file() {
        return Some(packaged);
    }
    app.ancestors()
        .map(|parent| parent.join("dashboard/dist"))
        .find(|path| path.join("index.html").is_file())
}
pub fn mount(router: Router, root: Option<&Path>) -> io::Result<Router> {
    let Some(root) = root else {
        return Ok(router);
    };
    let root = std::fs::canonicalize(root)?;
    let index = std::fs::canonicalize(root.join("index.html"))?;
    let assets = std::fs::canonicalize(root.join("assets"))?;
    if !index.starts_with(&root)
        || !assets.starts_with(&root)
        || !index.is_file()
        || !assets.is_dir()
    {
        return Err(io::Error::other(
            "Dashboard build requires index.html and assets",
        ));
    }
    Ok(router.fallback_service(Router::new().fallback(serve).with_state(Arc::new(root))))
}
fn missing(api: bool) -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({"detail":if api {"no such API endpoint"} else {"Not Found"}})),
    )
        .into_response()
}
async fn serve(State(root): State<Arc<PathBuf>>, request: Request) -> Response {
    let path = match percent_encoding::percent_decode_str(request.uri().path()).decode_utf8() {
        Ok(path) => path,
        Err(_) => return missing(false),
    };
    let relative = path.trim_start_matches('/').to_owned();
    let asset = relative.starts_with("assets/");
    if request.method() != Method::GET && !(asset && request.method() == Method::HEAD) {
        return super::application::method_not_allowed().await;
    }
    if relative.starts_with("api/") {
        return missing(true);
    }
    let checked = tokio::task::spawn_blocking(move || -> io::Result<Option<PathBuf>> {
        if relative.contains('\0') {
            return Ok(None);
        }
        let candidate = if relative.is_empty() {
            None
        } else {
            crate::storage::archive_paths::member_path(&root, &relative).ok()
        };
        // Protected paths are refused rather than accidentally becoming public
        // files. Normal missing frontend routes still select the SPA entry.
        if !relative.is_empty() && candidate.is_none() {
            return Ok(None);
        }
        if let Some(candidate) = candidate {
            match std::fs::canonicalize(&candidate) {
                Ok(physical) if physical.starts_with(root.as_path()) && physical.is_file() => {
                    return Ok(Some(physical));
                }
                Ok(physical) if !physical.starts_with(root.as_path()) => return Ok(None),
                Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
                _ => {}
            }
        }
        if asset {
            Ok(None)
        } else {
            let index = std::fs::canonicalize(root.join("index.html"))?;
            Ok((index.starts_with(root.as_path()) && index.is_file()).then_some(index))
        }
    })
    .await;
    let path = match checked {
        Ok(Ok(Some(path))) => path,
        Ok(Ok(None)) => return missing(false),
        _ => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"detail":"Dashboard file could not be read"})),
            )
                .into_response();
        }
    };
    match ServeFile::new(path).oneshot(request).await {
        Ok(mut response) => {
            if let Some(content_type) = response
                .headers()
                .get(axum::http::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .filter(|value| {
                    value.starts_with("text/") && !value.to_ascii_lowercase().contains("charset=")
                })
            {
                let content_type =
                    axum::http::HeaderValue::from_str(&format!("{content_type}; charset=utf-8"));
                if let Ok(content_type) = content_type {
                    response
                        .headers_mut()
                        .insert(axum::http::header::CONTENT_TYPE, content_type);
                }
            }
            response.map(Body::new)
        }
        Err(error) => match error {},
    }
}
#[cfg(test)]
#[path = "dashboard_tests.rs"]
mod tests;
