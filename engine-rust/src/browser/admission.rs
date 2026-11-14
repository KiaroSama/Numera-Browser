use crate::{
    conversion::value_text,
    models::Profile,
    ownership::{ready::StartupFailed, session::Session},
    storage::{
        locks::DataLock,
        repo::{ProfileRepo, RepoError},
    },
};
use std::{io, path::Path};

fn storage_failure(error: RepoError) -> StartupFailed {
    let kind = match &error {
        RepoError::Diverged(_) => "diverged",
        RepoError::VaultLocked(_) => "locked",
        RepoError::SessionBusy(_) => "busy",
        _ => "failed",
    };
    StartupFailed {
        kind: kind.into(),
        message: error.to_string(),
    }
}

/// The returned lease precedes the profile reread and must move into the
/// adapter owner. Never activate an SDK with a pre-lease profile snapshot.
pub fn current(
    root: &Path,
    id: &str,
    requested_name: &str,
    host: &str,
    password: Option<&str>,
    now: f64,
) -> Result<(Session, Profile), StartupFailed> {
    let session = Session::admit(root, id, host).map_err(|error| StartupFailed {
        kind: if error.kind() == io::ErrorKind::WouldBlock {
            "busy"
        } else {
            "failed"
        }
        .into(),
        message: error.to_string(),
    })?;
    let lock = DataLock::transaction(root)
        .map_err(RepoError::from)
        .map_err(storage_failure)?;
    let _guard = lock
        .acquire_configured()
        .map_err(RepoError::from)
        .map_err(storage_failure)?;
    let repo = ProfileRepo::new(root.into(), password.map(str::to_owned));
    let profile = repo.coherent(id, now).map_err(storage_failure)?.ok_or_else(|| StartupFailed {
        kind: "gone".into(),
        message: format!("{requested_name} was deleted while its session was being opened, so there is nothing to launch."),
    })?;
    if repo.secret_withheld(id).map_err(storage_failure)? {
        return Err(StartupFailed {
            kind: "locked".into(),
            message: format!(
                "{} has a proxy password this process cannot read, so the browser would start without it and browse from your own IP. Unlock the vault first: numera --password ... <command>",
                value_text(&profile.name)
            ),
        });
    }
    Ok((session, profile))
}
