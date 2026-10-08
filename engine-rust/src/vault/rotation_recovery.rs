use super::{
    check_password,
    completion::problems,
    durability::{end_journal, sync_config, sync_data},
    migration::Migration,
    recovery::{RecoveryError, refusal},
    transaction::commit_and_verify,
};
use serde_json::Value;
use std::path::Path;

pub(crate) fn finish(
    root: &Path,
    config: Value,
    new: &Value,
    old: &Value,
    password: Option<&str>,
    old_password: Option<&str>,
) -> Result<bool, RecoveryError> {
    let observed = problems(root, new, password)?;
    if let Some(damaged) = observed.iter().find(|p| p.code == "unreadable_files") {
        return Err(refusal(format!(
            "A master-password rotation was interrupted and {}. The rotation is not finished and its record is kept. Repair or restore those files first.",
            damaged.detail
        )));
    }
    let active = &config["vault"];
    if active["salt"] == new["salt"] && active["verifier"] == new["verifier"] && observed.is_empty()
    {
        // No descriptor rewrite: a fully committed generation only needs its
        // durable bytes and authenticated completion proved before cleanup.
        sync_data(root)?;
        sync_config(root)?;
        let persisted: Value = serde_json::from_slice(&std::fs::read(root.join("config.json"))?)
            .map_err(|_| {
                refusal("Persisted vault config cannot be read; recovery evidence retained")
            })?;
        if persisted["vault"] != *new {
            return Err(refusal(
                "Persisted descriptor disagrees with the verified generation; recovery evidence retained",
            ));
        }
        let checked = problems(root, new, password)?;
        if !checked.is_empty() {
            return Err(super::recovery::incomplete_for(
                checked,
                "master-password rotation",
            ));
        }
        end_journal(root)?;
        return Ok(true);
    }
    let (Some(password), Some(old_password)) = (
        password.filter(|p| !p.is_empty()),
        old_password.filter(|p| !p.is_empty()),
    ) else {
        let mut error = refusal(
            "A master-password rotation was interrupted, so some secrets are under the new password and some under the old one. Finish it with `numera vault resume`, which needs both passwords - they are the only way to tell the two apart, because both are ciphertext. This data root is not served until then, and the record of the new salt is kept.",
        );
        error.codes.push("needs_both_passwords");
        return Err(error);
    };
    if !check_password(
        password,
        new["salt"].as_str().unwrap_or(""),
        new["verifier"].as_str().unwrap_or(""),
    ) {
        return Err(super::recovery::locked(
            "new",
            "that is not the new master password the rotation was moving to",
        ));
    }
    if !check_password(
        old_password,
        old["salt"].as_str().unwrap_or(""),
        old["verifier"].as_str().unwrap_or(""),
    ) {
        return Err(super::recovery::locked(
            "old",
            "that is not the master password the rotation was moving away from",
        ));
    }
    let mut step = Migration::new(root.to_owned());
    let result = (|| {
        step.rekey_all(
            old_password,
            old["salt"].as_str().unwrap_or(""),
            password,
            new["salt"].as_str().unwrap_or(""),
            true,
        )
        .map_err(|error| refusal(error.to_string()))?;
        // Recovery also covers plaintext left by a prior incomplete enable.
        step.encrypt_all(password, new["salt"].as_str().unwrap_or(""))
            .map_err(|error| refusal(error.to_string()))?;
        commit_and_verify(
            root,
            &mut step,
            config,
            new,
            password,
            "master-password rotation",
        )
    })();
    if let Err(error) = result {
        let failures = step.rollback();
        if !failures.is_empty() {
            return Err(refusal(format!(
                "Vault recovery failed and {} file(s) could not be restored; journal retained",
                failures.len()
            )));
        }
        sync_data(root)?;
        sync_config(root)?;
        return Err(error);
    }
    end_journal(root)?;
    Ok(true)
}
