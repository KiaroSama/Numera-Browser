//! Carry request ownership into blocking work that survives HTTP cancellation.
use crate::ownership::update_admission::Reservation;
use axum::{
    Json, Router,
    extract::{Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
};
use std::{future::Future, path::PathBuf, sync::Arc};

tokio::task_local! {
    static REQUEST_OWNER: Arc<Reservation>;
}

pub(super) fn protect(router: Router, root: PathBuf) -> Router {
    router.layer(middleware::from_fn_with_state(root, admit))
}

async fn admit(State(root): State<PathBuf>, request: Request, next: Next) -> Response {
    let path = request.uri().path();
    let stop_profile = request.method() == "POST"
        && path
            .strip_prefix("/api/profiles/")
            .and_then(|path| path.strip_suffix("/stop"))
            .is_some_and(|id| !id.is_empty() && !id.contains('/'));
    if request.method() == "OPTIONS"
        || !path.starts_with("/api/")
        || path.trim_end_matches('/') == "/api/ready"
        || stop_profile
    {
        return next.run(request).await;
    }
    let reservation =
        tokio::task::spawn_blocking(move || Reservation::begin(&root, "native-http")).await;
    match reservation {
        Ok(Ok(owner)) => REQUEST_OWNER.scope(Arc::new(owner), next.run(request)).await,
        Ok(Err(error)) if error.kind() == std::io::ErrorKind::WouldBlock => (
            StatusCode::CONFLICT,
            Json(serde_json::json!({"detail":"An update has closed admission; finish update recovery before retrying"})),
        )
            .into_response(),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response(),
    }
}

pub(super) fn blocking<F, R>(operation: F) -> tokio::task::JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    let owner = REQUEST_OWNER.try_with(Arc::clone).ok();
    tokio::task::spawn_blocking(move || match owner {
        Some(owner) => REQUEST_OWNER.sync_scope(owner, operation),
        None => operation(),
    })
}

pub(super) fn owned<F: Future>(operation: F) -> impl Future<Output = F::Output> {
    let owner = REQUEST_OWNER.try_with(Arc::clone).ok();
    async move {
        match owner {
            Some(owner) => REQUEST_OWNER.scope(owner, operation).await,
            None => operation.await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common;
    use std::{io, sync::mpsc, time::Duration};

    #[test]
    fn closed_native_router_preserves_readiness_and_refuses_mutation_before_handler()
    -> Result<(), Box<dyn std::error::Error>> {
        use axum::{body::Body, http::Request, routing::post};
        use tower::ServiceExt;
        let temporary = common::TempDirectory::new("native-http-closed")?;
        let root = temporary.path().to_owned();
        std::fs::create_dir(root.join(".updates"))?;
        std::fs::write(root.join(".updates/admission.json"), b"malformed")?;
        let marker = root.join("must-not-write");
        let target = marker.clone();
        let app = protect(
            Router::new()
                .route("/api/ready", axum::routing::get(|| async { "ready" }))
                .route(
                    "/api/mutate",
                    post(move || {
                        let target = target.clone();
                        async move {
                            std::fs::write(target, b"changed").expect("Fixture write failed");
                            "changed"
                        }
                    }),
                ),
            root.clone(),
        );
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        runtime.block_on(async {
            assert_eq!(
                app.clone()
                    .oneshot(Request::post("/api/mutate").body(Body::empty())?)
                    .await?
                    .status(),
                StatusCode::CONFLICT,
            );
            assert_eq!(
                app.oneshot(Request::get("/api/ready").body(Body::empty())?)
                    .await?
                    .status(),
                StatusCode::OK,
            );
            Ok::<_, Box<dyn std::error::Error>>(())
        })?;
        assert!(!marker.exists());
        temporary.cleanup()?;
        Ok(())
    }

    #[test]
    fn cleanup_authentication_verifies_named_token_without_usage_mutation()
    -> Result<(), Box<dyn std::error::Error>> {
        let temporary = common::TempDirectory::new("native-stop-auth")?;
        let root = temporary.path();
        let tokens = super::super::tokens::TokenStore::new(root.into());
        let created = tokens.create("fixture", 1000.0)?;
        let token = created["token"].as_str().ok_or("Token fixture missing")?;
        let before = std::fs::read(root.join("tokens.json"))?;
        let auth = super::super::request_auth::RequestAuth::new(tokens, "");
        assert!(auth.authorize(
            "POST",
            "/api/profiles/profile/stop",
            Some(&format!("Bearer {token}")),
            None,
            2000.0
        )?);
        assert_eq!(std::fs::read(root.join("tokens.json"))?, before);
        temporary.cleanup()?;
        Ok(())
    }

    #[test]
    fn canceled_request_keeps_admission_until_actual_blocking_work_settles()
    -> Result<(), Box<dyn std::error::Error>> {
        let temporary = common::TempDirectory::new("native-http-admission")?;
        let root = temporary.path();
        let owner = Arc::new(Reservation::begin(root, "fixture-http")?);
        let (entered, started) = mpsc::sync_channel(1);
        let (finish, release) = mpsc::sync_channel(1);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let task = {
            let _runtime = runtime.enter();
            REQUEST_OWNER.sync_scope(owner.clone(), || {
                blocking(move || -> io::Result<()> {
                    entered.send(()).map_err(io::Error::other)?;
                    release
                        .recv_timeout(Duration::from_secs(5))
                        .map_err(io::Error::other)?;
                    Ok(())
                })
            })
        };
        started.recv_timeout(Duration::from_secs(5))?;
        drop(owner);
        task.abort(); // A running spawn_blocking closure cannot be canceled.
        let retained = std::fs::read_dir(root.join(".updates/work"))?.count();
        finish.send(())?;
        runtime.block_on(task)??;
        assert_eq!(retained, 1);
        assert_eq!(std::fs::read_dir(root.join(".updates/work"))?.count(), 0);
        temporary.cleanup()?;
        Ok(())
    }
}
