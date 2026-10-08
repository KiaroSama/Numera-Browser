use super::{
    check_password,
    completion::{Problem, problems},
    durability::{sync_data, sync_dir, sync_file},
    journal::{JournalState, classify, descriptors},
    migration::Migration,
    transaction::commit_and_verify,
};
use crate::storage::locks::DataLock;
use serde_json::Value;
use std::{error::Error, fmt, fs, io, path::Path, time::Duration};

#[derive(Debug)]
pub struct RecoveryError {
    pub codes: Vec<&'static str>,
    pub role: Option<&'static str>,
    message: String,
}
impl fmt::Display for RecoveryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl Error for RecoveryError {}
impl From<io::Error> for RecoveryError {
    fn from(e: io::Error) -> Self {
        Self {
            codes: vec!["unreadable_files"],
            role: None,
            message: format!("Vault recovery file operation failed; evidence retained: {e}"),
        }
    }
}
pub(crate) fn refusal(message: impl Into<String>) -> RecoveryError {
    RecoveryError {
        codes: Vec::new(),
        role: None,
        message: message.into(),
    }
}
pub(crate) fn locked(role: &'static str, message: &str) -> RecoveryError {
    let mut error = refusal(message);
    error.role = Some(role);
    error
}
pub(crate) fn incomplete(found: Vec<Problem>) -> RecoveryError {
    incomplete_for(found, "`vault enable`")
}
pub(crate) fn incomplete_for(found: Vec<Problem>, what: &str) -> RecoveryError {
    let mut codes: Vec<_> = found.iter().map(|p| p.code).collect();
    codes.sort();
    codes.dedup();
    let fixable = codes.iter().any(|code| {
        [
            "no_password",
            "needs_both_passwords",
            "verifier_mismatch",
            "wrong_generation",
        ]
        .contains(code)
    });
    let remedy = if fixable {
        "Open the store with the master password that migration was using (NUMERA_PASSWORD, or --password), or restore the data directory from a backup."
    } else {
        "Repair or restore the files named above, then open the store again."
    };
    RecoveryError {
        codes,
        role: None,
        message: format!(
            "A {what} cannot be completed against this data root: {}. Nothing has been committed and vault-migration.json is kept - it holds the descriptor those secrets belong to. {remedy}",
            found
                .into_iter()
                .map(|p| p.detail)
                .collect::<Vec<_>>()
                .join("; ")
        ),
    }
}
pub fn recover_enable(root: &Path, password: Option<&str>) -> Result<bool, RecoveryError> {
    recover(root, password, None)
}

pub fn needs_recovery(root: &Path) -> Result<bool, RecoveryError> {
    let (state, record) = classify(root);
    if state == JournalState::Absent {
        return Ok(false);
    }
    if state == JournalState::Valid
        && let Some(record) = record
    {
        return Ok(!owner_live(&record)?);
    }
    Ok(true)
}

fn owner_live(record: &Value) -> Result<bool, RecoveryError> {
    let raw = &record["pid"];
    if !crate::conversion::truth(raw) {
        return Ok(false);
    }
    let pid = raw
        .as_u64()
        .and_then(|pid| u32::try_from(pid).ok())
        .ok_or_else(|| {
            refusal("Vault migration owner PID is invalid; recovery evidence retained")
        })?;
    if pid == std::process::id() {
        return Ok(false);
    }
    let token = if record["token"].is_null() {
        None
    } else {
        Some(record["token"].as_u64().ok_or_else(|| {
            refusal("Vault migration owner token is invalid; recovery evidence retained")
        })?)
    };
    crate::ownership::identity::alive(pid, token).map_err(|_| {
        refusal("Vault migration owner identity cannot be established; recovery evidence retained")
    })
}

