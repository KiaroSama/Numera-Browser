use super::{
    files,
    manifest::{self, Asset, Build, InstalledFile},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

pub fn extract(
    archive: &Path,
    stage: &Path,
    asset: &Asset,
    key: &[u8; 32],
    cancelled: &AtomicBool,
) -> io::Result<Build> {
    fs::create_dir(stage)?;
    let mut writer = Writer {
        root: stage,
        asset,
        cancelled,
        end: Instant::now() + Duration::from_secs(1800),
        count: 0,
        total: 0,
        seen: BTreeSet::new(),
        identities: BTreeSet::new(),
        links: Vec::new(),
    };
    if asset.name.ends_with(".zip") {
        let mut zip = zip::ZipArchive::new(File::open(archive)?).map_err(io::Error::other)?;
        if zip.len() as u64 > asset.max_members {
            return Err(manifest::error("Update archive has too many members"));
        }
        for index in 0..zip.len() {
            let mut member = zip.by_index(index).map_err(io::Error::other)?;
            let name = std::str::from_utf8(member.name_raw())
                .map_err(|_| manifest::error("Update archive name is not UTF8"))?
                .to_owned();
            let mode = member.unix_mode().unwrap_or(0);
            let directory = member.is_dir();
            let symlink = mode & 0o170000 == 0o120000;
            if mode & 0o170000 != 0 && mode & 0o170000 != 0o100000 && !directory && !symlink {
                return Err(manifest::error("Update ZIP contains a special file"));
            }
            if symlink {
                let mut target = String::new();
                member.by_ref().take(4097).read_to_string(&mut target)?;
                writer.link(&name, &target)?;
            } else if directory {
                writer.directory(&name)?;
            } else {
                let size = member.size();
                writer.regular(&name, size, mode & 0o111 != 0, &mut member)?;
            }
        }
    } else {
        let decoder = flate2::read::GzDecoder::new(File::open(archive)?);
        let mut tar = tar::Archive::new(decoder);
        for member in tar.entries()? {
            let mut member = member?;
            let name = member
                .path()?
                .to_str()
                .ok_or_else(|| manifest::error("Update archive name is not UTF8"))?
                .to_owned();
            let kind = member.header().entry_type();
            if kind.is_dir() {
                writer.directory(&name)?;
            } else if kind.is_file() {
                let size = member.size();
                let executable = member.header().mode()? & 0o111 != 0;
                writer.regular(&name, size, executable, &mut member)?;
            } else if kind.is_symlink() {
                let target = member
                    .link_name()?
                    .ok_or_else(|| manifest::error("Update link target missing"))?
                    .to_str()
                    .ok_or_else(|| manifest::error("Update link target is not UTF8"))?
                    .to_owned();
                writer.link(&name, &target)?;
            } else {
                return Err(manifest::error(
                    "Update archive contains an unsupported special member",
                ));
            }
        }
    }
    let build_bytes = files::read(&stage.join("BUILD.json"), manifest::BUILD_LIMIT)?;
    if manifest::digest(&build_bytes) != asset.build_manifest_sha256 {
        return Err(manifest::error("Staged BUILD inventory changed"));
    }
    let signature = files::read(&stage.join("BUILD.sig"), 64)?;
    let build = manifest::signed_build(&build_bytes, &signature, key, &asset.platform)?;
    let inventory: BTreeMap<_, _> = build
        .files
        .iter()
        .map(|file| (file.path().to_owned(), file))
        .collect();
    let actual: BTreeSet<_> = writer
        .seen
        .iter()
        .filter(|name| {
            !name.ends_with('/') && name.as_str() != "BUILD.json" && name.as_str() != "BUILD.sig"
        })
        .cloned()
        .collect();
    if actual != inventory.keys().cloned().collect() {
        return Err(manifest::error(
            "Archive members differ from the authenticated file inventory",
        ));
    }
    for (name, target) in writer.links {
        let Some(InstalledFile::Link(file)) = inventory.get(&name).copied() else {
            return Err(manifest::error("Archive link absent from inventory"));
        };
        if file.link != target {
            return Err(manifest::error("Archive link target changed"));
        }
        let path = files::member(stage, &name)?;
        fs::create_dir_all(
            path.parent()
                .ok_or_else(|| manifest::error("Update link parent missing"))?,
        )?;
        files::link(&target, &path)?;
    }
    for file in &build.files {
        if !files::matches(stage, file)? {
            return Err(manifest::error(
                "Staged update file failed readback verification",
            ));
        }
    }
    Ok(build)
}
struct Writer<'a> {
    root: &'a Path,
    asset: &'a Asset,
    cancelled: &'a AtomicBool,
    end: Instant,
    count: u64,
    total: u64,
    seen: BTreeSet<String>,
    identities: BTreeSet<String>,
    links: Vec<(String, String)>,
}
impl Writer<'_> {
    fn path(&mut self, name: &str, directory: bool) -> io::Result<Option<(String, PathBuf)>> {
        if self.cancelled.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "Update cancelled",
            ));
        }
        if Instant::now() >= self.end {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Update archive extraction timed out",
            ));
        }
        self.count += 1;
        if self.count > self.asset.max_members {
            return Err(manifest::error("Update archive has too many members"));
        }
        let name = name.trim_end_matches('/');
        if name == self.asset.archive_root && directory {
            return Ok(None);
        }
        let prefix = format!("{}/", self.asset.archive_root);
        let relative = name
            .strip_prefix(&prefix)
            .ok_or_else(|| manifest::error("Update archive root changed"))?;
        manifest::safe_path(relative)?;
        let identity = if directory {
            format!("{relative}/")
        } else {
            relative.into()
        };
        if !self.identities.insert(identity.to_ascii_lowercase()) || !self.seen.insert(identity) {
            return Err(manifest::error("Duplicate archive member"));
        }
        Ok(Some((relative.into(), files::member(self.root, relative)?)))
    }
    fn directory(&mut self, name: &str) -> io::Result<()> {
        if let Some((_, path)) = self.path(name, true)? {
            fs::create_dir_all(path)?;
        }
        Ok(())
    }
    fn link(&mut self, name: &str, target: &str) -> io::Result<()> {
        let Some((name, _)) = self.path(name, false)? else {
            return Err(manifest::error("Invalid archive link"));
        };
        if target.len() > 4096 {
            return Err(manifest::error("Update link is too long"));
        }
        manifest::safe_link(&name, target)?;
        self.links.push((name, target.into()));
        Ok(())
    }
    fn regular(
        &mut self,
        name: &str,
        size: u64,
        executable: bool,
        input: &mut impl Read,
    ) -> io::Result<()> {
        let Some((_, path)) = self.path(name, false)? else {
            return Err(manifest::error("Invalid archive file"));
        };
        self.total = self
            .total
            .checked_add(size)
            .ok_or_else(|| manifest::error("Archive expansion overflow"))?;
        if self.total > self.asset.expanded_size {
            return Err(manifest::error(
                "Archive exceeds authenticated expanded size",
            ));
        }
        fs::create_dir_all(
            path.parent()
                .ok_or_else(|| manifest::error("Update file parent missing"))?,
        )?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        let mut written = 0u64;
        let mut bytes = [0; 65536];
        loop {
            if self.cancelled.load(Ordering::Acquire) || Instant::now() >= self.end {
                return Err(io::Error::new(
                    io::ErrorKind::ConnectionAborted,
                    "Update extraction cancelled or timed out",
                ));
            }
            let count = input.read(&mut bytes)?;
            if count == 0 {
                break;
            }
            written += count as u64;
            if written > size {
                return Err(manifest::error("Archive member exceeds its declared size"));
            }
            file.write_all(&bytes[..count])?;
        }
        if written != size {
            return Err(manifest::error("Archive member was truncated"));
        }
        file.sync_all()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(
                path,
                fs::Permissions::from_mode(if executable { 0o755 } else { 0o644 }),
            )?;
        }
        #[cfg(not(unix))]
        let _ = executable;
        Ok(())
    }
}
