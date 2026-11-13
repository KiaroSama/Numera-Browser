use super::{adapter::Adapter, extensions, hints, ip_identity, options, resources, startup};
use crate::{
    identity_script,
    models::Profile,
    ownership::{ready::StartupFailed, session::Session},
};
use serde_json::{Map, Value};
use std::{io, path::Path, process::Command, time::Duration};

pub struct PreparedLaunch<'a> {
    pub backend: &'a str,
    pub headless: bool,
    pub extra_args: &'a [String],
    pub cwd: &'a Path,
    pub home: &'a Path,
    pub country: Option<&'a str>,
    pub relay: Option<&'a str>,
    pub binary_options: Map<String, Value>,
}

fn failed_startup(adapter: &mut Adapter, error: io::Error) -> StartupFailed {
    match adapter.finish(Duration::from_secs(10)) {
        Ok(()) => StartupFailed {
            kind: "failed".into(),
            message: error.to_string(),
        },
        Err(cleanup) => StartupFailed {
            kind: "unsettled".into(),
            message: format!("{error}; browser startup cleanup failed: {cleanup}"),
        },
    }
}

/// Compose native decisions after admission. Network/resource discovery must
/// supply actual observations, never move into the SDK adapter as fallbacks.
pub fn open(
    session: Session,
    profile: Profile,
    command: &mut Command,
    launch: PreparedLaunch<'_>,
    now: f64,
) -> Result<Adapter, StartupFailed> {
    open_selected(session, profile, command, launch, now, None)
}
pub(crate) fn open_registered(
    session: Session,
    profile: Profile,
    command: &mut Command,
    launch: PreparedLaunch<'_>,
    now: f64,
    driver: &super::registry::ResolvedDriver,
) -> Result<Adapter, StartupFailed> {
    if driver.name() != launch.backend {
        return Err(StartupFailed {
            kind: "failed".into(),
            message: "Registered browser capability does not match the requested engine".into(),
        });
    }
    open_selected(session, profile, command, launch, now, Some(driver))
}
fn open_selected(
    session: Session,
    profile: Profile,
    command: &mut Command,
    launch: PreparedLaunch<'_>,
    now: f64,
    driver: Option<&super::registry::ResolvedDriver>,
) -> Result<Adapter, StartupFailed> {
    let failure = |error: io::Error| StartupFailed {
        kind: "failed".into(),
        message: error.to_string(),
    };
    let family = driver
        .map(|driver| driver.family())
        .or_else(|| super::registry::OptionsFamily::builtin(launch.backend))
        .ok_or_else(|| {
            failure(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Unknown browser backend",
            ))
        })?;
    let backend = family.options_backend();
    let profile = if backend == "camoufox" {
        profile
    } else {
        ip_identity::resolve(&profile, launch.country).map_err(failure)?
    };
    if backend != "camoufox" {
        resources::prepare(&session, &profile, now).map_err(failure)?;
    }
    let path = session.preparation_path(&profile.id).map_err(failure)?;
    let extensions = if backend == "camoufox" {
        Vec::new()
    } else {
        extensions::paths(&profile.extensions, launch.cwd, launch.home).map_err(failure)?
    };
    let relay = if launch.relay.is_none() {
        crate::network::relay::proxy_transport(profile.proxy.as_ref())
            .map_err(failure)?
            .1
    } else {
        None
    };
    let relay_url = relay
        .as_ref()
        .map(crate::network::relay::Relay::url)
        .transpose()
        .map_err(failure)?;
    let user_data_dir = path
        .to_str()
        .ok_or_else(|| failure(io::Error::other("Profile path is not Unicode")))?;
    let prepared_options = match driver {
        Some(driver) => options::build_registered(
            &profile,
            driver,
            user_data_dir,
            launch.headless,
            launch.extra_args,
            &extensions,
            launch.relay.or(relay_url.as_deref()),
        ),
        None => options::build(
            &profile,
            launch.backend,
            user_data_dir,
            launch.headless,
            launch.extra_args,
            &extensions,
            launch.relay.or(relay_url.as_deref()),
        ),
    };
    let mut options = prepared_options
        .map_err(failure)?
        .as_object()
        .cloned()
        .ok_or_else(|| failure(io::Error::other("Browser options missing")))?;
    // Only package-selected executable fields may augment application options.
    for (key, value) in launch.binary_options {
        let valid = match (backend, key.as_str()) {
            (_, "executable_path") => value.is_string(),
            ("cloak", "browser_version") | ("camoufox", "ff_version") => value.is_string(),
            ("camoufox", "addons" | "exclude_addons") => value
                .as_array()
                .is_some_and(|values| values.iter().all(Value::is_string)),
            _ => false,
        };
        if !valid {
            return Err(failure(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid packaged browser option",
            )));
        }
        options.insert(key, value);
    }
    let chromium = matches!(backend, "playwright" | "patchright");
    let scripts = if chromium {
        vec![
            identity_script::build(&profile.fingerprint)
                .map_err(io::Error::other)
                .map_err(failure)?,
        ]
    } else {
        Vec::new()
    };
    let policy = if chromium {
        Some(
            hints::policy(&profile.fingerprint)
                .map_err(io::Error::other)
                .map_err(failure)?,
        )
    } else {
        None
    };
    let mut adapter =
        Adapter::spawn_admitted(session, command, Duration::from_secs(10)).map_err(failure)?;
    if let Some(relay) = relay {
        adapter.own_relay(relay).map_err(failure)?;
    }
    let opened = match driver {
        Some(driver) => {
            adapter.open_registered(driver, options, scripts, policy, Duration::from_secs(90))
        }
        None => adapter.open_configured(
            launch.backend,
            options,
            scripts,
            policy,
            Duration::from_secs(90),
        ),
    };
    if let Err(error) = opened {
        return Err(failed_startup(&mut adapter, error));
    }
    if let Err(error) = startup::navigate(&mut adapter, &profile.startup_urls) {
        return Err(failed_startup(&mut adapter, error));
    }
    Ok(adapter)
}