/// Settle an abandoned journal; unknown owner identity retains the evidence.
pub fn recover(
    root: &Path,
    password: Option<&str>,
    old_password: Option<&str>,
) -> Result<bool, RecoveryError> {
    let (state, record) = classify(root);
    if state == JournalState::Absent {
        return Ok(false);
    }
    let record=record.ok_or_else(||refusal("vault-migration.json exists but is not a usable recovery record; nothing has been changed or deleted"))?;
    if state != JournalState::Valid {
        return Err(refusal(
            "vault-migration.json is unreadable or unsupported; recovery evidence is retained",
        ));
    }
    if owner_live(&record)? {
        return Ok(false);
    }
    let lock = DataLock::transaction(root)?;
    let _root = lock.acquire(Duration::from_secs(30))?;
    let (_, current_record) = classify(root);
    if current_record.as_ref() != Some(&record) {
        return Err(refusal(
            "Vault journal changed before recovery acquired its transaction; retry without discarding evidence",
        ));
    }
    let (new, old) = descriptors(&record);
    let rotation = record.get("op").and_then(Value::as_str) == Some("rotate");
    let config_path = root.join("config.json");
    let config: Value = match fs::read(&config_path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|_| {
            refusal("The active config cannot be read; recovery evidence is retained.")
        })?,
        Err(e) if e.kind() == io::ErrorKind::NotFound => serde_json::json!({}),
        Err(e) => return Err(e.into()),
    };
    if !config.is_object() {
        return Err(incomplete_for(
            vec![Problem {
                code: "descriptor_unusable",
                detail: "the active config is not a settings object".into(),
            }],
            "migration",
        ));
    }
    let active = &config["vault"];
    let same = |value: &Value, target: &Value| {
        value["salt"] == target["salt"] && value["verifier"] == target["verifier"]
    };
    if rotation || !active.is_null() {
        if !super::journal::usable_descriptor(active) {
            return Err(incomplete_for(
                vec![Problem {
                    code: "descriptor_unusable",
                    detail: "the active config has no usable vault descriptor".into(),
                }],
                "migration",
            ));
        }
        if !same(active, &new) && !(rotation && same(active, &old)) {
            return Err(incomplete_for(vec![Problem{code:"descriptor_disagreement",detail:"the active descriptor is neither an allowed starting generation nor the journal's target; neither record has been discarded".into()}], "migration"));
        }
    }
    if rotation {
        return super::rotation_recovery::finish(root, config, &new, &old, password, old_password);
    }
    let observed = problems(root, &new, password)?;
    let blocked: Vec<_> = observed
        .iter()
        .filter(|p| ["descriptor_unusable", "unreadable_files"].contains(&p.code))
        .map(|p| Problem {
            code: p.code,
            detail: p.detail.clone(),
        })
        .collect();
    if !blocked.is_empty() {
        return Err(incomplete(blocked));
    }
    let leftover = observed.iter().find(|p| p.code == "plaintext_left");
    if let Some(leftover) = leftover {
        let count = leftover
            .detail
            .split_whitespace()
            .next()
            .unwrap_or("unknown");
        if password.is_none_or(str::is_empty) {
            return Err(refusal(format!(
                "A `vault enable` was interrupted with {count} credential(s) still unencrypted, so this data root is half migrated. Open it with the master password that migration was using (NUMERA_PASSWORD, or `--password`) and it will be finished. Nothing is lost; the vault is not on until it is complete."
            )));
        }
        if !check_password(
            password.unwrap_or(""),
            new["salt"].as_str().unwrap_or(""),
            new["verifier"].as_str().unwrap_or(""),
        ) {
            return Err(refusal(
                "that is not the master password the interrupted migration was using, so it cannot be finished with it",
            ));
        }
    } else if !observed.is_empty() {
        return Err(incomplete(observed));
    }
    let mut step = Migration::new(root.to_owned());
    let result = (|| -> Result<(), RecoveryError> {
        if leftover.is_some() {
            step.encrypt_all(password.unwrap_or(""), new["salt"].as_str().unwrap_or(""))
                .map_err(|e| refusal(e.to_string()))?;
        }
        commit_and_verify(
            root,
            &mut step,
            config,
            &new,
            password.unwrap_or(""),
            "`vault enable`",
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
        if config_path.exists() {
            sync_file(&config_path)?;
        }
        sync_dir(root)?;
        return Err(error);
    }
    fs::remove_file(root.join("vault-migration.json"))?;
    sync_dir(root)?;
    Ok(true)
}
