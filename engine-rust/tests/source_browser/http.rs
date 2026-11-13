use super::*;
use axum::{
    Router,
    body::{Body, to_bytes},
    http::Request,
};
use std::{collections::BTreeMap, path::Path, sync::Arc};
use tower::ServiceExt;
pub struct Http {
    runtime: tokio::runtime::Runtime,
    app: Router,
}
impl Http {
    pub fn new(root: &Path) -> Self {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("browser contract");
        let sessions = Arc::new(
            numera_core::ownership::registry::Registry::new(
                root.into(),
                "source-browser-host".into(),
                None,
            )
            .expect("browser contract"),
        );
        let app = runtime
            .block_on(numera_core::http::application::router_with_dashboard(
                root.into(),
                "",
                "",
                None,
                "source-browser-host",
                numera_core::http::application::SessionApplication {
                    installed: BTreeMap::new(),
                    sessions,
                },
                (None, None),
            ))
            .expect("browser contract");
        Self { runtime, app }
    }
    pub fn request(&self, method: &str, path: &str, body: Value) -> Value {
        self.runtime.block_on(async {
            let request = Request::builder()
                .method(method)
                .uri(path)
                .header("content-type", "application/json")
                .body(Body::from(if body.is_null() {
                    Vec::new()
                } else {
                    serde_json::to_vec(&body).expect("browser contract")
                }))
                .expect("browser contract");
            let response = self
                .app
                .clone()
                .oneshot(request)
                .await
                .expect("browser contract");
            let status = response.status();
            let bytes = to_bytes(response.into_body(), 16 * 1024 * 1024)
                .await
                .expect("browser contract");
            assert!(
                status == axum::http::StatusCode::OK,
                "HTTP {status}: {}",
                String::from_utf8_lossy(&bytes)
            );
            serde_json::from_slice(&bytes).expect("browser contract")
        })
    }
}
