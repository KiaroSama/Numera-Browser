//! Individual source acceptance contracts, with real native API/process effects.
use axum::{
    Router,
    body::{Body, to_bytes},
    http::Request,
};
use numera_core::{conversion, storage::repo::ProfileRepo};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, io, path::Path, process::Command, sync::Arc, time::Duration};
use tower::ServiceExt;
mod common;
type TestResult = Result<(), Box<dyn std::error::Error>>;
macro_rules! original {
    ($name:ident, $id:literal, $body:block) => { #[test] #[doc=$id] fn $name() -> TestResult $body };
}
struct Http {
    runtime: tokio::runtime::Runtime,
    app: Router,
}
impl Http {
    fn new(root: &Path, password: Option<String>) -> Result<Self, Box<dyn std::error::Error>> {
        Self::configured(root, password, "", None, "fixture-private-update-token")
    }
    fn configured(
        root: &Path,
        password: Option<String>,
        environment: &str,
        ticket: Option<String>,
        update_token: &str,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let sessions = Arc::new(numera_core::ownership::registry::Registry::new(
            root.to_owned(),
            "acceptance-host".into(),
            password.clone(),
        )?);
        let app = runtime.block_on(numera_core::http::application::router_with_recovery(
            root.to_owned(),
            environment,
            "",
            password,
            "acceptance-host",
            numera_core::http::application::SessionApplication {
                sessions,
                installed: ["playwright", "patchright", "camoufox", "cloak"]
                    .into_iter()
                    .map(|name| (name.into(), true))
                    .collect::<BTreeMap<_, _>>(),
            },
            (None, None),
            numera_core::http::application::RecoveryOptions {
                ticket,
                update_token: update_token.into(),
            },
        ))?;
        Ok(Self { runtime, app })
    }
    fn request(
        &self,
        method: &str,
        path: &str,
        body: Value,
    ) -> Result<(u16, Value), Box<dyn std::error::Error>> {
        self.raw(
            method,
            path,
            if body.is_null() {
                Vec::new()
            } else {
                serde_json::to_vec(&body)?
            },
            &[],
        )
    }
    fn authorized(
        &self,
        method: &str,
        path: &str,
        body: Value,
        token: &str,
    ) -> Result<(u16, Value), Box<dyn std::error::Error>> {
        self.raw(
            method,
            path,
            serde_json::to_vec(&body)?,
            &[("authorization", &format!("Bearer {token}"))],
        )
    }
    fn raw(
        &self,
        method: &str,
        path: &str,
        body: Vec<u8>,
        headers: &[(&str, &str)],
    ) -> Result<(u16, Value), Box<dyn std::error::Error>> {
        self.runtime.block_on(async {
            let mut request = Request::builder()
                .method(method)
                .uri(path)
                .header("content-type", "application/json");
            for (key, value) in headers {
                request = request.header(*key, *value);
            }
            let request = request.body(Body::from(body))?;
            let response = self.app.clone().oneshot(request).await?;
            let status = response.status().as_u16();
            let bytes = to_bytes(response.into_body(), 1_048_576).await?;
            Ok((
                status,
                if bytes.is_empty() {
                    Value::Null
                } else {
                    serde_json::from_slice(&bytes)?
                },
            ))
        })
    }
    fn list(&self) -> Result<Value, Box<dyn std::error::Error>> {
        let (status, body) = self.request("GET", "/api/profiles", Value::Null)?;
        assert_eq!(status, 200);
        Ok(body)
    }
    fn import(&self, document: &Value) -> Result<(u16, Value), Box<dyn std::error::Error>> {
        self.request(
            "POST",
            "/api/profiles/import",
            json!({"text":serde_json::to_string(document)?}),
        )
    }
}
#[path = "source_acceptance/export.rs"]
mod export;
#[path = "source_acceptance/self_command.rs"]
mod self_command;
#[path = "source_acceptance/update.rs"]
mod update;
