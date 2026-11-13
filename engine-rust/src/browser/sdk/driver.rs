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
    process::{Child, Command},
    task::JoinHandle,
};

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
}
impl Driver {
    pub(super) fn playwright(&self) -> io::Result<&Playwright> {
        self.playwright
            .as_ref()
            .ok_or_else(|| io::Error::other("SDK not initialized"))
    }
    pub async fn start(installation: Installation) -> playwright_rs::Result<Self> {
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
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()?;
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
                driver.playwright = Some(playwright);
                Ok(driver)
            }
            Err(error) => {
                let _ = driver.stop().await;
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
        self.playwright.take();
        result
    }
}
impl Drop for Driver {
    fn drop(&mut self) {
        self.connection.close_writer();
        self.dispatch.abort();
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
