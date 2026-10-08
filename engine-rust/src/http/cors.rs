use axum::{
    Router,
    extract::{Request, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
};
use std::sync::Arc;

const METHODS: &str = "DELETE, GET, HEAD, OPTIONS, PATCH, POST, PUT, QUERY";
const PREFLIGHT_VARY: &str = "Origin, Access-Control-Request-Method, Access-Control-Request-Headers, Access-Control-Request-Private-Network";

// The source's preflight 400/body/header contract differs from CorsLayer's
// response defaults. Keep that compatibility at one HTTP adapter boundary.
pub fn apply(router: Router, configured: &str) -> Router {
    let origins = configured
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    router.layer(middleware::from_fn_with_state(Arc::new(origins), cors))
}
fn append_vary(headers: &mut HeaderMap, value: &'static str) {
    let joined = headers
        .get("vary")
        .and_then(|value| value.to_str().ok())
        .map_or_else(|| value.to_owned(), |old| format!("{old}, {value}"));
    if let Ok(value) = HeaderValue::from_str(&joined) {
        headers.insert("vary", value);
    }
}
async fn cors(State(origins): State<Arc<Vec<String>>>, request: Request, next: Next) -> Response {
    let origin = request.headers().get("origin").cloned();
    let wildcard = origins.iter().any(|origin| origin == "*");
    let allowed = origin.as_ref().is_some_and(|value| {
        wildcard
            || value
                .to_str()
                .is_ok_and(|value| origins.iter().any(|origin| origin == value))
    });
    let preflight = origin.is_some()
        && request.method() == "OPTIONS"
        && request
            .headers()
            .contains_key("access-control-request-method");
    if preflight {
        let mut failures = Vec::new();
        if !allowed {
            failures.push("origin");
        }
        if !request
            .headers()
            .get("access-control-request-method")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| METHODS.split(", ").any(|method| method == value))
        {
            failures.push("method");
        }
        if request
            .headers()
            .contains_key("access-control-request-private-network")
        {
            failures.push("private-network");
        }
        let (status, body) = if failures.is_empty() {
            (StatusCode::OK, "OK".into())
        } else {
            (
                StatusCode::BAD_REQUEST,
                format!("Disallowed CORS {}", failures.join(", ")),
            )
        };
        let mut response = (status, body).into_response();
        let headers = response.headers_mut();
        headers.insert("vary", HeaderValue::from_static(PREFLIGHT_VARY));
        headers.insert(
            "access-control-allow-methods",
            HeaderValue::from_static(METHODS),
        );
        headers.insert("access-control-max-age", HeaderValue::from_static("600"));
        if wildcard {
            headers.insert("access-control-allow-origin", HeaderValue::from_static("*"));
        } else if allowed && let Some(origin) = origin {
            headers.insert("access-control-allow-origin", origin);
        }
        if let Some(requested) = request.headers().get("access-control-request-headers") {
            headers.insert("access-control-allow-headers", requested.clone());
        }
        return response;
    }
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    if origin.is_some() {
        if wildcard {
            headers.insert("access-control-allow-origin", HeaderValue::from_static("*"));
        } else if allowed && let Some(origin) = origin {
            headers.insert("access-control-allow-origin", origin);
        }
    }
    append_vary(headers, "Origin");
    response
}
