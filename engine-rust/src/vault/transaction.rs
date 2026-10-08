use super::{
    check_password,
    completion::problems,
    durability::{end_journal, sync_config, sync_data, sync_dir, sync_file},
    encrypt,
    journal::JournalState,
    migration::Migration,
    recovery::{RecoveryError, incomplete_for, refusal},
};
use crate::storage::{atomic::write_json, locks::DataLock};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::{
    fs, io,
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

fn read_config(root: &Path) -> Result<Value, RecoveryError> {
    match fs::read(root.join("config.json")) {
        Ok(bytes) => serde_json::from_slice::<Value>(&bytes).map_err(|_| {
            refusal("The active config cannot be read; recovery evidence is retained")
        }),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(json!({})),
        Err(error) => Err(error.into()),
    }
    .and_then(|value| {
        if value.is_object() {
            Ok(value)
        } else {
            Err(refusal(
                "The active config is not a settings object; nothing was changed",
            ))
        }
    })
}

fn new_descriptor(password: &str) -> Result<Value, RecoveryError> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes)
        .map_err(|error| refusal(format!("Vault salt randomness unavailable: {error}")))?;
    let salt = STANDARD.encode(bytes);
    let verifier =
        encrypt(password, &salt, "numera-vault-ok").map_err(|error| refusal(error.to_string()))?;
    Ok(json!({"salt": salt, "verifier": verifier}))
}

fn begin(root: &Path, new: &Value, old: Option<&Value>) -> Result<(), RecoveryError> {
    let pid = std::process::id();
    let token = crate::ownership::identity::started_at(pid)?.ok_or_else(|| {
        refusal("Vault migration owner identity unavailable; nothing was written")
    })?;
    let record = json!({"version":1,"op":if old.is_some(){"rotate"}else{"enable"},"salt":new["salt"],"verifier":new["verifier"],"new":new,"old":old,
        "started":SystemTime::now().duration_since(UNIX_EPOCH).map_err(io::Error::other)?.as_secs_f64(),"pid":pid,"token":token});
    write_json(&root.join("vault-migration.json"), &record)?;
    sync_file(&root.join("vault-migration.json"))?;
    sync_dir(root)?;
    Ok(())
}

pub(crate) fn commit_and_verify(
    root: &Path,
    step: &mut Migration,
    mut config: Value,
    descriptor: &Value,
    password: &str,
    what: &str,
) -> Result<(), RecoveryError> {
    sync_data(root)?;
    step.remember_config()?;
    config["vault"] = descriptor.clone();
    let path = root.join("config.json");
    let lock = DataLock::for_file(&path)?;
    let _guard = lock.acquire(Duration::from_secs(30))?;
    write_json(&path, &config)?;
    sync_config(root)?;
    let persisted = read_config(root)?;
    if persisted["vault"] != *descriptor {
        return Err(refusal(
            "Persisted descriptor disagrees with the verified generation; recovery evidence retained",
        ));
    }
    let found = problems(root, descriptor, Some(password))?;
    if !found.is_empty() {
        return Err(incomplete_for(found, what));
    }
    Ok(())
}

/// Ordinary enable publishes a journal before any credential mutation. Failure
/// restores exact bytes and syncs the restored generation before journal removal.
pub fn enable(root: &Path, password: &str) -> Result<usize, RecoveryError> {
    let lock = DataLock::transaction(root)?;
    let _guard = lock.acquire(Duration::from_secs(30))?;
    if super::journal::classify(root).0 != JournalState::Absent {
        return Err(refusal(
            "An unresolved vault migration must be recovered before enabling a vault",
        ));
    }
    let config = read_config(root)?;
    if crate::conversion::truth(&config["vault"]) {
        return Err(refusal(
            "A vault is already set up. Use `numera vault change` to rotate the master password.",
        ));
    }
    let descriptor = new_descriptor(password)?;
    begin(root, &descriptor, None)?;
    let mut step = Migration::new(root.to_owned());
    let result = (|| {
        let count = step
            .encrypt_all(password, descriptor["salt"].as_str().unwrap_or(""))
            .map_err(|error| refusal(error.to_string()))?;
        commit_and_verify(
            root,
            &mut step,
            config,
            &descriptor,
            password,
            "`vault enable`",
        )?;
        Ok(count)
    })();
    finish(root, &mut step, result)
}

fn finish(
    root: &Path,
    step: &mut Migration,
    result: Result<usize, RecoveryError>,
) -> Result<usize, RecoveryError> {
    if let Err(error) = result {
        let failures = step.rollback();
        if !failures.is_empty() {
            return Err(refusal(format!(
                "Vault migration failed and {} file(s) could not be restored; journal retained",
                failures.len()
            )));
        }
        sync_data(root)?;
        sync_config(root)?;
        end_journal(root)?;
        return Err(error);
    }
    end_journal(root)?;
    result
}

pub fn rotate(root: &Path, old_password: &str, password: &str) -> Result<usize, RecoveryError> {
    if password.is_empty() {
        return Err(refusal("the new master password cannot be empty"));
    }
    let lock = DataLock::transaction(root)?;
    let _guard = lock.acquire(Duration::from_secs(30))?;
    if super::journal::classify(root).0 != JournalState::Absent {
        return Err(refusal(
            "An unresolved vault migration must be recovered before rotating a vault",
        ));
    }
    let config = read_config(root)?;
    let old = config["vault"].clone();
    if !crate::conversion::truth(&old) {
        return Err(refusal(
            "There is no vault to rotate. Use `numera vault enable` first.",
        ));
    }
    if !check_password(
        old_password,
        old["salt"].as_str().unwrap_or(""),
        old["verifier"].as_str().unwrap_or(""),
    ) {
        return Err(refusal("that is not the current master password"));
    }
    let descriptor = new_descriptor(password)?;
    begin(root, &descriptor, Some(&old))?;
    let mut step = Migration::new(root.to_owned());
    let result = (|| {
        let count = step
            .rekey_all(
                old_password,
                old["salt"].as_str().unwrap_or(""),
                password,
                descriptor["salt"].as_str().unwrap_or(""),
                false,
            )
            .map_err(|error| refusal(error.to_string()))?;
        commit_and_verify(
            root,
            &mut step,
            config,
            &descriptor,
            password,
            "master-password rotation",
        )?;
        Ok(count)
    })();
    finish(root, &mut step, result)
}
