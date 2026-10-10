use super::{adapter::Adapter, hints, ip_identity, options, resources};
use crate::{
    identity_script,
    models::Profile,
    ownership::{ready::StartupFailed, session::Session},
};
use serde_json::{Map, Value};
use std::{io, process::Command, time::Duration};

pub struct PreparedLaunch<'a> {
    pub backend: &'a str,
    pub headless: bool,
    pub extra_args: &'a [String],
    pub extensions: &'a [String],
    pub country: Option<&'a str>,
    pub relay: Option<&'a str>,
    pub binary_options: Map<String, Value>,
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
    let profile = ip_identity::resolve(&profile, launch.country).map_err(failure)?;
    if launch.backend != "camoufox" {
        resources::prepare(&session, &profile, now).map_err(failure)?;
    }
    let path = session.preparation_path(&profile.id).map_err(failure)?;
    let mut options = options::build(
        &profile,
        launch.backend,
        path.to_str()
            .ok_or_else(|| failure(io::Error::other("Profile path is not Unicode")))?,
        launch.headless,
        launch.extra_args,
        launch.extensions,
        launch.relay,
    )
    .map_err(failure)?
    .as_object()
    .cloned()
    .ok_or_else(|| failure(io::Error::other("Browser options missing")))?;
    // Only package-selected executable fields may augment application options.
    for (key, value) in launch.binary_options {
        if key != "executable_path" || !value.is_string() {
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
    if let Err(error) = adapter.open_configured(
        launch.backend,
        options,
        scripts,
        policy,
        Duration::from_secs(90),
    ) {
        adapter.finish(Duration::from_secs(10)).map_err(failure)?;
        return Err(failure(error));
    }
    Ok(adapter)
}
