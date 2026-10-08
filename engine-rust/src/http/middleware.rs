use super::request_auth::RequestAuth;
use axum::{
    Json, Router,
    extract::{Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
};
use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

pub fn protect(router: Router, auth: Arc<RequestAuth>) -> Router {
    router.layer(middleware::from_fn_with_state(auth, require_token))
}

async fn require_token(
    State(auth): State<Arc<RequestAuth>>,
    request: Request,
    next: Next,
) -> Response {
    let method = request.method().as_str().to_owned();
    let path = request.uri().path().to_owned();
    let authorization = request
        .headers()
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let alternate = request
        .headers()
        .get("x-numera-token")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let now = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs_f64(),
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"detail":"Request clock is unavailable"})),
            )
                .into_response();
        }
    };
    // Token policy/read-modify-write takes a finite file lock. Do not run that
    // blocking work on an event-loop worker; a canceled future cannot abort IO.
    let decision = tokio::task::spawn_blocking(move || {
        auth.authorize(
            &method,
            &path,
            authorization.as_deref(),
            alternate.as_deref(),
            now,
        )
    })
    .await;
    match decision {
        Ok(Ok(true)) => next.run(request).await,
        Ok(Ok(false)) => {
            (StatusCode::UNAUTHORIZED, Json(RequestAuth::denial_body())).into_response()
        }
        _ => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"detail":"Authentication state could not be verified"})),
        )
            .into_response(),
    }
}
