use super::{
    adapter::Adapter,
    launch::{self, PreparedLaunch},
};
use crate::{
    models::Profile,
    ownership::{ready::StartupFailed, session::Session},
};
use serde_json::{Map, Value};
use std::{path::PathBuf, process::Command};

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
}
impl Runtime {
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
        if let Some(deployment) = &self.deployment {
            return self.open_discovered(
                session,
                profile,
                headless,
                first_page,
                now,
                super::bundled::Discovery {
                    app: &deployment.app,
                    environment: &deployment.environment,
                    default_addons: &deployment.default_addons,
                },
            );
        }
        self.open_packaged(
            session,
            profile,
            headless,
            first_page,
            now,
            super::bundled::Resources::default(),
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
        let engine = if profile.engine.is_empty() {
            "playwright"
        } else {
            &profile.engine
        };
        let resources = super::bundled::select(
            bundle.as_deref(),
            engine,
            &root,
            discovery.environment,
            discovery.default_addons,
        )
        .map_err(failure)?;
        self.open_packaged(session, profile, headless, first_page, now, resources)
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
        let mut command = self.sdk_command();
        command.env_remove("NUMERA_ADAPTER_GEOIP_DIR");
        command.envs(resources.environment);
        if let Some(geoip) = resources.geoip {
            command.env("NUMERA_ADAPTER_GEOIP_DIR", geoip);
        }
        let mut binary_options = resources.options;
        binary_options.extend(self.binary_options.clone());
        if profile.engine == "cloak"
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
        let country = if backend != "camoufox"
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
        launch::open(
            session,
            profile,
            &mut command,
            PreparedLaunch {
                backend: &backend,
                headless,
                extra_args: &[],
                cwd: &self.cwd,
                home: &self.home,
                country: country.as_deref(),
                relay: self.relay.as_deref(),
                binary_options,
            },
            now,
        )
    }
}
