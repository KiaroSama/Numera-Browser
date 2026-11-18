//! Read-only host facts used by Camoufox. No environment or profile mutation.
use std::path::Path;

pub fn cpu_count() -> Option<u32> {
    #[cfg(target_os = "linux")]
    {
        #[allow(unsafe_code)]
        // SAFETY: the zeroed cpu_set_t is writable for exactly its allocation;
        // sched_getaffinity initializes it on success, then CPU_COUNT only reads it.
        let count = unsafe {
            let mut set: libc::cpu_set_t = std::mem::zeroed();
            if libc::sched_getaffinity(0, std::mem::size_of_val(&set), &mut set) == 0 {
                u32::try_from(libc::CPU_COUNT(&set)).ok().filter(|n| *n > 0)
            } else {
                None
            }
        };
        if count.is_some() {
            return count;
        }
    }
    std::thread::available_parallelism()
        .ok()
        .and_then(|n| u32::try_from(n.get()).ok())
}
pub fn stock_disk_limit() -> Option<u64> {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })?;
    let home = std::path::PathBuf::from(home);
    let profile = if cfg!(windows) {
        std::env::var_os("APPDATA")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| home.clone())
            .join("Mozilla")
    } else if cfg!(target_os = "macos") {
        home.join("Library/Application Support/Firefox")
    } else {
        home.join(".mozilla")
    };
    for candidate in [&profile, &home] {
        let mut path = candidate.as_path();
        while !path.exists() {
            path = match path.parent() {
                Some(parent) => parent,
                None => break,
            };
        }
        if let Some(total) = capacity(path).filter(|total| *total > 0) {
            return Some((total / 2 / 1024).min(i32::MAX as u64));
        }
    }
    None
}
#[cfg(unix)]
fn capacity(path: &Path) -> Option<u64> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    let path = CString::new(path.as_os_str().as_bytes()).ok()?;
    #[allow(unsafe_code)]
    // SAFETY: statvfs receives a terminated path and writable statvfs allocation;
    // fields are read only after its successful initialization.
    unsafe {
        let mut output = std::mem::MaybeUninit::<libc::statvfs>::uninit();
        if libc::statvfs(path.as_ptr(), output.as_mut_ptr()) != 0 {
            return None;
        }
        let output = output.assume_init();
        // statvfs field widths differ across supported Unix ABIs.
        #[allow(clippy::unnecessary_cast)]
        let capacity = (output.f_blocks as u64).checked_mul(output.f_frsize as u64);
        capacity
    }
}
#[cfg(windows)]
fn capacity(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut total = 0;
    #[allow(unsafe_code)]
    // SAFETY: terminated path and live writable u64; unused result pointers may be null.
    let success = unsafe {
        windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(
            path.as_ptr(),
            std::ptr::null_mut(),
            &mut total,
            std::ptr::null_mut(),
        )
    };
    (success != 0).then_some(total)
}
