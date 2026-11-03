use super::archive_paths::member_path;
use std::{
    fs,
    io::{self, Cursor},
    path::{Path, PathBuf},
};
use zip::ZipArchive;

fn invalid_zip(error: zip::result::ZipError) -> io::Error {
    let message = if matches!(error, zip::result::ZipError::InvalidArchive(_)) {
        "File is not a zip file".to_owned()
    } else {
        error.to_string()
    };
    io::Error::other(format!(
        "the downloaded file is not a valid extension package: {message}"
    ))
}

fn word(data: &[u8], offset: usize) -> io::Result<u32> {
    let bytes: [u8; 4] = data
        .get(offset..offset + 4)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| io::Error::other("Extension CRX header is truncated"))?;
    Ok(u32::from_le_bytes(bytes))
}

fn zip_bytes(data: &[u8], crx: bool) -> io::Result<&[u8]> {
    if !crx {
        return Ok(data);
    }
    if !data.starts_with(b"Cr24") {
        return Err(io::Error::other("Extension package has no CRX magic"));
    }
    let start = match word(data, 4)? {
        3 => 12u64 + u64::from(word(data, 8)?),
        2 => 16u64 + u64::from(word(data, 8)?) + u64::from(word(data, 12)?),
        version => {
            return Err(io::Error::other(format!(
                "unsupported CRX version {version}"
            )));
        }
    };
    let start = usize::try_from(start)
        .map_err(|_| io::Error::other("Extension CRX header length overflows"))?;
    data.get(start..)
        .ok_or_else(|| io::Error::other("Extension CRX header exceeds package length"))
}

fn contained_target(base: &Path, member: &str) -> io::Result<PathBuf> {
    let target = member_path(base, member)?;
    let mut ancestor = target.as_path();
    loop {
        match fs::symlink_metadata(ancestor) {
            Ok(_) => {
                if !fs::canonicalize(ancestor)?.starts_with(base) {
                    return Err(io::Error::other(
                        "Archive member resolves outside the folder",
                    ));
                }
                break;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                ancestor = ancestor
                    .parent()
                    .ok_or_else(|| io::Error::other("Archive path has no containing directory"))?;
            }
            Err(error) => return Err(error),
        }
    }
    Ok(target)
}

/// Parse the format and validate every member before the first write. The caller
/// installs into private staging; this primitive is not a transactional installer.
pub fn unpack(data: &[u8], destination: &Path, crx: bool) -> io::Result<()> {
    let bytes = zip_bytes(data, crx)?;
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(invalid_zip)?;
    let base = fs::canonicalize(destination)?;
    let mut targets = Vec::new();
    for index in 0..archive.len() {
        let file = archive.by_index(index).map_err(invalid_zip)?;
        if file.is_dir() {
            continue;
        }
        if file
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(io::Error::other(
                "Extension extraction refuses symbolic link members",
            ));
        }
        let name = file.name().map_err(invalid_zip)?;
        let target = contained_target(&base, &name).map_err(|error| {
            io::Error::other(format!(
                "refusing archive member outside the folder: {error}"
            ))
        })?;
        targets.push((index, target));
    }
    for (index, target) in targets {
        let mut input = archive.by_index(index).map_err(invalid_zip)?;
        fs::create_dir_all(
            target
                .parent()
                .ok_or_else(|| io::Error::other("Extension member has no parent"))?,
        )?;
        let mut options = fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut output = options.open(target)?;
        io::copy(&mut input, &mut output)?;
        output.sync_all()?;
    }
    Ok(())
}
