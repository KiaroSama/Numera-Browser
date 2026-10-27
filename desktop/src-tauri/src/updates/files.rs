use super::manifest::{self, InstalledFile};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

pub fn linked(meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    meta.is_symlink()
}
pub fn validate_private(path: &Path) -> io::Result<()> {
    super::custody::validate(path)?;
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let meta = fs::symlink_metadata(entry.path())?;
        if linked(&meta) {
            return Err(manifest::error(
                "Private update recovery contains a linked member",
            ));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if [
                "journal.json",
                "manifest.json",
                "manifest.sig",
                "update-worker",
                "update-worker.exe",
                "old-BUILD.json",
                "old-BUILD.sig",
                "new-BUILD.json",
                "new-BUILD.sig",
            ]
            .contains(&entry.file_name().to_string_lossy().as_ref())
                && (meta.uid() != unsafe { libc::geteuid() } || meta.mode() & 0o077 != 0)
            {
                return Err(manifest::error(
                    "Private update recovery member permissions are unsafe",
                ));
            }
        }
    }
    Ok(())
}
pub fn nonce() -> io::Result<String> {
    let mut bytes = [0; 32];
    getrandom::fill(&mut bytes)
        .map_err(|_| io::Error::other("Update randomness is unavailable"))?;
    Ok(manifest::hex(&bytes))
}
pub fn read(path: &Path, limit: u64) -> io::Result<Vec<u8>> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_file() || linked(&meta) || meta.len() > limit {
        return Err(manifest::error(
            "Update metadata is not a bounded regular file",
        ));
    }
    let mut bytes = Vec::new();
    File::open(path)?.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(manifest::error("Update metadata exceeds its limit"));
    }
    Ok(bytes)
}
pub fn write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| manifest::error("Update output parent missing"))?;
    let temporary = parent.join(format!(".write-{}", nonce()?));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        };
        let source: Vec<_> = temporary.as_os_str().encode_wide().chain([0]).collect();
        let target: Vec<_> = path.as_os_str().encode_wide().chain([0]).collect();
        // SAFETY: two retained, terminated path buffers; only this transaction owns temporary.
        if unsafe {
            MoveFileExW(
                source.as_ptr(),
                target.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
    }
    #[cfg(not(windows))]
    {
        fs::rename(&temporary, path)?;
        File::open(parent)?.sync_all()?;
    }
    Ok(())
}
pub fn json(path: &Path, value: &impl serde::Serialize) -> io::Result<()> {
    write(path, &serde_json::to_vec(value).map_err(io::Error::other)?)
}
pub fn lock(root: &Path) -> io::Result<File> {
    let path = root.join(".numera-update.lock");
    if fs::symlink_metadata(&path).is_ok_and(|meta| linked(&meta)) {
        return Err(manifest::error("Update lock is linked"));
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    file.try_lock().map_err(|_| {
        io::Error::new(
            io::ErrorKind::WouldBlock,
            "Another update transaction owns this installation",
        )
    })?;
    Ok(file)
}
pub fn move_file(source: &Path, target: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_WRITE_THROUGH};
        let source: Vec<_> = source.as_os_str().encode_wide().chain([0]).collect();
        let target: Vec<_> = target.as_os_str().encode_wide().chain([0]).collect();
        // SAFETY: retained terminated paths, same-volume transaction, never overwrite an unknown target.
        if unsafe { MoveFileExW(source.as_ptr(), target.as_ptr(), MOVEFILE_WRITE_THROUGH) } == 0 {
            return Err(io::Error::last_os_error());
        }
    }
    #[cfg(not(windows))]
    {
        fs::rename(source, target)?;
        for parent in [source.parent(), target.parent()].into_iter().flatten() {
            File::open(parent)?.sync_all()?;
        }
    }
    Ok(())
}
pub fn hash(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut bytes = [0; 65536];
    loop {
        let count = file.read(&mut bytes)?;
        if count == 0 {
            break;
        }
        hash.update(&bytes[..count]);
    }
    Ok(manifest::hex(&hash.finalize()))
}
pub fn member(root: &Path, relative: &str) -> io::Result<PathBuf> {
    manifest::safe_path(relative)?;
    let mut path = root.to_owned();
    let parts = relative.split('/').collect::<Vec<_>>();
    for part in &parts[..parts.len() - 1] {
        path.push(part);
        match fs::symlink_metadata(&path) {
            Ok(meta) if !meta.is_dir() || linked(&meta) => {
                return Err(manifest::error(
                    "Update parent is linked or not a directory",
                ))
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(root.join(relative))
}
pub fn matches(root: &Path, file: &InstalledFile) -> io::Result<bool> {
    let path = member(root, file.path())?;
    let meta = match fs::symlink_metadata(&path) {
        Ok(value) => value,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    match file {
        InstalledFile::Regular(record) => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if (meta.permissions().mode() & 0o111 != 0) != record.executable {
                    return Ok(false);
                }
            }
            Ok(meta.is_file()
                && !linked(&meta)
                && meta.len() == record.size
                && hash(&path)? == record.sha256)
        }
        InstalledFile::Link(record) => {
            Ok(linked(&meta) && fs::read_link(path)?.to_string_lossy() == record.link)
        }
    }
}
#[cfg(unix)]
pub fn private_directory(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    fs::DirBuilder::new().mode(0o700).create(path)?;
    if fs::metadata(path)?.permissions().mode() & 0o077 != 0 {
        return Err(manifest::error(
            "Private update directory permissions are unsafe",
        ));
    }
    Ok(())
}
#[cfg(windows)]
pub fn private_directory(path: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::{
            Authorization::{
                ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
            },
            SECURITY_ATTRIBUTES,
        },
        Storage::FileSystem::CreateDirectoryW,
    };
    let parent = path
        .parent()
        .ok_or_else(|| manifest::error("Private update directory parent missing"))?;
    let parent: Vec<_> = parent.as_os_str().encode_wide().chain([0]).collect();
    let mut volume = [0u16; 32768];
    let mut flags = 0;
    // SAFETY: retained input, initialized bounded volume buffer and flags output.
    if unsafe {
        windows_sys::Win32::Storage::FileSystem::GetVolumePathNameW(
            parent.as_ptr(),
            volume.as_mut_ptr(),
            volume.len() as u32,
        )
    } == 0
        || unsafe {
            windows_sys::Win32::Storage::FileSystem::GetVolumeInformationW(
                volume.as_ptr(),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut flags,
                std::ptr::null_mut(),
                0,
            )
        } == 0
    {
        return Err(io::Error::last_os_error());
    }
    if flags & 0x0000_0008 == 0 {
        return Err(manifest::error(
            "Update recovery requires a filesystem with persistent access controls",
        ));
    }
    let policy: Vec<_> = "D:P(A;OICI;FA;;;OW)(A;OICI;FA;;;SY)"
        .encode_utf16()
        .chain([0])
        .collect();
    let name: Vec<_> = path.as_os_str().encode_wide().chain([0]).collect();
    let mut descriptor = std::ptr::null_mut();
    // SAFETY: initialized descriptor output; fixed protected owner/system DACL.
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            policy.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let attributes = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor,
        bInheritHandle: 0,
    };
    // SAFETY: descriptor retained through synchronous creation; freed on both outcomes.
    let result = unsafe { CreateDirectoryW(name.as_ptr(), &attributes) };
    let error = io::Error::last_os_error();
    unsafe {
        LocalFree(descriptor);
    }
    if result == 0 {
        return Err(error);
    }
    validate_private(path)
}
#[cfg(unix)]
pub fn available(path: &Path) -> io::Result<u64> {
    use std::os::unix::ffi::OsStrExt;
    let path = std::ffi::CString::new(path.as_os_str().as_bytes()).map_err(io::Error::other)?;
    let mut stat = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: initialized by successful statvfs, retained nul-terminated path.
    if unsafe { libc::statvfs(path.as_ptr(), stat.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let stat = unsafe { stat.assume_init() };
    u64::try_from(u128::from(stat.f_bavail) * u128::from(stat.f_frsize))
        .map_err(|_| manifest::error("Filesystem free-space count overflow"))
}
#[cfg(windows)]
pub fn available(path: &Path) -> io::Result<u64> {
    use std::os::windows::ffi::OsStrExt;
    let path: Vec<_> = path.as_os_str().encode_wide().chain([0]).collect();
    let mut free = 0;
    // SAFETY: retained terminated path and initialized free-byte output.
    if unsafe {
        windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(
            path.as_ptr(),
            &mut free,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(free)
}
#[cfg(unix)]
pub fn link(target: &str, path: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, path)
}
#[cfg(windows)]
pub fn link(_target: &str, _path: &Path) -> io::Result<()> {
    Err(manifest::error(
        "Windows portable archives must contain regular files",
    ))
}
