//! Deployment discovery for the native candidate, never source-engine forwarding.
use crate::{browser::runtime::Runtime, ownership::capture};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    io,
    path::{Path, PathBuf},
    process::Command,
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
        let adapter = std::env::var_os("NUMERA_BROWSER_ADAPTER")
            .map(PathBuf::from)
            .unwrap_or_else(|| app.join("scripts/browser-adapter/adapter.py"));
        let python = std::env::var_os("NUMERA_SDK_PYTHON")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(if cfg!(windows) {
                    "python.exe"
                } else {
                    "python3"
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
                python,
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
    pub fn metadata(&mut self, addons: bool) -> Result<BTreeMap<String, bool>, capture::Failure> {
        if !self.browser.adapter.is_file() {
            return Ok(["playwright", "patchright", "camoufox", "cloak"]
                .into_iter()
                .map(|name| (name.into(), false))
                .collect());
        }
        let mut command = Command::new(&self.browser.python);
        command.args(["-I", "-u", "-c", "import importlib.util,json,sys; s=importlib.util.spec_from_file_location('sdk_metadata',sys.argv[1]); m=importlib.util.module_from_spec(s); s.loader.exec_module(m); data=m.available(); addons=[];\nif sys.argv[2]=='1' and data['camoufox']['installed']:\n from camoufox.addons import DefaultAddons\n addons=[v.name for v in DefaultAddons]\nprint(json.dumps({'backends':data,'default_addons':addons}),flush=True)"])
            .arg(&self.browser.adapter).arg(if addons { "1" } else { "0" })
            .env_remove("NUMERA_PASSWORD").env_remove("NUMERA_API_TOKEN");
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
            deployment.default_addons = addons;
        }
        Ok(installed)
    }
}
