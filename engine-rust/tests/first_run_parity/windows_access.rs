//! A scoped thread token makes a denied directory traversal observable.
//! The process token and any prior thread impersonation remain unchanged.
use std::{
    io,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
};
use windows_sys::Win32::{
    Foundation::{ERROR_NO_TOKEN, ERROR_SUCCESS, GetLastError, LUID, SetLastError},
    Security::{
        AdjustTokenPrivileges, DuplicateTokenEx, GetTokenInformation, LUID_AND_ATTRIBUTES,
        LookupPrivilegeValueW, SE_CHANGE_NOTIFY_NAME, SECURITY_IMPERSONATION_LEVEL,
        SecurityImpersonation, TOKEN_ADJUST_PRIVILEGES, TOKEN_DUPLICATE, TOKEN_IMPERSONATE,
        TOKEN_PRIVILEGES, TOKEN_QUERY, TokenImpersonation, TokenImpersonationLevel,
    },
    System::Threading::{
        GetCurrentProcess, GetCurrentThread, OpenProcessToken, OpenThreadToken, SetThreadToken,
    },
};

struct ThreadToken {
    prior: Option<OwnedHandle>,
    _restricted: OwnedHandle,
    active: bool,
}

impl ThreadToken {
    #[allow(unsafe_code)]
    fn install() -> io::Result<Self> {
        let mut source = std::ptr::null_mut();
        // SAFETY: a live current-thread pseudo-handle and initialized output.
        let prior = if unsafe {
            OpenThreadToken(
                GetCurrentThread(),
                TOKEN_QUERY | TOKEN_DUPLICATE | TOKEN_IMPERSONATE,
                1,
                &mut source,
            )
        } != 0
        {
            // SAFETY: the successful API transferred one owned token handle.
            Some(unsafe { OwnedHandle::from_raw_handle(source) })
        } else {
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(ERROR_NO_TOKEN as i32) {
                return Err(error);
            }
            None
        };
        let process;
        let source = if let Some(prior) = &prior {
            prior.as_raw_handle()
        } else {
            // SAFETY: initialized output and a live current-process handle.
            if unsafe {
                OpenProcessToken(
                    GetCurrentProcess(),
                    TOKEN_QUERY | TOKEN_DUPLICATE,
                    &mut source,
                )
            } == 0
            {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: the successful API transferred one owned token handle.
            process = unsafe { OwnedHandle::from_raw_handle(source) };
            process.as_raw_handle()
        };
        let mut level: SECURITY_IMPERSONATION_LEVEL = SecurityImpersonation;
        if prior.is_some() {
            let mut returned = 0;
            // SAFETY: source stays owned above; level is an initialized buffer
            // with its exact size. Preserve the prior token's level as well.
            if unsafe {
                GetTokenInformation(
                    source,
                    TokenImpersonationLevel,
                    (&mut level as *mut SECURITY_IMPERSONATION_LEVEL).cast(),
                    std::mem::size_of_val(&level) as u32,
                    &mut returned,
                )
            } == 0
            {
                return Err(io::Error::last_os_error());
            }
            if returned as usize != std::mem::size_of_val(&level) {
                return Err(io::Error::other("Thread token level has an invalid size"));
            }
        }
        let mut restricted = std::ptr::null_mut();
        // SAFETY: the live source is only duplicated, with no extra privileges
        // or increased impersonation level; null security attributes are valid.
        if unsafe {
            DuplicateTokenEx(
                source,
                TOKEN_QUERY | TOKEN_ADJUST_PRIVILEGES | TOKEN_IMPERSONATE,
                std::ptr::null(),
                level,
                TokenImpersonation,
                &mut restricted,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: DuplicateTokenEx succeeded and transferred this handle.
        let restricted = unsafe { OwnedHandle::from_raw_handle(restricted) };
        let mut luid = LUID {
            LowPart: 0,
            HighPart: 0,
        };
        // SAFETY: a static terminated privilege name and initialized output.
        if unsafe { LookupPrivilegeValueW(std::ptr::null(), SE_CHANGE_NOTIFY_NAME, &mut luid) } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let privileges = TOKEN_PRIVILEGES {
            PrivilegeCount: 1,
            Privileges: [LUID_AND_ATTRIBUTES {
                Luid: luid,
                Attributes: 0,
            }],
        };
        // SAFETY: only the newly owned duplicate is modified, and Attributes=0
        // disables its existing privilege. No process or prior token is changed.
        unsafe { SetLastError(ERROR_SUCCESS) };
        if unsafe {
            AdjustTokenPrivileges(
                restricted.as_raw_handle(),
                0,
                &privileges,
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        // AdjustTokenPrivileges can succeed while failing to assign a privilege.
        if unsafe { GetLastError() } != ERROR_SUCCESS {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: null selects only this thread; the owned token is impersonation.
        if unsafe { SetThreadToken(std::ptr::null(), restricted.as_raw_handle()) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            prior,
            _restricted: restricted,
            active: true,
        })
    }

    #[allow(unsafe_code)]
    fn restore(&mut self) -> io::Result<()> {
        if !self.active {
            return Ok(());
        }
        let token = self
            .prior
            .as_ref()
            .map_or(std::ptr::null_mut(), AsRawHandle::as_raw_handle);
        // SAFETY: null selects this thread; the exact prior token remains owned.
        // A null token restores the original absence of thread impersonation.
        if unsafe { SetThreadToken(std::ptr::null(), token) } == 0 {
            return Err(io::Error::last_os_error());
        }
        self.active = false;
        Ok(())
    }
}

impl Drop for ThreadToken {
    fn drop(&mut self) {
        if self.restore().is_err() {
            // Do not replace an operation's panic payload. A normal restoration
            // failure is propagated by without_traversal_bypass before Drop.
            eprintln!("Owned first-run thread token could not be restored");
        }
    }
}

pub(super) fn without_traversal_bypass<T>(
    operation: impl FnOnce() -> io::Result<T>,
) -> io::Result<T> {
    let mut token = ThreadToken::install()?;
    let result = operation();
    token.restore()?;
    result
}
