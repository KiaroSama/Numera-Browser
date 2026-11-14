//! Native transport ownership. No Python imports, implicit acquisition, process
//! group escape, or SDK-installed signal handlers are involved.
use playwright_rs::server::{
    channel_owner::ChannelOwner,
    connection::{Connection, ConnectionExt, ConnectionLike},
    transport::PipeTransport,
};
use playwright_rs::{BrowserContext, Playwright};
use serde_json::{Value, json};
use std::{
    io::{self, Read},
    path::PathBuf,
    process::Stdio,
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::AsyncReadExt,
    process::{Child, Command},
    task::JoinHandle,
};
const STARTUP_STDERR_LIMIT: usize = 65_536;

#[derive(Default)]
struct DiagnosticBytes {
    bytes: Vec<u8>,
    observed: u64,
    eof: bool,
    read_failed: bool,
}
impl DiagnosticBytes {
    fn append(&mut self, bytes: &[u8]) {
        self.observed = self.observed.saturating_add(bytes.len() as u64);
        let keep = bytes
            .len()
            .min(STARTUP_STDERR_LIMIT.saturating_sub(self.bytes.len()));
        self.bytes.extend_from_slice(&bytes[..keep]);
    }
    fn summary(&self) -> Value {
        // These fixed byte signatures are observations, never a startup verdict.
        // Neither matching bytes nor surrounding vendor text leave this boundary.
        const SIGNATURES: &[(&str, &[u8])] = &[
            ("node-module-not-found", b"MODULE_NOT_FOUND"),
            ("node-esm-module-not-found", b"ERR_MODULE_NOT_FOUND"),
            (
                "node-unsupported-url-scheme",
                b"ERR_UNSUPPORTED_ESM_URL_SCHEME",
            ),
            ("node-invalid-argument-type", b"ERR_INVALID_ARG_TYPE"),
            ("node-syntax-error", b"SyntaxError:"),
            ("node-reference-error", b"ReferenceError:"),
            (
                "dynamic-library-loader",
                b"error while loading shared libraries:",
            ),
        ];
        let signatures = SIGNATURES
            .iter()
            .filter(|(_, bytes)| {
                self.bytes
                    .windows(bytes.len())
                    .any(|window| window == *bytes)
            })
            .map(|(label, _)| *label)
            .collect::<Vec<_>>();
        json!({
            "observed_bytes":self.observed,
            "captured_bytes":self.bytes.len(),
            "truncated":self.observed > self.bytes.len() as u64,
            "sample_complete":self.eof && !self.read_failed && self.observed == self.bytes.len() as u64,
            "read_state":if self.eof {"eof"} else if self.read_failed {"read-error"} else {"reading"},
            "signatures":signatures
        })
    }
}
fn initialization_timeout_pattern(error: &playwright_rs::Error) -> Option<&'static str> {
    // This identifies a public message pattern only. A vendor TimeoutError can
    // mimic the same text; the opaque error cannot prove a typed timer branch.
    match error {
        playwright_rs::Error::Timeout(message)
            if message == "Playwright initialization timeout after 30 seconds" =>
        {
            Some("root-reply")
        }
        // Object-registration timeouts include an opaque GUID. The public error
        // carries no typed provenance, so do not infer its origin from a prefix.
        playwright_rs::Error::Timeout(_) => Some("unknown"),
        _ => None,
    }
}
fn report_initialization(
    boundary: &'static str,
    error: Option<&playwright_rs::Error>,
    child: Value,
    stderr: Value,
) {
    let summary = json!({
        "boundary":boundary,
        "category":error.map(super::error_category),
        "timeout_pattern":error.and_then(initialization_timeout_pattern),
        "child":child,
        "stderr":stderr
    });
    // The native adapter discards worker stderr. Retain the same static-only
    // summary in its already configured log directory as well as the smoke sink.
    if let Some(directory) = std::env::var_os("NUMERA_ADAPTER_LOG_DIR") {
        let log =
            crate::runtime_log::RuntimeLog::file_only(&PathBuf::from(directory), "browser-driver");
        log.message(
            if error.is_some() { "ERROR" } else { "INFO" },
            "numera.browser-adapter",
            &format!("SDK initialization observation={summary}"),
        );
    }
    // Reuse the bounded reporting sink with an entirely constructed message.
    // Never pass the actual vendor error, stderr, paths, argv or GUIDs to it.
    let safe = playwright_rs::Error::TransportError(format!(
        "Native SDK initialization observation={summary}"
    ));
    super::container_smoke::report_failure(
        crate::browser::container_smoke::Phase::Start,
        &safe,
        None,
    );
}
struct DiagnosticOutput {
    bytes: Arc<std::sync::Mutex<DiagnosticBytes>>,
    starting: Arc<std::sync::atomic::AtomicBool>,
    task: JoinHandle<()>,
}

