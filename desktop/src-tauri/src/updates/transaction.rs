use super::{
    files,
    manifest::{self, Build, InstalledFile},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self},
    io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Journal {
    pub schema: u32,
    pub root: PathBuf,
    pub directory: PathBuf,
    pub manifest_digest: String,
    pub nonce: String,
    pub phase: String,
    pub ticket: String,
    pub data_root: PathBuf,
    pub moved: Vec<String>,
    pub installed: Vec<String>,
    pub pending: Option<String>,
    pub started: u64,
}
pub fn now() -> io::Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_secs())
}
pub fn installed(root: &Path, key: &[u8; 32], platform: &str) -> io::Result<Build> {
    manifest::signed_build(
        &files::read(&root.join("BUILD.json"), manifest::BUILD_LIMIT)?,
        &files::read(&root.join("BUILD.sig"), 64)?,
        key,
        platform,
    )
}
pub fn validate_files(root: &Path, build: &Build) -> io::Result<()> {
    for file in &build.files {
        if !files::matches(root, file)? {
            return Err(manifest::error(
                "Installed files differ from their signed generation; automatic update refused",
            ));
        }
    }
    Ok(())
}
pub fn create(root: &Path, offer: &str) -> io::Result<Journal> {
    if files::linked(&fs::symlink_metadata(root)?) {
        return Err(manifest::error("Portable installation root is linked"));
    }
    let root = root.canonicalize()?;
    let directory = root.join(format!(".numera-update-{}", files::nonce()?));
    files::private_directory(&directory)?;
    let journal = Journal {
        schema: 1,
        root,
        directory,
        manifest_digest: offer.into(),
        nonce: files::nonce()?,
        phase: "staging".into(),
        ticket: String::new(),
        data_root: PathBuf::new(),
        moved: Vec::new(),
        installed: Vec::new(),
        pending: None,
        started: now()?,
    };
    journal.save()?;
    Ok(journal)
}
impl Journal {
    pub fn save(&self) -> io::Result<()> {
        files::json(&self.directory.join("journal.json"), self)
    }
    pub fn load(path: &Path) -> io::Result<Self> {
        let journal: Self = serde_json::from_slice(&files::read(path, manifest::BUILD_LIMIT)?)
            .map_err(|_| manifest::error("Update journal is malformed"))?;
        if files::linked(&fs::symlink_metadata(&journal.directory)?)
            || files::linked(&fs::symlink_metadata(&journal.root)?)
        {
            return Err(manifest::error("Update recovery directory is linked"));
        }
        files::validate_private(&journal.directory)?;
        if journal.schema != 1
            || journal.directory.canonicalize()?
                != path
                    .parent()
                    .ok_or_else(|| manifest::error("Update journal directory missing"))?
                    .canonicalize()?
            || journal.directory.parent() != Some(journal.root.as_path())
            || !journal
                .directory
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(".numera-update-"))
        {
            return Err(manifest::error(
                "Update journal has an invalid ownership root",
            ));
        }
        manifest::decode_hex::<32>(&journal.nonce)?;
        manifest::decode_hex::<32>(&journal.manifest_digest)?;
        if !matches!(
            journal.phase.as_str(),
            "staging"
                | "quiescing"
                | "ready"
                | "activating"
                | "awaiting-ready"
                | "rollback-pending"
                | "committed-needs-resume"
                | "rolled-back"
                | "cancelled"
                | "complete"
        ) {
            return Err(manifest::error(
                "Unknown update journal phase; recovery evidence retained",
            ));
        }
        if matches!(
            journal.phase.as_str(),
            "quiescing"
                | "ready"
                | "activating"
                | "awaiting-ready"
                | "rollback-pending"
                | "committed-needs-resume"
        ) {
            manifest::decode_hex::<32>(&journal.ticket)?;
            if !journal.data_root.is_absolute() {
                return Err(manifest::error(
                    "Update journal has no absolute recovery data root",
                ));
            }
        }
        Ok(journal)
    }
    pub fn backup(&self) -> PathBuf {
        self.directory.join("previous")
    }
    pub fn staged(&self) -> PathBuf {
        self.directory.join("next")
    }
}
fn inventory(build: &Build) -> BTreeMap<String, InstalledFile> {
    build
        .files
        .iter()
        .map(|file| (file.path().into(), file.clone()))
        .collect()
}
fn metadata_files(root: &Path) -> io::Result<Vec<InstalledFile>> {
    ["BUILD.json", "BUILD.sig"]
        .iter()
        .map(|name| {
            let path = root.join(name);
            Ok(InstalledFile::Regular(manifest::RegularFile {
                path: (*name).into(),
                size: fs::metadata(&path)?.len(),
                sha256: files::hash(&path)?,
                executable: false,
            }))
        })
        .collect()
}
pub fn activate(journal: &mut Journal, old: &Build, next: &Build) -> io::Result<()> {
    validate_files(&journal.root, old)?;
    validate_files(&journal.staged(), next)?;
    let mut previous = inventory(old);
    for file in metadata_files(&journal.root)? {
        previous.insert(file.path().into(), file);
    }
    let mut target = inventory(next);
    for file in metadata_files(&journal.staged())? {
        target.insert(file.path().into(), file);
    }
    for name in target.keys() {
        let path = files::member(&journal.root, name)?;
        if !previous.contains_key(name) && fs::symlink_metadata(&path).is_ok() {
            return Err(manifest::error(
                "Update would replace an unowned local file",
            ));
        }
    }
    if !journal.backup().exists() {
        files::private_directory(&journal.backup())?;
    } else {
        files::validate_private(&journal.backup())?;
        if fs::read_dir(journal.backup())?.next().is_some() {
            return Err(manifest::error(
                "Rollback directory is not empty before activation",
            ));
        }
    }
    // Keep authenticated metadata outside the move set so an interruption
    // between BUILD.json and BUILD.sig is still independently recoverable.
    for (source, prefix) in [(&journal.root, "old"), (&journal.staged(), "new")] {
        for name in ["BUILD.json", "BUILD.sig"] {
            files::write(
                &journal.directory.join(format!("{prefix}-{name}")),
                &files::read(&source.join(name), manifest::BUILD_LIMIT)?,
            )?;
        }
    }
    journal.phase = "activating".into();
    journal.save()?;
    // Intent is durable before each rename. Recovery compares old/new digests
    // rather than assuming that the last written progress marker landed.
    for (name, file) in &previous {
        if !files::matches(&journal.root, file)? {
            return Err(manifest::error("Installed file changed during activation"));
        }
        journal.pending = Some(name.clone());
        journal.save()?;
        let source = files::member(&journal.root, name)?;
        let destination = files::member(&journal.backup(), name)?;
        fs::create_dir_all(
            destination
                .parent()
                .ok_or_else(|| manifest::error("Rollback parent missing"))?,
        )?;
        files::move_file(&source, &destination)?;
        journal.moved.push(name.clone());
        journal.pending = None;
        journal.save()?;
        #[cfg(feature = "update-acceptance")]
        if journal.moved.len() == 1 {
            super::acceptance::checkpoint("first-file-moved")?;
        }
    }
    for (name, file) in &target {
        if !files::matches(&journal.staged(), file)? {
            return Err(manifest::error("Staged file changed during activation"));
        }
        journal.pending = Some(name.clone());
        journal.save()?;
        let source = files::member(&journal.staged(), name)?;
        let destination = files::member(&journal.root, name)?;
        fs::create_dir_all(
            destination
                .parent()
                .ok_or_else(|| manifest::error("Installed parent missing"))?,
        )?;
        if fs::symlink_metadata(&destination).is_ok() {
            return Err(manifest::error("Installed file appeared during activation"));
        }
        files::move_file(&source, &destination)?;
        journal.installed.push(name.clone());
        journal.pending = None;
        journal.save()?;
    }
    journal.phase = "awaiting-ready".into();
    journal.save()?;
    Ok(())
}
pub fn rollback(journal: &mut Journal, old: &Build, next: &Build) -> io::Result<()> {
    let mut previous = inventory(old);
    let mut target = inventory(next);
    for (prefix, inventory) in [("old", &mut previous), ("new", &mut target)] {
        for name in ["BUILD.json", "BUILD.sig"] {
            let path = journal.directory.join(format!("{prefix}-{name}"));
            inventory.insert(
                name.into(),
                InstalledFile::Regular(manifest::RegularFile {
                    path: name.into(),
                    size: fs::metadata(&path)?.len(),
                    sha256: files::hash(&path)?,
                    executable: false,
                }),
            );
        }
    }
    let names: BTreeSet<_> = previous.keys().chain(target.keys()).cloned().collect();
    if journal.phase == "activating" && journal.moved.is_empty() && !journal.backup().exists() {
        return Ok(());
    }
    for name in names.into_iter().rev() {
        let live = files::member(&journal.root, &name)?;
        let backup = files::member(&journal.backup(), &name)?;
        let old_file = previous.get(&name);
        if let Some(old_file) = old_file {
            if !files::matches(&journal.backup(), old_file)? {
                if files::matches(&journal.root, old_file)? {
                    continue;
                }
                return Err(manifest::error(
                    "Rollback old file is missing or changed; retain recovery journal",
                ));
            }
        }
        if fs::symlink_metadata(&live).is_ok() {
            let Some(new_file) = target.get(&name) else {
                return Err(manifest::error("Rollback found an unrecognized live file"));
            };
            if !files::matches(&journal.root, new_file)? {
                return Err(manifest::error("Rollback refuses a changed live file"));
            }
            let failed = journal.directory.join("failed").join(&name);
            fs::create_dir_all(
                failed
                    .parent()
                    .ok_or_else(|| manifest::error("Failed generation parent missing"))?,
            )?;
            files::move_file(&live, &failed)?;
        }
        if old_file.is_some() {
            fs::create_dir_all(
                live.parent()
                    .ok_or_else(|| manifest::error("Rollback destination missing"))?,
            )?;
            files::move_file(&backup, &live)?;
        }
    }
    validate_files(&journal.root, old)?;
    journal.phase = "rollback-pending".into();
    journal.pending = None;
    journal.save()?;
    Ok(())
}
pub fn generation_floor(root: &Path, installed: u64) -> io::Result<u64> {
    let path = root.join(".numera-update-generation.json");
    match files::read(&path, 1024) {
        Ok(bytes) => {
            let value: serde_json::Value = serde_json::from_slice(&bytes)
                .map_err(|_| manifest::error("Accepted generation state is damaged"))?;
            Ok(installed.max(
                value["generation"]
                    .as_u64()
                    .ok_or_else(|| manifest::error("Accepted generation missing"))?,
            ))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(installed),
        Err(error) => Err(error),
    }
}
pub fn commit_floor(root: &Path, generation: u64) -> io::Result<()> {
    files::json(
        &root.join(".numera-update-generation.json"),
        &serde_json::json!({"generation":generation_floor(root,generation)?}),
    )
}
