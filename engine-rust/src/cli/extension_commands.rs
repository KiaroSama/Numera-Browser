use super::{
    profile_commands::{Outcome, Profiles},
    settings_commands::expand,
};
use crate::storage::{
    config::ConfigStore, extension_install, extensions, locks::DataLock, repo::RepoError,
};
use serde_json::{Map, Value, json};
use std::{io, path::Path};

impl Profiles<'_> {
    pub fn extensions(
        &self,
        verb: &str,
        reference: &str,
        path: Option<&str>,
        home: &Path,
        time: f64,
    ) -> Result<Outcome, RepoError> {
        let Some(profile) = self.repo.resolve(reference, time)? else {
            return Ok(Outcome::failed(format!(
                "no profile matching '{reference}'"
            )));
        };
        if verb == "list" {
            return Ok(Outcome::success(
                json!({"id":profile.id,"extensions":profile.extensions}),
            ));
        }
        let path = expand(
            path.ok_or_else(|| RepoError::Message("Extension path missing".into()))?,
            home,
        )?;
        if verb == "add" {
            let problems = extensions::unloadable(std::slice::from_ref(&path));
            if !problems.is_empty() {
                return Ok(Outcome::failed(problems.join("; ")));
            }
        } else if verb != "remove" {
            return Err(RepoError::Message(
                "Unknown profile extension command".into(),
            ));
        }
        let mut changed = false;
        let mut missing = false;
        let saved = self.repo.patch_many(
            &[reference.into()],
            |current| {
                if verb == "add" {
                    if !current.extensions.contains(&path) {
                        current.extensions.push(path.clone());
                        changed = true;
                    }
                } else if let Some(index) = current.extensions.iter().position(|p| p == &path) {
                    current.extensions.remove(index);
                    changed = true;
                } else {
                    missing = true;
                }
                Ok(true)
            },
            Map::new(),
            "reject",
            time,
        )?;
        let Some(profile) = saved.first() else {
            return Ok(Outcome::failed(format!(
                "no profile matching '{reference}'"
            )));
        };
        if missing {
            return Ok(Outcome::failed(format!("'{path}' isn't on this profile")));
        }
        Ok(Outcome::success(
            json!({"ok":true,"id":profile.id,"extensions":profile.extensions,"changed":changed}),
        ))
    }
}
pub fn defaults(
    root: &Path,
    verb: &str,
    path: Option<&str>,
    home: &Path,
) -> Result<Outcome, RepoError> {
    let config = ConfigStore::new(root.into());
    let lock = DataLock::transaction(root)?;
    let _guard = lock.acquire_configured()?;
    let mut current = config.default_extensions().map_err(RepoError::from)?;
    if verb == "default-list" {
        return Ok(Outcome::success(json!({"extensions":current})));
    }
    let path = expand(
        path.ok_or_else(|| RepoError::Message("Default extension path missing".into()))?,
        home,
    )?;
    let index = current
        .iter()
        .position(|p| p.as_str() == Some(path.as_str()));
    if verb == "default-add" {
        let problems = extensions::unloadable(std::slice::from_ref(&path));
        if !problems.is_empty() {
            return Ok(Outcome::failed(problems.join("; ")));
        }
        if index.is_some() {
            return Ok(Outcome::success(
                json!({"ok":true,"extensions":current,"changed":false}),
            ));
        }
        current.push(path.into());
    } else if verb == "default-remove" {
        let Some(index) = index else {
            return Ok(Outcome::failed(format!(
                "'{path}' isn't in the default set"
            )));
        };
        current.remove(index);
    } else {
        return Err(RepoError::Message(
            "Unknown default extension command".into(),
        ));
    }
    config
        .set_many(Map::from_iter([(
            "default_extensions".into(),
            json!(current),
        )]))
        .map_err(RepoError::from)?;
    Ok(Outcome::success(
        json!({"ok":true,"extensions":current,"changed":true}),
    ))
}
pub fn installed(root: &Path) -> Result<Outcome, RepoError> {
    Ok(Outcome::success(
        json!({"extensions":extensions::installed(root)?}),
    ))
}
pub fn uninstall(root: &Path, reference: &str) -> Outcome {
    match extension_install::uninstall(root, reference) {
        Ok(removed) => Outcome::success(json!({"removed":removed})),
        Err(error) => Outcome {
            exit: 1,
            payload: json!({"error":error.to_string()}),
        },
    }
}
pub fn install(
    root: &Path,
    reference: &str,
    force: bool,
    no_default: bool,
    download: impl FnOnce(&str) -> io::Result<Vec<u8>>,
) -> Result<Outcome, RepoError> {
    install_observed(root, reference, force, no_default, download, |_, _| Ok(()))
}
pub fn install_with_output(
    root: &Path,
    args: &Map<String, Value>,
    download: impl FnOnce(&str) -> io::Result<Vec<u8>>,
    output: &mut super::output::Output<impl io::Write, impl io::Write>,
) -> Result<Outcome, RepoError> {
    let reference = args
        .get("url")
        .and_then(Value::as_str)
        .ok_or_else(|| RepoError::Message("Extension reference missing".into()))?;
    let outcome = install_observed(
        root,
        reference,
        args.get("force") == Some(&Value::Bool(true)),
        args.get("no_default") == Some(&Value::Bool(true)),
        download,
        |payload, added| {
            super::extension_output::render(output, args, payload, None, root, added)?;
            Ok(())
        },
    )?;
    if outcome.payload.get("error").is_some() {
        super::extension_output::render(output, args, &outcome.payload, None, root, false)?;
    }
    Ok(outcome)
}
fn install_observed(
    root: &Path,
    reference: &str,
    force: bool,
    no_default: bool,
    download: impl FnOnce(&str) -> io::Result<Vec<u8>>,
    event: impl FnOnce(&Value, bool) -> Result<(), RepoError>,
) -> Result<Outcome, RepoError> {
    let result = match extension_install::install(root, reference, force, download) {
        Ok(result) => result,
        Err(error) => {
            return Ok(Outcome {
                exit: 1,
                payload: json!({"error":error.to_string()}),
            });
        }
    };
    let mut added_default = false;
    if !no_default {
        let lock = DataLock::transaction(root)?;
        let _guard = lock.acquire_configured()?;
        let config = ConfigStore::new(root.into());
        let mut current = config.default_extensions().map_err(RepoError::from)?;
        let path = serde_json::to_value(&result.path).map_err(io::Error::other)?;
        if !current.contains(&path) {
            current.push(path);
            config
                .set_many(Map::from_iter([(
                    "default_extensions".into(),
                    json!(current),
                )]))
                .map_err(RepoError::from)?;
            added_default = true;
        }
    }
    let payload = serde_json::to_value(result).map_err(io::Error::other)?;
    event(&payload, added_default)?;
    Ok(Outcome::success(payload))
}
