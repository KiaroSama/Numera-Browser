use super::{
    adapter::Adapter,
    launch::{self, PreparedLaunch},
    registry::{DriverContext, DriverRegistry, OptionsFamily, ResolvedDriver},
};
use crate::{
    models::Profile,
    ownership::{ready::StartupFailed, session::Session},
};
use serde_json::{Map, Value};
use std::{collections::BTreeMap, io, path::PathBuf, process::Command, sync::Arc};

/// Deployment-selected SDK transport and independently observed launch resources.
/// Only the bounded native SDK protocol crosses this process boundary.
#[derive(Clone)]
pub struct Deployment {
    pub app: PathBuf,
    pub environment: std::collections::BTreeMap<String, String>,
    pub default_addons: Vec<String>,
}
#[derive(Clone)]
pub struct Runtime {
    pub adapter: PathBuf,
    pub cwd: PathBuf,
    pub home: PathBuf,
    pub logs: PathBuf,
    pub country: Option<String>,
    pub relay: Option<String>,
    pub binary_options: Map<String, Value>,
    pub deployment: Option<Deployment>,
    pub registry: Option<Arc<DriverRegistry>>,
}
struct OpenSelection {
    driver: Option<ResolvedDriver>,
    headless: bool,
    first_page: bool,
    now: f64,
}
impl Runtime {
    fn resolve_driver(
        &self,
        session: &Session,
        profile: &Profile,
    ) -> Result<Option<ResolvedDriver>, StartupFailed> {
        let failure = |error: io::Error| StartupFailed {
            kind: "failed".into(),
            message: error.to_string(),
        };
        session.preparation_path(&profile.id).map_err(failure)?;
        let backend = if profile.engine.is_empty() {
            "playwright"
        } else {
            &profile.engine
        };
        if let Some(registry) = &self.registry {
            return registry
                .resolve_for_launch(backend)
                .map(Some)
                .map_err(failure);
        }
        if OptionsFamily::builtin(backend).is_none() {
            return Err(failure(io::Error::new(
                io::ErrorKind::NotFound,
                "Unknown engine",
            )));
        }
        Ok(None)
    }
    pub fn available(
        &self,
        builtin_status: BTreeMap<String, bool>,
    ) -> io::Result<BTreeMap<String, bool>> {
        match &self.registry {
            Some(registry) => registry.available(&builtin_status),
            None => Ok(builtin_status),
        }
    }

