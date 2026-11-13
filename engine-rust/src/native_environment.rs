//! Deployment discovery for the native application and browser worker.
use crate::{browser::runtime::Runtime, ownership::capture};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    io,
    path::{Path, PathBuf},
    time::Duration,
};

pub struct Environment {
    pub app: PathBuf,
    pub home: PathBuf,
    pub cwd: PathBuf,
    pub browser: Runtime,
    pub host: String,
}
impl Environment {
    pub fn discover() -> io::Result<Self> {
        let executable = std::env::current_exe()?;
        let app = std::env::var_os("NUMERA_APP_DIR")
            .map(PathBuf::from)
            .or_else(|| executable.parent().map(Path::to_owned))
            .ok_or_else(|| io::Error::other("Application directory unavailable"))?;
        let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
            .map(PathBuf::from)
            .ok_or_else(|| io::Error::other("Home directory unavailable"))?;
        let adapter = std::env::var_os("NUMERA_BROWSER_RUNTIME")
            .or_else(|| std::env::var_os("NUMERA_BROWSER_ADAPTER"))
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                app.join(if cfg!(windows) {
                    "numera-browser-runtime.exe"
                } else {
                    "numera-browser-runtime"
                })
            });
        let logs = std::env::var_os("NUMERA_LOG_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| app.join("logs"));
        let cwd = std::env::current_dir()?;
        let host = crate::native_host::name()?;
        Ok(Self {
            app: app.clone(),
            home: home.clone(),
            cwd: cwd.clone(),
            host,
            browser: Runtime {
                adapter,
                cwd,
                home,
                logs,
                country: None,
                relay: None,
                binary_options: Default::default(),
                deployment: Some(crate::browser::runtime::Deployment {
                    app: app.clone(),
                    environment: [
                        "PLAYWRIGHT_BROWSERS_PATH",
                        "CLOAKBROWSER_CACHE_DIR",
                        "CLOAKBROWSER_BINARY_PATH",
                        "CLOAKBROWSER_LICENSE_KEY",
                    ]
                    .into_iter()
                    .filter_map(|key| std::env::var(key).ok().map(|value| (key.into(), value)))
                    .collect(),
                    default_addons: Vec::new(),
                }),
            },
        })
    }
    pub fn installed(&mut self) -> Result<BTreeMap<String, bool>, capture::Failure> {
        self.metadata(false)
    }
    pub fn metadata(
        &mut self,
        addons_requested: bool,
    ) -> Result<BTreeMap<String, bool>, capture::Failure> {
        if !self.browser.adapter.is_file() {
            return Ok(["playwright", "patchright", "camoufox", "cloak"]
                .into_iter()
                .map(|name| (name.into(), false))
                .collect());
        }
        let mut command = self.browser.sdk_command();
        command.arg("--metadata");
        command
            .env_remove("NUMERA_PASSWORD")
            .env_remove("NUMERA_API_TOKEN");
        let result = capture::run(
            &mut command,
            capture::Limits {
                wall: Duration::from_secs(10),
                idle: Duration::from_secs(10),
                bytes: 65536,
                cleanup: Duration::from_secs(5),
            },
        )?;
        if result.exit != 0 {
            return Err(io::Error::other("Browser SDK metadata discovery failed").into());
        }
        let value: Value = serde_json::from_slice(&result.stdout).map_err(io::Error::other)?;
        let mut installed = BTreeMap::new();
        for name in ["playwright", "patchright", "camoufox", "cloak"] {
            let present = value["backends"][name]["installed"]
                .as_bool()
                .ok_or_else(|| io::Error::other("Browser SDK metadata is malformed"))?;
            installed.insert(name.into(), present);
        }
        let addons = value["default_addons"]
            .as_array()
            .ok_or_else(|| io::Error::other("Browser SDK addon metadata is malformed"))?
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| io::Error::other("Browser SDK addon name is not text"))
            })
            .collect::<io::Result<Vec<_>>>()?;
        if let Some(deployment) = self.browser.deployment.as_mut() {
            deployment.default_addons = if addons_requested && installed["camoufox"] {
                addons
            } else {
                Vec::new()
            };
        }
        Ok(installed)
    }
}
