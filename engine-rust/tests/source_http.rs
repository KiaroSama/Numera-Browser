//! Individually reported native ports of the frozen original HTTP/API tests.
//! Source identities and concrete adaptations are recorded in the mapping file.
use axum::{
    Router,
    body::{Body, to_bytes},
    http::{HeaderMap, Request},
};
use numera_core::{
    conversion,
    http::{
        auth_policy::{self, AuthPolicy},
        tokens::{TokenState, TokenStore},
    },
    models::Profile,
    storage::{config::ConfigStore, repo::ProfileRepo},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs, io,
    path::Path,
    process::{Command, Output},
    sync::Arc,
};
use tower::ServiceExt;
mod common;
type TestResult = Result<(), Box<dyn std::error::Error>>;
macro_rules! source_test {
    ($name:ident,$source:literal,$body:block) => { #[test] #[doc=$source] fn $name()->TestResult $body };
}
fn fresh() -> common::TempDirectory {
    common::TempDirectory::new("source-http").expect("exclusive HTTP fixture")
}
struct Reply {
    status: u16,
    headers: HeaderMap,
    body: Value,
    bytes: Vec<u8>,
}
impl Reply {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.bytes).into_owned()
    }
}
struct Http {
    runtime: tokio::runtime::Runtime,
    app: Router,
}
impl Http {
    fn new(root: &Path) -> Self {
        Self::configured(root, "", "", None)
    }
    fn configured(root: &Path, environment: &str, origins: &str, dashboard: Option<&Path>) -> Self {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("HTTP runtime");
        let sessions = Arc::new(
            numera_core::ownership::registry::Registry::new(
                root.to_owned(),
                "source-http-host".into(),
                None,
            )
            .expect("HTTP registry"),
        );
        let app = runtime
            .block_on(numera_core::http::application::router_with_dashboard(
                root.to_owned(),
                environment,
                origins,
                None,
                "source-http-host",
                numera_core::http::application::SessionApplication {
                    installed: BTreeMap::new(),
                    sessions,
                },
                (None, dashboard),
            ))
            .expect("native application");
        Self { runtime, app }
    }
    fn raw(
        &self,
        method: &str,
        path: &str,
        bytes: Vec<u8>,
        token: Option<&str>,
        origin: Option<&str>,
    ) -> Reply {
        self.runtime.block_on(async {
            let mut builder = Request::builder()
                .method(method)
                .uri(path)
                .header("content-type", "application/json");
            if let Some(token) = token {
                builder = builder.header("authorization", format!("Bearer {token}"));
            }
            if let Some(origin) = origin {
                builder = builder.header("origin", origin);
            }
            let response = self
                .app
                .clone()
                .oneshot(builder.body(Body::from(bytes)).expect("HTTP request"))
                .await
                .expect("HTTP response");
            let status = response.status().as_u16();
            let headers = response.headers().clone();
            let bytes = to_bytes(response.into_body(), 16 * 1024 * 1024)
                .await
                .expect("bounded HTTP response")
                .to_vec();
            let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
            Reply {
                status,
                headers,
                body,
                bytes,
            }
        })
    }
    fn request(&self, method: &str, path: &str, body: Value, token: Option<&str>) -> Reply {
        self.raw(
            method,
            path,
            if body.is_null() {
                vec![]
            } else {
                serde_json::to_vec(&body).expect("request JSON")
            },
            token,
            None,
        )
    }
    fn get(&self, path: &str, token: Option<&str>) -> Reply {
        self.request("GET", path, Value::Null, token)
    }
    fn post(&self, path: &str, body: Value, token: Option<&str>) -> Reply {
        self.request("POST", path, body, token)
    }
    fn put(&self, path: &str, body: Value) -> Reply {
        self.request("PUT", path, body, None)
    }
    fn delete(&self, path: &str, token: Option<&str>) -> Reply {
        self.request("DELETE", path, Value::Null, token)
    }
    fn create(&self, name: &str) -> Value {
        let r = self.post(
            "/api/profiles",
            json!({"name":name,"engine":"playwright"}),
            None,
        );
        assert_eq!(r.status, 200, "{}", r.text());
        r.body
    }
}
fn text(value: &Value, key: &str) -> String {
    value[key].as_str().expect("text field").to_owned()
}
fn read(path: impl AsRef<Path>) -> Value {
    serde_json::from_slice(&fs::read(path).expect("persisted file")).expect("persisted JSON")
}
fn snapshot(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn visit(base: &Path, path: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        if !path.exists() {
            return;
        }
        for entry in fs::read_dir(path).expect("snapshot directory") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                visit(base, &path, out)
            } else if path.extension().is_some_and(|e| e == "json") {
                out.insert(
                    path.strip_prefix(base)
                        .expect("child")
                        .to_string_lossy()
                        .into_owned(),
                    fs::read(&path).expect("state bytes"),
                );
            }
        }
    }
    let mut out = BTreeMap::new();
    visit(root, root, &mut out);
    out
}
fn profile(name: &str) -> Profile {
    let mut entropy = [0u8; 8];
    getrandom::fill(&mut entropy).expect("profile fixture identity");
    conversion::to_engine(&json!({"id":format!("source-{:016x}",u64::from_le_bytes(entropy)),"name":name,"engine":"playwright"}),1000.0).expect("coherent profile")
}
fn token(root: &Path, name: &str) -> Value {
    TokenStore::new(root.to_owned())
        .create(name, 1000.0)
        .expect("mint token")
}
fn legacy(root: &Path, body: Option<&str>) {
    fs::write(root.join("tokens.json"),body.map(str::to_owned).unwrap_or_else(||json!([{"id":"aaaaaaaaaaaa","name":"CI","hash":TokenStore::hash("nmr_legacy-token-value"),"created":1.0,"lastUsed":null}]).to_string())).expect("legacy credentials");
    assert!(!root.join("auth-policy.json").exists());
}
fn cli(root: &Path, args: &[&str], json_mode: bool) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_numera-engine"));
    command.arg("--data-dir").arg(root).args(args);
    if json_mode {
        command.arg("--json");
    }
    command
        .env_remove("NUMERA_API_TOKEN")
        .env_remove("NUMERA_PASSWORD")
        .env_remove("NUMERA_UPDATE_TICKET")
        .env_remove("NUMERA_BROWSER_RUNTIME")
        .output()
        .expect("native CLI subprocess")
}
fn payload(output: &Output) -> Value {
    assert!(
        !output.stdout.is_empty(),
        "empty CLI output; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("one complete CLI JSON document")
}
#[path = "source_http/auth.rs"]
mod auth_cases;
#[path = "source_http/benchmark.rs"]
mod benchmark_cases;
#[path = "source_http/mcp.rs"]
mod mcp_cases;
#[path = "source_http/numeric.rs"]
mod numeric_cases;
#[path = "source_http/product.rs"]
mod product_cases;
#[path = "source_http/server.rs"]
mod server_cases;
#[path = "source_http/startup.rs"]
mod startup_cases;
#[path = "source_http/ui.rs"]
mod ui_cases;

#[path = "source_http/agent.rs"]
mod agent_cases;
