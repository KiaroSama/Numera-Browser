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
    let failure = |error: io::Error| StartupFailed {
        kind: "failed".into(),
        message: error.to_string(),
    };
    if !["playwright", "patchright", "camoufox", "cloak"].contains(&launch.backend) {
        return Err(failure(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Unknown browser backend",
        )));
    }
    let profile = if launch.backend == "camoufox" {
        profile
    } else {
        ip_identity::resolve(&profile, launch.country).map_err(failure)?
    };
    if launch.backend != "camoufox" {
        resources::prepare(&session, &profile, now).map_err(failure)?;
    }
    let path = session.preparation_path(&profile.id).map_err(failure)?;
    let extensions = if launch.backend == "camoufox" {
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
    let relay_url = relay.as_ref().map(crate::network::relay::Relay::url);
    let mut options = options::build(
        &profile,
        launch.backend,
        path.to_str()
            .ok_or_else(|| failure(io::Error::other("Profile path is not Unicode")))?,
        launch.headless,
        launch.extra_args,
        &extensions,
        launch.relay.or(relay_url.as_deref()),
    )
    .map_err(failure)?
    .as_object()
    .cloned()
    .ok_or_else(|| failure(io::Error::other("Browser options missing")))?;
    // Only package-selected executable fields may augment application options.
    for (key, value) in launch.binary_options {
        let valid = match (launch.backend, key.as_str()) {
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
    let chromium = matches!(launch.backend, "playwright" | "patchright");
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
    if let Err(error) = adapter.open_configured(
        launch.backend,
        options,
        scripts,
        policy,
        Duration::from_secs(90),
    ) {
        return Err(failed_startup(&mut adapter, error));
    }
    if let Err(error) = startup::navigate(&mut adapter, &profile.startup_urls) {
        return Err(failed_startup(&mut adapter, error));
    }
    Ok(adapter)
}