    /// Start only the explicitly selected native SDK executable.
    pub(crate) fn sdk_command(&self) -> Command {
        let mut command = Command::new(&self.adapter);
        command
            .env_remove("NUMERA_UPDATE_TOKEN")
            .env_remove("NUMERA_UPDATE_TICKET")
            .env_remove("NUMERA_UPDATE_ACTIVATION");
        command
    }
    pub fn open(
        &self,
        session: Session,
        profile: Profile,
        headless: bool,
        now: f64,
    ) -> Result<Adapter, StartupFailed> {
        self.open_lifetime(session, profile, headless, false, now)
    }
    pub fn open_lifetime(
        &self,
        session: Session,
        profile: Profile,
        headless: bool,
        first_page: bool,
        now: f64,
    ) -> Result<Adapter, StartupFailed> {
        let selection = OpenSelection {
            driver: self.resolve_driver(&session, &profile)?,
            headless,
            first_page,
            now,
        };
        if let Some(deployment) = &self.deployment {
            return self.open_discovered_selected(
                session,
                profile,
                super::bundled::Discovery {
                    app: &deployment.app,
                    environment: &deployment.environment,
                    default_addons: &deployment.default_addons,
                },
                selection,
            );
        }
        self.open_packaged_selected(
            session,
            profile,
            super::bundled::Resources::default(),
            selection,
        )
    }
    pub fn open_discovered(
        &self,
        session: Session,
        profile: Profile,
        headless: bool,
        first_page: bool,
        now: f64,
        discovery: super::bundled::Discovery<'_>,
    ) -> Result<Adapter, StartupFailed> {
        let selection = OpenSelection {
            driver: self.resolve_driver(&session, &profile)?,
            headless,
            first_page,
            now,
        };
        self.open_discovered_selected(session, profile, discovery, selection)
    }
    fn open_discovered_selected(
        &self,
        session: Session,
        profile: Profile,
        discovery: super::bundled::Discovery<'_>,
        selection: OpenSelection,
    ) -> Result<Adapter, StartupFailed> {
        let failure = |error: std::io::Error| StartupFailed {
            kind: "failed".into(),
            message: error.to_string(),
        };
        let root = session
            .preparation_path(&profile.id)
            .map_err(failure)?
            .parent()
            .and_then(std::path::Path::parent)
            .ok_or_else(|| failure(std::io::Error::other("Session data root missing")))?
            .to_owned();
        let bundle = super::bundled::discover(discovery.app).map_err(failure)?;
        let engine = selection
            .driver
            .as_ref()
            .map(|driver| driver.family().options_backend())
            .unwrap_or_else(|| {
                if profile.engine.is_empty() {
                    "playwright"
                } else {
                    &profile.engine
                }
            });
        let resources = super::bundled::select(
            bundle.as_deref(),
            engine,
            &root,
            discovery.environment,
            discovery.default_addons,
        )
        .map_err(failure)?;
        self.open_packaged_selected(session, profile, resources, selection)
    }
    pub fn open_packaged(
        &self,
        session: Session,
        profile: Profile,
        headless: bool,
        first_page: bool,
        now: f64,
        resources: super::bundled::Resources,
    ) -> Result<Adapter, StartupFailed> {
        let selection = OpenSelection {
            driver: self.resolve_driver(&session, &profile)?,
            headless,
            first_page,
            now,
        };
        self.open_packaged_selected(session, profile, resources, selection)
    }
    fn open_packaged_selected(
        &self,
        session: Session,
        profile: Profile,
        resources: super::bundled::Resources,
        selection: OpenSelection,
    ) -> Result<Adapter, StartupFailed> {
        let OpenSelection {
            driver,
            headless,
            first_page,
            now,
        } = selection;
        let fail = |error: io::Error| StartupFailed {
            kind: "failed".into(),
            message: error.to_string(),
        };
        let family = driver
            .as_ref()
            .map(ResolvedDriver::family)
            .unwrap_or_else(|| {
                OptionsFamily::builtin(if profile.engine.is_empty() {
                    "playwright"
                } else {
                    &profile.engine
                })
                .expect("validated builtin")
            });
        let mut command = match &driver {
            Some(driver) => driver
                .command(&DriverContext {
                    profile: &profile,
                    adapter: &self.adapter,
                    cwd: &self.cwd,
                    home: &self.home,
                    headless,
                    first_page,
                    now,
                })
                .map_err(fail)?,
            None => self.sdk_command(),
        };
        command.env_remove("NUMERA_ADAPTER_GEOIP_DIR");
        command.envs(resources.environment);
        if let Some(geoip) = resources.geoip {
            command.env("NUMERA_ADAPTER_GEOIP_DIR", geoip);
        }
        let mut binary_options = resources.options;
        binary_options.extend(self.binary_options.clone());
        if family == OptionsFamily::Cloak
            && !command
                .get_envs()
                .any(|(key, value)| key == "CLOAKBROWSER_BINARY_PATH" && value.is_some())
        {
            let fail = |error: std::io::Error| StartupFailed {
                kind: "failed".into(),
                message: error.to_string(),
            };
            let prepared = session.preparation_path(&profile.id).map_err(fail)?;
            let root = prepared
                .parent()
                .and_then(std::path::Path::parent)
                .ok_or_else(|| fail(std::io::Error::other("Session data root is missing")))?;
            let selected = crate::cloak_acquisition::selected_binary(root).map_err(fail)?;
            command.envs(selected.environment);
            binary_options.extend(selected.options);
        }
        command
            .current_dir(&self.cwd)
            .env("NUMERA_ADAPTER_LOG_DIR", &self.logs)
            .env(
                "NUMERA_ADAPTER_FIRST_PAGE",
                if first_page { "1" } else { "0" },
            );
        let backend = if profile.engine.is_empty() {
            "playwright".into()
        } else {
            profile.engine.clone()
        };
        let country = if family != OptionsFamily::Camoufox
            && super::ip_identity::needs_lookup(&profile)
            && self.country.is_none()
        {
            profile.proxy.as_ref().and_then(|proxy| {
                crate::network::proxy_check::exit_country(proxy)
                    .ok()
                    .flatten()
            })
        } else {
            self.country.clone()
        };
        let launch = PreparedLaunch {
            backend: &backend,
            headless,
            extra_args: &[],
            cwd: &self.cwd,
            home: &self.home,
            country: country.as_deref(),
            relay: self.relay.as_deref(),
            binary_options,
        };
        if let Some(driver) = driver {
            return launch::open_registered(session, profile, &mut command, launch, now, &driver);
        }
        launch::open(session, profile, &mut command, launch, now)
    }
}
