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

#[path = "admission_handle.rs"]
mod handle;

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
        || super::update_routes::private_path(path)
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

pub(super) fn blocking<F, R>(operation: F) -> handle::Blocking<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    handle::Blocking::new(blocking_boxed(Box::new(move || {
        Box::new(operation()) as handle::Erased
    })))
}

// All handlers share the same Tokio/task-local machinery. The typed front end
// alone creates the erased result and the corresponding checked typed handle.
#[inline(never)]
fn blocking_boxed(
    operation: Box<dyn FnOnce() -> handle::Erased + Send>,
) -> tokio::task::JoinHandle<handle::Erased> {
    let owner = REQUEST_OWNER.try_with(Arc::clone).ok();
    #[cfg(feature = "test-support")]
    let observer = crate::vault::testing::capture();
    tokio::task::spawn_blocking(move || {
        let operation = || match owner {
            Some(owner) => REQUEST_OWNER.sync_scope(owner, || {
                // Observe the active worker before application transaction
                // locks, so cancellation fixtures do not race their own I/O
                // barrier timeout while quiesce waits for the same lock.
                #[cfg(feature = "test-support")]
                crate::vault::testing::event(
                    "before-owned-http-blocking-work",
                    std::path::Path::new(""),
                )
                .expect("Owned HTTP blocking observer failed");
                operation()
            }),
            None => operation(),
        };
        #[cfg(feature = "test-support")]
        return crate::vault::testing::with_context(observer, operation);
        #[cfg(not(feature = "test-support"))]
        operation()
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

        let owner = Arc::new(Reservation::begin(root, "fixture-http-panic")?);
        #[cfg(feature = "test-support")]
        let observations = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        #[cfg(feature = "test-support")]
        let observed = observations.clone();
        #[cfg(feature = "test-support")]
        let callback_root = root.to_path_buf();
        let spawn = || {
            let _runtime = runtime.enter();
            REQUEST_OWNER.sync_scope(owner.clone(), || {
                blocking(move || -> io::Result<()> {
                    assert!(REQUEST_OWNER.try_with(|_| ()).is_ok());
                    #[cfg(feature = "test-support")]
                    crate::vault::testing::event("boxed-blocking-observer", &callback_root)?;
                    panic!("Controlled blocking callback panic");
                })
            })
        };
        #[cfg(feature = "test-support")]
        let panic_task = crate::vault::testing::with_observer(
            move |event| {
                if event.operation == "boxed-blocking-observer" {
                    observed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                }
                Ok(crate::vault::testing::Action::Continue)
            },
            spawn,
        );
        #[cfg(not(feature = "test-support"))]
        let panic_task = spawn();
        drop(owner);
        assert!(
            runtime
                .block_on(panic_task)
                .is_err_and(|error| error.is_panic())
        );
        #[cfg(feature = "test-support")]
        assert_eq!(observations.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(std::fs::read_dir(root.join(".updates/work"))?.count(), 0);
        let owner = Arc::new(Reservation::begin(root, "fixture-http-result-type")?);
        let refused = {
            let _runtime = runtime.enter();
            REQUEST_OWNER.sync_scope(owner.clone(), || {
                handle::Blocking::<u8>::new(blocking_boxed(Box::new(|| Box::new(()))))
            })
        };
        drop(owner);
        assert!(matches!(
            runtime.block_on(refused),
            Err(handle::Failure::ResultType)
        ));
        assert_eq!(std::fs::read_dir(root.join(".updates/work"))?.count(), 0);
        temporary.cleanup()?;
        Ok(())
    }
}