pub struct Installation {
    pub node: PathBuf,
    pub cli: PathBuf,
    pub version: String,
}
impl Installation {
    pub fn discover(backend: &str) -> io::Result<Self> {
        let key = match backend {
            "playwright" => "NUMERA_PLAYWRIGHT_DRIVER",
            "camoufox" => "NUMERA_CAMOUFOX_DRIVER",
            "patchright" => "NUMERA_PATCHRIGHT_DRIVER",
            "cloak" => "NUMERA_CLOAK_DRIVER",
            _ => return Err(io::Error::other("Unknown SDK backend")),
        };
        let root = std::env::var_os(key)
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("NUMERA_BROWSER_DRIVERS")
                    .map(|root| PathBuf::from(root).join(backend))
            })
            .or_else(|| {
                std::env::current_exe().ok().and_then(|exe| {
                    exe.parent()
                        .map(|parent| parent.join("drivers").join(backend))
                })
            })
            .ok_or_else(|| io::Error::other("Browser SDK driver has not been provisioned"))?;
        let node = root.join(if cfg!(windows) { "node.exe" } else { "node" });
        let cli = root.join("package/cli.js");
        if !node.is_file() || !cli.is_file() {
            return Err(io::Error::other("Browser SDK driver is incomplete"));
        }
        let manifest = root.join("package/package.json");
        let mut bytes = Vec::new();
        std::fs::File::open(manifest)?
            .take(65537)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 65536 {
            return Err(io::Error::other("Browser SDK manifest exceeds limit"));
        }
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|_| io::Error::other("Browser SDK manifest is malformed"))?;
        let version = value["version"]
            .as_str()
            .filter(|version| {
                let mut parts = version.split('.');
                parts.next() == Some("1")
                    && match parts.next() {
                        Some("62") => backend == "camoufox",
                        Some("64") => backend != "camoufox",
                        Some("65") => matches!(backend, "playwright" | "cloak"),
                        _ => false,
                    }
                    && parts.next().is_some_and(|part| part.parse::<u32>().is_ok())
                    && parts.next().is_none()
            })
            .ok_or_else(|| io::Error::other("Browser SDK protocol version is unsupported"))?
            .to_owned();
        Ok(Self { node, cli, version })
    }
}

pub struct Driver {
    child: Child,
    connection: Arc<Connection>,
    dispatch: JoinHandle<()>,
    playwright: Option<Playwright>,
    diagnostics: Option<DiagnosticOutput>,
}
impl Driver {
    pub(super) fn playwright(&self) -> io::Result<&Playwright> {
        self.playwright
            .as_ref()
            .ok_or_else(|| io::Error::other("SDK not initialized"))
    }
    pub async fn start(installation: Installation) -> playwright_rs::Result<Self> {
        Self::start_inner(installation, false).await
    }
    pub(super) async fn start_for_smoke(installation: Installation) -> playwright_rs::Result<Self> {
        Self::start_inner(installation, true).await
    }
    async fn start_inner(
        installation: Installation,
        diagnostics: bool,
    ) -> playwright_rs::Result<Self> {
        let mut child = Command::new(installation.node)
            .arg(installation.cli)
            .arg("run-driver")
            .env("PW_LANG_NAME", "rust")
            .env("PW_LANG_NAME_VERSION", env!("CARGO_PKG_RUST_VERSION"))
            .env("PW_CLI_DISPLAY_VERSION", env!("CARGO_PKG_VERSION"))
            .env("CLOAKBROWSER_AUTO_UPDATE", "false")
            .env_remove("NUMERA_PASSWORD")
            .env_remove("NUMERA_API_TOKEN")
            .env_remove("NUMERA_UPDATE_TOKEN")
            .env_remove("NUMERA_UPDATE_TICKET")
            .env_remove("NUMERA_UPDATE_ACTIVATION")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;
        let diagnostics = child.stderr.take().map(|mut stderr| {
            let bytes = Arc::new(std::sync::Mutex::new(DiagnosticBytes::default()));
            let captured = bytes.clone();
            let starting = Arc::new(std::sync::atomic::AtomicBool::new(true));
            let startup = starting.clone();
            let task = tokio::spawn(async move {
                let mut buffer = [0; 8192];
                loop {
                    let count = match stderr.read(&mut buffer).await {
                        Ok(count) => count,
                        Err(_) => {
                            if let Ok(mut output) = captured.lock() {
                                output.read_failed = true;
                            }
                            break;
                        }
                    };
                    if count == 0 {
                        let summary = captured.lock().ok().and_then(|mut output| {
                            output.eof = true;
                            (diagnostics
                                && startup.load(std::sync::atomic::Ordering::Acquire)
                                && output.observed > 0)
                                .then(|| output.summary())
                        });
                        // Release the capture lock before either file or console I/O.
                        if let Some(summary) = summary {
                            // Preserve smoke's early EOF evidence before an outer idle
                            // deadline; this is an observation, not an error verdict.
                            report_initialization(
                                "startup-stderr-eof",
                                None,
                                json!({"state":"unavailable","sampled":false}),
                                summary,
                            );
                        }
                        break;
                    }
                    if let Ok(mut output) = captured.lock()
                        && startup.load(std::sync::atomic::Ordering::Acquire)
                    {
                        output.append(&buffer[..count]);
                    }
                    // Keep draining after the capture cap to avoid a blocked child.
                }
            });
            DiagnosticOutput {
                bytes,
                starting,
                task,
            }
        });
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("SDK input missing"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("SDK output missing"))?;
        let (transport, messages) = PipeTransport::new(stdin, stdout);
        let (sender, receiver) = transport.into_parts();
        let connection = Arc::new(Connection::new(sender, receiver, messages));
        let dispatch = {
            let connection = Arc::clone(&connection);
            tokio::spawn(async move {
                connection.run().await;
            })
        };
        let mut driver = Self {
            child,
            connection,
            dispatch,
            playwright: None,
            diagnostics,
        };
        let initialized = async {
            let object = driver.connection.initialize_playwright().await?;
            driver
                .connection
                .get_typed::<Playwright>(object.guid())
                .await
        }
        .await;
        match initialized {
            Ok(playwright) => {
                if let Some(diagnostics) = &driver.diagnostics {
                    // Serialize the capture flag and buffer release with append:
                    // no late read can refill the startup buffer after success.
                    let mut output = diagnostics.bytes.lock().ok();
                    diagnostics
                        .starting
                        .store(false, std::sync::atomic::Ordering::Release);
                    if let Some(output) = output.as_mut() {
                        output.bytes = Vec::new();
                    }
                }
                driver.playwright = Some(playwright);
                Ok(driver)
            }
            Err(error) => {
                // Observe the real child before the unchanged close/wait/kill path.
                let child = match driver.child.try_wait() {
                    Ok(Some(status)) => json!({
                        "state":"exited","sampled":true,"success":status.success(),"code":status.code()
                    }),
                    Ok(None) => json!({"state":"running","sampled":true}),
                    Err(error) => json!({
                        "state":"query-failed","sampled":true,"os_error":error.raw_os_error()
                    }),
                };
                let stderr = driver
                    .diagnostics
                    .as_ref()
                    .and_then(|output| output.bytes.lock().ok().map(|bytes| bytes.summary()))
                    .unwrap_or_else(|| json!({"read_state":"unavailable"}));
                let _ = driver.stop().await;
                report_initialization("initialization-failed", Some(&error), child, stderr);
                Err(error)
            }
        }
    }

