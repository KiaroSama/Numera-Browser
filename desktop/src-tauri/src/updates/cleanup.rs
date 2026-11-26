use super::{files, manifest, transaction::Journal};
use std::{fs, io, path::Path};

pub(super) fn prune(root: &Path, data_root: &Path) -> io::Result<u64> {
    if files::linked(&fs::symlink_metadata(root)?) {
        return Err(manifest::error("Cannot clean a linked installation"));
    }
    let root = root.canonicalize()?;
    let data_root = data_root.canonicalize()?;
    let mut directories = fs::read_dir(&root)?.collect::<Result<Vec<_>, _>>()?;
    directories.sort_by_key(fs::DirEntry::file_name);
    let mut plan = Vec::new();
    let mut bytes = 0u64;
    for entry in directories {
        let name = entry.file_name();
        let Some(identity) = name
            .to_str()
            .and_then(|name| name.strip_prefix(".numera-update-"))
        else {
            continue;
        };
        if manifest::decode_hex::<32>(identity).is_err() {
            continue;
        }
        let directory = entry.path();
        if data_root.starts_with(&directory) || directory.starts_with(&data_root) {
            return Err(manifest::error(
                "Update cache overlaps the active data root",
            ));
        }
        if !entry.file_type()?.is_dir() || files::linked(&fs::symlink_metadata(&directory)?) {
            return Err(manifest::error(
                "Update cache directory is not safe to clean",
            ));
        }
        let journal = Journal::load(&directory.join("journal.json"))?;
        if journal.root != root
            || !matches!(
                journal.phase.as_str(),
                "complete" | "cancelled" | "rolled-back" | "staging"
            )
        {
            continue;
        }
        if !journal.data_root.as_os_str().is_empty() {
            match journal.data_root.canonicalize() {
                Ok(recorded)
                    if recorded.starts_with(&directory) || directory.starts_with(&recorded) =>
                {
                    return Err(manifest::error(
                        "Update cache overlaps a recorded data root",
                    ));
                }
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
        for name in ["archive.part", "failed", "next", "previous"] {
            let path = directory.join(name);
            match fs::symlink_metadata(&path) {
                Ok(meta) => {
                    bytes = bytes
                        .checked_add(size(&path, &meta)?)
                        .ok_or_else(|| manifest::error("Update cache size overflow"))?;
                    plan.push((path, meta.is_dir()));
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
    }
    for (path, directory) in plan {
        if directory {
            fs::remove_dir_all(path)?;
        } else {
            fs::remove_file(path)?;
        }
    }
    Ok(bytes)
}

fn size(path: &Path, meta: &fs::Metadata) -> io::Result<u64> {
    if files::linked(meta) {
        if meta.is_symlink() {
            return Ok(0);
        }
        return Err(manifest::error("Update cache contains a reparse point"));
    }
    if meta.is_file() {
        return Ok(meta.len());
    }
    if !meta.is_dir() {
        return Err(manifest::error(
            "Update cache contains an unexpected file type",
        ));
    }
    let mut bytes = 0u64;
    for entry in fs::read_dir(path)? {
        let path = entry?.path();
        bytes = bytes
            .checked_add(size(&path, &fs::symlink_metadata(&path)?)?)
            .ok_or_else(|| manifest::error("Update cache size overflow"))?;
    }
    Ok(bytes)
}
