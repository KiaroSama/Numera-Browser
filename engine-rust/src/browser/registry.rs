//! Explicit embedding-owned browser registrations. No profile or environment
//! input can manufacture a custom backend capability.
use crate::models::Profile;
use std::{
    collections::BTreeMap,
    io,
    path::Path,
    process::Command,
    sync::{Arc, RwLock},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptionsFamily {
    Chromium,
    Camoufox,
    Cloak,
}
impl OptionsFamily {
    pub fn builtin(name: &str) -> Option<Self> {
        match name {
            "playwright" | "patchright" => Some(Self::Chromium),
            "camoufox" => Some(Self::Camoufox),
            "cloak" => Some(Self::Cloak),
            _ => None,
        }
    }
    pub(crate) fn options_backend(self) -> &'static str {
        match self {
            Self::Chromium => "playwright",
            Self::Camoufox => "camoufox",
            Self::Cloak => "cloak",
        }
    }
}
/// Available only after the native caller admitted this session and reread its
/// profile. Factories return a Command; the controller creates its owned tree.
pub struct DriverContext<'a> {
    pub profile: &'a Profile,
    pub adapter: &'a Path,
    pub cwd: &'a Path,
    pub home: &'a Path,
    pub headless: bool,
    pub first_page: bool,
    pub now: f64,
}
type Factory = Arc<dyn Fn(&DriverContext<'_>) -> io::Result<Command> + Send + Sync>;
type Probe = Arc<dyn Fn() -> io::Result<bool> + Send + Sync>;
#[derive(Clone)]
pub struct DriverRegistration {
    wire_backend: String,
    family: OptionsFamily,
    requires: Option<String>,
    factory: Option<Factory>,
    available: Option<Probe>,
}
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}
impl DriverRegistration {
    pub fn native(
        wire_backend: impl Into<String>,
        family: OptionsFamily,
        requires: Option<String>,
        factory: impl Fn(&DriverContext<'_>) -> io::Result<Command> + Send + Sync + 'static,
        available: impl Fn() -> io::Result<bool> + Send + Sync + 'static,
    ) -> io::Result<Self> {
        let wire_backend = wire_backend.into();
        if !valid_name(&wire_backend)
            || requires.as_ref().is_some_and(|label| {
                label.is_empty() || label.len() > 256 || label.chars().any(char::is_control)
            })
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid native browser registration",
            ));
        }
        Ok(Self {
            wire_backend,
            family,
            requires,
            factory: Some(Arc::new(factory)),
            available: Some(Arc::new(available)),
        })
    }
    fn builtin(name: &str) -> Self {
        Self {
            wire_backend: name.into(),
            family: OptionsFamily::builtin(name).expect("builtin family"),
            requires: Some(
                if name == "cloak" {
                    "cloakbrowser"
                } else {
                    name
                }
                .into(),
            ),
            factory: None,
            available: None,
        }
    }
    pub fn wire_backend(&self) -> &str {
        &self.wire_backend
    }
    pub fn family(&self) -> OptionsFamily {
        self.family
    }
    /// Prepare an inert native command. Runtime always spawns it through the
    /// admitted Session, restricts credentials, and checks hello/open itself.
    pub fn command(&self, context: &DriverContext<'_>) -> io::Result<Command> {
        match &self.factory {
            Some(factory) => factory(context).map_err(|error| {
                io::Error::new(
                    error.kind(),
                    "Registered browser command preparation failed",
                )
            }),
            None => super::runtime::selected_sdk_command(context.adapter),
        }
    }
    fn installed(&self, name: &str, builtin_status: &BTreeMap<String, bool>) -> io::Result<bool> {
        match &self.available {
            Some(probe) => probe().map_err(|error| {
                io::Error::new(error.kind(), "Registered browser availability probe failed")
            }),
            None => Ok(builtin_status.get(name).copied().unwrap_or(false)),
        }
    }
    fn unavailable(&self, name: &str) -> io::Error {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "The '{name}' engine needs {}, which is not available. Provide its native dependency, then try again.",
                self.requires
                    .as_deref()
                    .unwrap_or("its registered native SDK")
            ),
        )
    }
}
/// A resolved registration is an immutable capability, not a deserializable
/// allowlist. An override cannot change a launch already holding its snapshot.
#[derive(Clone)]
pub struct ResolvedDriver {
    name: String,
    registration: DriverRegistration,
}
impl ResolvedDriver {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn wire_backend(&self) -> &str {
        self.registration.wire_backend()
    }
    pub fn family(&self) -> OptionsFamily {
        self.registration.family()
    }
    pub(crate) fn command(&self, context: &DriverContext<'_>) -> io::Result<Command> {
        self.registration.command(context)
    }
}
pub struct DriverRegistry {
    entries: RwLock<BTreeMap<String, DriverRegistration>>,
}
fn poisoned() -> io::Error {
    io::Error::other("Browser driver registry is poisoned")
}
impl Default for DriverRegistry {
    fn default() -> Self {
        Self {
            entries: RwLock::new(
                ["playwright", "patchright", "camoufox", "cloak"]
                    .into_iter()
                    .map(|name| (name.into(), DriverRegistration::builtin(name)))
                    .collect(),
            ),
        }
    }
}
impl DriverRegistry {
    /// Last registration wins. Callbacks are retained by immutable snapshots;
    /// no registry lock is held while user code runs.
    pub fn register(
        &self,
        name: impl Into<String>,
        registration: DriverRegistration,
    ) -> io::Result<Option<DriverRegistration>> {
        let name = name.into();
        if !valid_name(&name) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid browser registration name",
            ));
        }
        Ok(self
            .entries
            .write()
            .map_err(|_| poisoned())?
            .insert(name, registration))
    }
    pub fn names(&self) -> io::Result<Vec<String>> {
        Ok(self
            .entries
            .read()
            .map_err(|_| poisoned())?
            .keys()
            .cloned()
            .collect())
    }
    fn snapshot(&self, name: &str) -> io::Result<ResolvedDriver> {
        let entries = self.entries.read().map_err(|_| poisoned())?;
        let registration = entries.get(name).cloned().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "Unknown engine '{name}'. Known engines: {}.",
                    entries
                        .keys()
                        .map(String::as_str)
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            )
        })?;
        Ok(ResolvedDriver {
            name: name.into(),
            registration,
        })
    }
    /// Builtin status is the selected worker's metadata, not a global SDK guess.
    pub fn get(
        &self,
        name: &str,
        builtin_status: &BTreeMap<String, bool>,
    ) -> io::Result<ResolvedDriver> {
        let driver = self.snapshot(name)?;
        if !driver.registration.installed(name, builtin_status)? {
            return Err(driver.registration.unavailable(name));
        }
        Ok(driver)
    }
    pub fn available(
        &self,
        builtin_status: &BTreeMap<String, bool>,
    ) -> io::Result<BTreeMap<String, bool>> {
        let entries = self.entries.read().map_err(|_| poisoned())?.clone();
        entries
            .into_iter()
            .map(|(name, registration)| {
                registration
                    .installed(&name, builtin_status)
                    .map(|installed| (name, installed))
            })
            .collect()
    }
    pub(crate) fn resolve_for_launch(&self, name: &str) -> io::Result<ResolvedDriver> {
        let driver = self.snapshot(name)?;
        if let Some(probe) = &driver.registration.available {
            let installed = probe().map_err(|error| {
                io::Error::new(error.kind(), "Registered browser availability probe failed")
            })?;
            if !installed {
                return Err(driver.registration.unavailable(name));
            }
        } // Builtin availability is checked by the mandatory owned hello/open.
        Ok(driver)
    }
}
