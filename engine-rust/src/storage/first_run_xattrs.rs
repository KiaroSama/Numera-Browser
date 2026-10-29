//! Linux copy2-compatible metadata on the already opened source and destination.
use std::{ffi::CStr, fs::File, io, os::fd::AsRawFd};

const LIMIT: usize = 65536;

fn optional(error: &io::Error, value: bool) -> bool {
    matches!(
        error.raw_os_error(),
        Some(libc::ENOTSUP | libc::ENODATA | libc::EINVAL)
    ) || value && matches!(error.raw_os_error(), Some(libc::EPERM | libc::EACCES))
}

pub(super) fn copy(source: &File, destination: &File) -> io::Result<()> {
    let mut names = vec![0u8; LIMIT];
    // SAFETY: both descriptors stay owned by the caller; the writable buffer is
    // initialized and its exact capacity is supplied to libc.
    #[allow(unsafe_code)]
    let size =
        unsafe { libc::flistxattr(source.as_raw_fd(), names.as_mut_ptr().cast(), names.len()) };
    if size < 0 {
        let error = io::Error::last_os_error();
        return if optional(&error, false) {
            Ok(())
        } else {
            Err(error)
        };
    }
    let size = usize::try_from(size).map_err(io::Error::other)?;
    let names = names
        .get(..size)
        .ok_or_else(|| io::Error::other("Attribute list exceeds its buffer"))?;
    if names.is_empty() {
        return Ok(());
    }
    let mut value = vec![0u8; LIMIT];
    for name in names.split_inclusive(|byte| *byte == 0) {
        let name = CStr::from_bytes_with_nul(name).map_err(io::Error::other)?;
        // SAFETY: name is NUL-terminated and value is writable for the supplied
        // length. No pointer escapes this call or either caller-owned file.
        #[allow(unsafe_code)]
        let size = unsafe {
            libc::fgetxattr(
                source.as_raw_fd(),
                name.as_ptr(),
                value.as_mut_ptr().cast(),
                value.len(),
            )
        };
        if size < 0 {
            let error = io::Error::last_os_error();
            if optional(&error, true) {
                continue;
            }
            return Err(error);
        }
        let size = usize::try_from(size).map_err(io::Error::other)?;
        let value = value
            .get(..size)
            .ok_or_else(|| io::Error::other("Attribute value exceeds its buffer"))?;
        // SAFETY: the attribute name and value remain live for the call; an empty
        // value is valid. flags=0 matches copy2's create-or-replace semantics.
        #[allow(unsafe_code)]
        let result = unsafe {
            libc::fsetxattr(
                destination.as_raw_fd(),
                name.as_ptr(),
                value.as_ptr().cast(),
                value.len(),
                0,
            )
        };
        if result < 0 {
            let error = io::Error::last_os_error();
            if !optional(&error, true) {
                return Err(error);
            }
        }
    }
    Ok(())
}
