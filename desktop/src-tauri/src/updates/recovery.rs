//! Interrupted owned activation recovers only authenticated old/new file states.
use super::{
    files, install_lease, manifest,
    transaction::{self, Journal},
};
use std::{fs, io, path::Path, time::Duration};
pub fn pending(root: &Path) -> io::Result<Option<std::path::PathBuf>> {
    let mut pending = None;
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let Some(identity) = name.strip_prefix(".numera-update-") else {
            continue;
        };
        if identity == "generation.json" {
            continue;
        }
        if manifest::decode_hex::<32>(identity).is_err() {
            return Err(manifest::error(
                "Unrecognized update recovery directory identity",
            ));
        }
        if !entry.file_type()?.is_dir() || files::linked(&fs::symlink_metadata(entry.path())?) {
            return Err(manifest::error("Update recovery directory is unsafe"));
        }
        let path = entry.path().join("journal.json");
        let journal = Journal::load(&path)?;
        if matches!(
            journal.phase.as_str(),
            "quiescing"
                | "ready"
                | "activating"
                | "awaiting-ready"
                | "rollback-pending"
                | "committed-needs-resume"
        ) {
            if pending.replace(path).is_some() {
                return Err(manifest::error(
                    "Multiple unfinished updates require explicit recovery",
                ));
            }
        }
    }
    Ok(pending)
}
pub fn quiesced(journal: &Journal) -> io::Result<bool> {
    let updates = journal.data_root.join(".updates");
    match fs::symlink_metadata(&updates) {
        Ok(meta) if !meta.is_dir() || files::linked(&meta) => {
            return Err(manifest::error("Update admission directory is unsafe"))
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    }
    let bytes = match files::read(&updates.join("admission.json"), 65_536) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    let gate: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|_| manifest::error("Update admission evidence is malformed"))?;
    if gate["schema"] != 1
        || gate["state"] != "quiesced"
        || gate["ticket_sha256"] != manifest::digest(journal.ticket.as_bytes())
    {
        return Err(manifest::error(
            "Update admission belongs to another recovery transaction",
        ));
    }
    Ok(true)
}

pub fn rollback(path: &Path) -> io::Result<()> {
    let mut journal = Journal::load(path)?;
    if !matches!(
        journal.phase.as_str(),
        "quiescing"
            | "ready"
            | "activating"
            | "awaiting-ready"
            | "rollback-pending"
            | "committed-needs-resume"
    ) {
        return Err(manifest::error(
            "Only interrupted activation can be recovered",
        ));
    }
    let _transaction = files::lock(&journal.root)?;
    let exclusive = install_lease::exclusive(&journal.root, Duration::from_secs(10))?;
    let key = manifest::decode_hex::<32>(manifest::PUBLIC_KEY)?;
    let platform = manifest::platform()?;
    let commit = super::worker::committed(&journal)?;
    if journal.phase == "committed-needs-resume" && commit.is_none() {
        return Err(manifest::error(
            "Committed generation evidence is missing; rollback refused",
        ));
    }
    if let Some(identity) = commit {
        let installed = transaction::installed(&journal.root, &key, platform)?;
        transaction::validate_files(&journal.root, &installed)?;
        if identity != (installed.source_sha.clone(), installed.generation) {
            return Err(manifest::error(
                "Live files no longer match the committed generation",
            ));
        }
        install_lease::mark(&journal.root, &journal.nonce)?;
        drop(exclusive);
        super::worker::restart_recovered(&journal, &installed.source_sha)?;
        install_lease::clear(&journal.root, &journal.nonce)?;
        journal.phase = "complete".into();
        journal.save()?;
        return Ok(());
    }
    if journal.phase == "quiescing" && !quiesced(&journal)? {
        journal.phase = "cancelled".into();
        journal.ticket.clear();
        journal.save()?;
        return Ok(());
    }
    if matches!(journal.phase.as_str(), "ready" | "quiescing") {
        let old = transaction::installed(&journal.root, &key, platform)?;
        transaction::validate_files(&journal.root, &old)?;
        install_lease::mark(&journal.root, &journal.nonce)?;
        drop(exclusive);
        super::worker::restart_recovered(&journal, &old.source_sha)?;
        install_lease::clear(&journal.root, &journal.nonce)?;
        journal.phase = "rolled-back".into();
        journal.save()?;
        return Ok(());
    }
    let old_bytes = files::read(
        &journal.directory.join("old-BUILD.json"),
        manifest::BUILD_LIMIT,
    )?;
    let old_signature = files::read(&journal.directory.join("old-BUILD.sig"), 64)?;
    let old = manifest::signed_build(&old_bytes, &old_signature, &key, platform)?;
    let new_bytes = files::read(
        &journal.directory.join("new-BUILD.json"),
        manifest::BUILD_LIMIT,
    )?;
    let new_signature = files::read(&journal.directory.join("new-BUILD.sig"), 64)?;
    let next = manifest::signed_build(&new_bytes, &new_signature, &key, platform)?;
    install_lease::mark(&journal.root, &journal.nonce)?;
    transaction::rollback(&mut journal, &old, &next)?;
    drop(exclusive);
    super::worker::restart_recovered(&journal, &old.source_sha)?;
    install_lease::clear(&journal.root, &journal.nonce)?;
    journal.phase = "rolled-back".into();
    journal.save()?;
    Ok(())
}
