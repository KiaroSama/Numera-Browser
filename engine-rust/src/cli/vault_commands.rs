use super::profile_commands::Outcome;
use crate::{
    conversion::truth,
    storage::{
        config::ConfigStore,
        repo::{ProfileRepo, RepoError},
    },
    vault,
};
use serde_json::json;
use std::path::Path;

pub fn run(
    root: &Path,
    verb: &str,
    password: Option<&str>,
    new_password: Option<&str>,
) -> Result<Outcome, RepoError> {
    ProfileRepo::new(root.into(), password.map(str::to_owned)).require_settled()?;
    let config = ConfigStore::new(root.into()).read();
    let descriptor = config.get("vault");
    let enabled = descriptor.is_some_and(|v| !v.is_null());
    match verb {
        "status" => {
            let unlocked = descriptor.filter(|v| truth(v)).is_none_or(|v| {
                password.filter(|p| !p.is_empty()).is_some_and(|p| {
                    vault::check_password(
                        p,
                        v["salt"].as_str().unwrap_or(""),
                        v["verifier"].as_str().unwrap_or(""),
                    )
                })
            });
            Ok(Outcome::success(
                json!({"enabled":enabled,"unlocked":if enabled{Some(unlocked)}else{None},"backend":true}),
            ))
        }
        "enable" => {
            let password = required_password(password)?;
            let count = vault::transaction::enable(root, password).map_err(RepoError::Recovery)?;
            Ok(Outcome::success(
                json!({"ok":true,"enabled":true,"encrypted":count}),
            ))
        }
        "change" => {
            let old = required_password(password)?;
            let new=new_password.filter(|p|!p.is_empty()).ok_or_else(||RepoError::Message("this needs the new master password - pass --new-password (a --json run never prompts)".into()))?;
            match vault::transaction::rotate(root, old, new) {
                Ok(count) => Ok(Outcome::success(json!({"ok":true,"rotated":count}))),
                Err(error) => Ok(Outcome::failed(error.to_string())),
            }
        }
        // Resume is performed by store-open with both passwords before dispatch.
        "resume" => Ok(Outcome::success(
            json!({"ok":true,"resumed":true,"enabled":enabled}),
        )),
        _ => Ok(Outcome::failed(format!("unknown vault command '{verb}'"))),
    }
}
fn required_password(password: Option<&str>) -> Result<&str, RepoError> {
    password.filter(|p|!p.is_empty()).ok_or_else(||RepoError::Message("this needs a master password — pass --password or set NUMERA_PASSWORD (a --json run never prompts)".into()))
}
