//! Shared home expansion for persisted paths, CLI settings and browser resources.
use std::{
    io,
    path::{Component, Path, PathBuf},
};

fn home_error(message: String) -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, message)
}

/// Expand current or named-user homes using the supplied current home and the
/// platform account lookup. Non-home paths and non-Unicode suffixes stay owned.
pub fn expand(path: &Path, home: &Path) -> io::Result<PathBuf> {
    // pathlib.Path removes current-directory components before expanduser.
    // Work with OS components so non-Unicode suffixes never become text.
    let mut normalized: PathBuf = path
        .components()
        .filter(|component| !matches!(component, Component::CurDir))
        .collect();
    if normalized.as_os_str().is_empty() {
        normalized.push(".");
    }
    let mut components = normalized.components();
    let Some(Component::Normal(first)) = components.next() else {
        return Ok(normalized);
    };
    let Some(name) = first.to_str().and_then(|text| text.strip_prefix('~')) else {
        if first.as_encoded_bytes().first() == Some(&b'~') {
            return Err(home_error("Could not determine home directory.".into()));
        }
        return Ok(normalized);
    };
    let base = if name.is_empty() {
        home.to_owned()
    } else {
        named_home(name, home)?
    };
    Ok(base.join(components.as_path()).components().collect())
}

#[cfg(unix)]
#[allow(unsafe_code)]
fn named_home(name: &str, _: &std::path::Path) -> std::io::Result<PathBuf> {
    use std::{
        ffi::{CStr, CString, OsStr},
        mem::MaybeUninit,
        os::unix::ffi::OsStrExt,
        ptr,
    };
    let name =
        CString::new(name).map_err(|_| home_error("Could not determine home directory.".into()))?;
    let mut length = 1024;
    loop {
        let mut buffer = vec![0u8; length];
        let mut record = MaybeUninit::<libc::passwd>::uninit();
        let mut result = ptr::null_mut();
        // SAFETY: all outputs are live, correctly aligned, and writable for
        // the supplied buffer length. The reentrant API owns no global record.
        let code = unsafe {
            libc::getpwnam_r(
                name.as_ptr(),
                record.as_mut_ptr(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                &mut result,
            )
        };
        if code == libc::ERANGE && length < 1024 * 1024 {
            length *= 2;
            continue;
        }
        if code == libc::ENOENT || code == libc::ESRCH {
            return Err(home_error("Could not determine home directory.".into()));
        }
        if code != 0 {
            return Err(std::io::Error::from_raw_os_error(code));
        }
        if result.is_null() {
            return Err(home_error("Could not determine home directory.".into()));
        }
        // SAFETY: successful nonnull getpwnam_r initializes the passwd record
        // and its NUL-terminated directory inside the retained buffer.
        let directory = unsafe { (*result).pw_dir };
        if directory.is_null() {
            return Err(home_error("Could not determine home directory.".into()));
        }
        // SAFETY: the successful lookup's directory remains live until copied.
        let directory = unsafe { CStr::from_ptr(directory) };
        return Ok(PathBuf::from(OsStr::from_bytes(directory.to_bytes())));
    }
}

#[cfg(windows)]
fn named_home(name: &str, home: &std::path::Path) -> std::io::Result<PathBuf> {
    windows_named_home(name, home, std::env::var_os("USERNAME").as_deref())
}

/// Windows' account-name path policy, with observations supplied by its caller.
#[cfg(windows)]
pub fn windows_named_home(
    name: &str,
    home: &std::path::Path,
    current: Option<&std::ffi::OsStr>,
) -> std::io::Result<PathBuf> {
    // CPython ntpath expands another user only when the current profile has
    // the ordinary <parent>/<USERNAME> shape. It does not query the account
    // database or require the guessed sibling profile to exist.
    if current == Some(std::ffi::OsStr::new(name)) {
        return Ok(home.to_owned());
    }
    if current.is_none() || home.file_name() != current {
        return Err(home_error("Could not determine home directory.".into()));
    }
    Ok(home
        .parent()
        .ok_or_else(|| home_error("Could not determine home directory.".into()))?
        .join(name))
}
