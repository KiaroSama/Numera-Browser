use super::manifest;
use std::{io, path::Path};
#[cfg(unix)]
pub fn validate(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::symlink_metadata(path)?;
    // SAFETY: geteuid has no borrowed pointers or side effects.
    if !meta.is_dir()
        || super::files::linked(&meta)
        || meta.mode() & 0o077 != 0
        || meta.uid() != unsafe { libc::geteuid() }
    {
        return Err(manifest::error(
            "Update recovery directory is not private to the current user",
        ));
    }
    Ok(())
}
#[cfg(windows)]
pub fn validate(path: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::{
        Foundation::{LocalFree, ERROR_SUCCESS},
        Security::{
            self,
            Authorization::{GetNamedSecurityInfoW, SE_FILE_OBJECT},
            WinCreatorOwnerRightsSid, WinLocalSystemSid, ACCESS_ALLOWED_ACE,
            DACL_SECURITY_INFORMATION, SE_DACL_PROTECTED,
        },
    };
    let meta = std::fs::symlink_metadata(path)?;
    if !meta.is_dir() || super::files::linked(&meta) {
        return Err(manifest::error("Update recovery directory is linked"));
    }
    let name: Vec<_> = path.as_os_str().encode_wide().chain([0]).collect();
    let mut acl = std::ptr::null_mut();
    let mut descriptor = std::ptr::null_mut();
    // SAFETY: terminated path and initialized output pointers, descriptor owns ACL memory.
    let code = unsafe {
        GetNamedSecurityInfoW(
            name.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut acl,
            std::ptr::null_mut(),
            &mut descriptor,
        )
    };
    if code != ERROR_SUCCESS {
        return Err(io::Error::from_raw_os_error(code as i32));
    }
    let result = (|| -> io::Result<()> {
        let mut control = 0;
        let mut revision = 0;
        if acl.is_null()
            || unsafe {
                Security::GetSecurityDescriptorControl(descriptor, &mut control, &mut revision)
            } == 0
            || control & SE_DACL_PROTECTED == 0
        {
            return Err(manifest::error(
                "Update recovery access controls are not protected",
            ));
        }
        let count = unsafe { (*acl).AceCount };
        if count == 0 || count > 8 {
            return Err(manifest::error("Unexpected update recovery access list"));
        }
        let mut owner = false;
        let mut system = false;
        for index in 0..u32::from(count) {
            let mut ace = std::ptr::null_mut();
            if unsafe { Security::GetAce(acl, index, &mut ace) } == 0 || ace.is_null() {
                return Err(io::Error::last_os_error());
            }
            let allowed = unsafe { &*ace.cast::<ACCESS_ALLOWED_ACE>() };
            if allowed.Header.AceType != 0
                || usize::from(allowed.Header.AceSize) < std::mem::size_of::<ACCESS_ALLOWED_ACE>()
            {
                return Err(manifest::error("Unexpected update recovery access entry"));
            }
            let sid = std::ptr::addr_of!(allowed.SidStart).cast_mut().cast();
            if unsafe { Security::IsWellKnownSid(sid, WinCreatorOwnerRightsSid) } != 0 {
                owner = true;
            } else if unsafe { Security::IsWellKnownSid(sid, WinLocalSystemSid) } != 0 {
                system = true;
            } else {
                return Err(manifest::error(
                    "Update recovery directory grants another principal access",
                ));
            }
        }
        if !owner || !system {
            return Err(manifest::error("Update recovery owner access missing"));
        }
        Ok(())
    })();
    // SAFETY: GetNamedSecurityInfo allocates this single descriptor with LocalAlloc.
    unsafe {
        LocalFree(descriptor);
    }
    result
}
