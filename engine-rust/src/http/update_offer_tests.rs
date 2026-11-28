use super::*;
use axum::body::Body;
use std::sync::Mutex;

fn application(root: &Path) -> io::Result<Arc<Updates>> {
    Ok(Arc::new(Updates {
        admission: Admission::new(root, "offer-test")?,
        sessions: Arc::new(Registry::new(
            root.to_path_buf(),
            "offer-test".into(),
            None,
        )?),
    }))
}

fn apply_request(body: &Value) -> Request {
    Request::builder()
        .method("POST")
        .uri("/api/updates/apply")
        .header("content-type", "application/json")
        .header("x-numera-update-approval", "c".repeat(64))
        .body(Body::from(body.to_string()))
        .expect("Synthetic apply request")
}

#[test]
fn manual_update_apply_requires_a_valid_reviewed_offer_before_any_bridge_call()
-> Result<(), Box<dyn std::error::Error>> {
    let staging = crate::storage::archive_paths::Staging::new(
        &std::env::temp_dir(),
        "numera-offer-validation-",
    )?;
    let state = application(staging.path()?)?;
    let desktop = Arc::new(Desktop {
        token: String::new(),
        endpoint: None,
    });
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let mut bodies = vec![json!({"channel":"all", "clean_old":false})];
        for offer in [
            Value::Null,
            json!(1),
            json!(""),
            json!("a".repeat(63)),
            json!("a".repeat(65)),
            json!("G".repeat(64)),
            json!("A".repeat(64)),
        ] {
            bodies.push(json!({"channel":"all", "clean_old":false, "offerId":offer}));
        }
        for body in bodies {
            let response = apply(
                State(state.clone()),
                Extension(desktop.clone()),
                apply_request(&body),
            )
            .await;
            assert_eq!(
                response.status(),
                StatusCode::BAD_REQUEST,
                "Unbound or malformed approval reached update dispatch: {body}"
            );
        }
    });
    Ok(())
}

#[test]
fn update_http_bridge_forwards_the_reviewed_offer_and_preserves_stale_refusal()
-> Result<(), Box<dyn std::error::Error>> {
    let staging = crate::storage::archive_paths::Staging::new(
        &std::env::temp_dir(),
        "numera-offer-forward-",
    )?;
    let state = application(staging.path()?)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let requests = Arc::new(Mutex::new(Vec::<Value>::new()));
        let observed = requests.clone();
        let bridge = Router::new()
            .route("/api/apply", post(move |Json(body): Json<Value>| {
                let observed = observed.clone();
                async move {
                    observed.lock().expect("Captured request lock").push(body);
                    (StatusCode::CONFLICT, Json(json!({"detail":"Update offer is no longer current"})))
                }
            }))
            .route("/api/check", post(|| async {
                Json(json!({"state":"available", "offerId":"b".repeat(64)}))
            }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        let server = tokio::spawn(async move {
            axum::serve(listener, bridge)
                .with_graceful_shutdown(async { let _ = stopped.await; })
                .await
        });
        let desktop = Arc::new(Desktop {
            token: "synthetic-bridge-token".into(),
            endpoint: Some((port, reqwest::Client::builder().no_proxy().timeout(Duration::from_secs(2)).build()?)),
        });
        let response = apply(State(state), Extension(desktop.clone()), apply_request(
            &json!({"channel":"all", "clean_old":false, "offerId":"a".repeat(64)}),
        )).await;
        let status = response.status();
        let body = to_bytes(response.into_body(), 4096).await?;
        let checked = check(Extension(desktop), Request::builder()
            .uri("/api/updates/check?channel=all").body(Body::empty())?).await;
        let checked_body = to_bytes(checked.into_body(), 4096).await?;
        let _ = stop.send(());
        tokio::time::timeout(Duration::from_secs(3), server).await???;
        assert_eq!(status, StatusCode::CONFLICT, "An exact-offer request was rejected or changed before reaching the bridge");
        assert_eq!(serde_json::from_slice::<Value>(&body)?["detail"], "Update offer is no longer current");
        assert_eq!(*requests.lock().expect("Captured request lock"), vec![json!({
            "channel":"all", "clean_old":false, "offerId":"a".repeat(64), "approval":"c".repeat(64),
        })]);
        assert_eq!(serde_json::from_slice::<Value>(&checked_body)?["offerId"], "b".repeat(64));
        Ok::<(), Box<dyn std::error::Error>>(())
    })
}
