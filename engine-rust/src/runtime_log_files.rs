use std::{
    fs::{self, File, Metadata, OpenOptions},
    io,
    path::Path,
};

const KEEP_RUNS: usize = 20;

pub(super) fn create(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{FILE_SHARE_READ, FILE_SHARE_WRITE};
        // Readers remain usable; deletion is refused until the writer closes.
        options.share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE);
    }
    #[cfg(not(unix))]
    {
        options.open(path)
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let name = path
            .file_name()
            .ok_or_else(|| io::Error::other("Log filename missing"))?;
        let mut pending = std::ffi::OsString::from(".");
        pending.push(name);
        pending.push(".pending");
        let pending = path.with_file_name(pending);
        let file = options.mode(0o600).open(&pending)?;
        let temporary = Pending(pending);
        file.try_lock()?;
        // A final name is never visible before its lifetime lock is held.
        fs::hard_link(&temporary.0, path)?;
        drop(temporary);
        Ok(file)
    }
}

#[cfg(unix)]
struct Pending(std::path::PathBuf);
#[cfg(unix)]
impl Drop for Pending {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn run_name(name: &str, component: &str) -> bool {
    let Some(rest) = name
        .strip_prefix(component)
        .and_then(|name| name.strip_prefix('_'))
    else {
        return false;
    };
    let Some(rest) = rest.strip_suffix(".log") else {
        return false;
    };
    let (stamp, suffix) = rest
        .split_once('.')
        .map_or((rest, None), |(stamp, suffix)| (stamp, Some(suffix)));
    if let Some(suffix) = suffix
        && !suffix
            .parse::<u16>()
            .is_ok_and(|value| (1..1000).contains(&value) && value.to_string() == suffix)
    {
        return false;
    }
    chrono::NaiveDateTime::parse_from_str(stamp, "%Y-%m-%d_%H-%M-%S_UTC")
        .is_ok_and(|date| date.format("%Y-%m-%d_%H-%M-%S_UTC").to_string() == stamp)
}

fn regular(metadata: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return false;
        }
    }
    metadata.is_file() && !metadata.is_symlink()
}

pub(super) fn prune(directory: &Path, component: &str, current: &Path) -> io::Result<()> {
    let mut files = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let name = entry.file_name();
        if !name.to_str().is_some_and(|name| run_name(name, component)) {
            continue;
        }
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if regular(&metadata) {
            files.push((metadata.modified()?, path, metadata));
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    let excess = files.len().saturating_sub(KEEP_RUNS);
    for (_, path, metadata) in files.into_iter().take(excess) {
        if path != current {
            remove_inactive(&path, &metadata)?;
        }
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn remove_inactive(_: &Path, _: &Metadata) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Runtime log retention is unsupported",
    ))
}

#[cfg(unix)]
fn remove_inactive(path: &Path, observed: &Metadata) -> io::Result<()> {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    let file = match OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    // A live run or another pruner owns this file. Never wait for it.
    match file.try_lock() {
        Ok(()) => (),
        Err(std::fs::TryLockError::WouldBlock) => return Ok(()),
        Err(std::fs::TryLockError::Error(error)) => return Err(error),
    }
    let opened = file.metadata()?;
    let current = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    if regular(&current)
        && current.nlink() == 1
        && opened.nlink() == 1
        && current.dev() == observed.dev()
        && current.ino() == observed.ino()
        && current.dev() == opened.dev()
        && current.ino() == opened.ino()
        && current.modified()? == observed.modified()?
    {
        fs::remove_file(path)?;
    }
    Ok(())
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn remove_inactive(path: &Path, observed: &Metadata) -> io::Result<()> {
    use std::os::windows::{fs::OpenOptionsExt, io::AsRawHandle};
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, DELETE, FILE_DISPOSITION_INFO, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FileDispositionInfo, GetFileInformationByHandle,
        SetFileInformationByHandle,
    };
    let file = match OpenOptions::new()
        .access_mode(DELETE | FILE_READ_ATTRIBUTES)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
    {
        Ok(file) => file,
        Err(error)
            if error.kind() == io::ErrorKind::NotFound || error.raw_os_error() == Some(32) =>
        {
            return Ok(());
        }
        Err(error) => return Err(error),
    };
    let current = file.metadata()?;
    if !regular(&current) || current.modified()? != observed.modified()? {
        return Ok(());
    }
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: live owned file handle and correctly typed initialized output.
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    if info.nNumberOfLinks != 1 {
        return Ok(());
    }
    let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
    // SAFETY: DELETE access was requested; exact structure/class/size. Deletion
    // targets this checked handle, not a potentially replaced pathname.
    if unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle(),
            FileDispositionInfo,
            std::ptr::addr_of!(disposition).cast(),
            std::mem::size_of_val(&disposition) as u32,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