    pub async fn open(
        &self,
        backend: &str,
        options: Value,
    ) -> playwright_rs::Result<BrowserContext> {
        let request = super::launch_request(
            backend,
            options
                .as_object()
                .ok_or_else(|| io::Error::other("SDK launch options are invalid"))?,
        )?;
        let playwright = self
            .playwright
            .as_ref()
            .ok_or_else(|| io::Error::other("SDK not initialized"))?;
        let browser = if request.family == "firefox" {
            playwright.firefox()
        } else {
            playwright.chromium()
        };
        // Use the protocol without the Rust binding's test-only Windows launch
        // flags, which would otherwise disable web security in user profiles.
        let response: Value = browser
            .channel()
            .send(request.method, request.options)
            .await?;
        let guid = response["context"]["guid"]
            .as_str()
            .ok_or_else(|| io::Error::other("SDK context reference missing"))?;
        browser
            .connection()
            .wait_for_typed::<BrowserContext>(guid)
            .await
    }

    pub async fn stop(mut self) -> io::Result<()> {
        self.connection.close_writer();
        let result = match tokio::time::timeout(Duration::from_secs(10), self.child.wait()).await {
            Ok(Ok(status)) if status.success() => Ok(()),
            Ok(Ok(_)) => Err(io::Error::other("SDK driver exit was unsuccessful")),
            Ok(Err(error)) => Err(error),
            Err(_) => {
                let _ = self.child.kill().await;
                let _ = self.child.wait().await;
                Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "SDK driver shutdown exceeded deadline",
                ))
            }
        };
        self.dispatch.abort();
        let _ = (&mut self.dispatch).await;
        if let Some(mut diagnostics) = self.diagnostics.take() {
            diagnostics.task.abort();
            let _ = (&mut diagnostics.task).await;
        }
        self.playwright.take();
        result
    }
}
impl Drop for Driver {
    fn drop(&mut self) {
        self.connection.close_writer();
        self.dispatch.abort();
        if let Some(diagnostics) = &self.diagnostics {
            diagnostics.task.abort();
        }
    }
}

pub fn metadata() -> Value {
    let mut backends = serde_json::Map::new();
    for name in ["playwright", "patchright", "camoufox", "cloak"] {
        let installation = Installation::discover(name).ok();
        backends.insert(
            name.into(),
            json!({
                "installed":installation.is_some(),
                "version":installation.map(|installation| installation.version)
            }),
        );
    }
    json!({"backends":backends,"default_addons":["UBO"],"runtime":concat!("rust/",env!("CARGO_PKG_RUST_VERSION"))})
}
