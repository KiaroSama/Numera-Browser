use std::io;

/// None means absent; an error is unknown, never authority to signal a PID.
#[cfg(target_os = "linux")]
pub fn started_at(pid: u32) -> io::Result<Option<u64>> {
    let text = match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(value) => value,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let (_, fields) = text
        .rsplit_once(") ")
        .ok_or_else(|| io::Error::other("Process stat has no closing command field"))?;
    fields
        .split_whitespace()
        .nth(19)
        .ok_or_else(|| io::Error::other("Process stat has no birth field"))?
        .parse::<u64>()
        .map(Some)
        .map_err(io::Error::other)
}

#[cfg(windows)]
#[allow(unsafe_code)]
pub fn started_at(pid: u32) -> io::Result<Option<u64>> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, ERROR_INVALID_PARAMETER, FILETIME},
        System::Threading::{GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
    };
    // SAFETY: fixed OS capability, valid scalar PID; never a borrowed pointer.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(ERROR_INVALID_PARAMETER as i32) {
            return Ok(None);
        }
        return Err(error);
    }
    let mut created = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let mut exit = created;
    let mut kernel = created;
    let mut user = created;
    // SAFETY: owned live process handle and four distinct initialized FILETIMEs.
    let ok = unsafe { GetProcessTimes(handle, &mut created, &mut exit, &mut kernel, &mut user) };
    let error = if ok == 0 {
        Some(io::Error::last_os_error())
    } else {
        None
    };
    // SAFETY: only this function owns the handle; close on either result path.
    unsafe { CloseHandle(handle) };
    if let Some(error) = error {
        return Err(error);
    }
    Ok(Some(
        (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime),
    ))
}

#[cfg(windows)]
#[allow(unsafe_code)]
pub fn alive(pid: u32, token: Option<u64>) -> io::Result<bool> {
    use windows_sys::Win32::{
        Foundation::{
            CloseHandle, ERROR_INVALID_PARAMETER, FILETIME, WAIT_FAILED, WAIT_OBJECT_0,
            WAIT_TIMEOUT,
        },
        System::Threading::{
            GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
            WaitForSingleObject,
        },
    };
    if pid == 0 {
        return Ok(false);
    }
    // SAFETY: query/synchronization only. Keep one handle for birth and exit
    // checks so a recycled PID cannot enter between the observations.
    let handle = unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
            0,
            pid,
        )
    };
    if handle.is_null() {
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(ERROR_INVALID_PARAMETER as i32) {
            return Ok(false);
        }
        return Err(error);
    }
    let result = (|| {
        if let Some(token) = token {
            let mut created = FILETIME {
                dwLowDateTime: 0,
                dwHighDateTime: 0,
            };
            let mut exit = created;
            let mut kernel = created;
            let mut user = created;
            // SAFETY: owned live handle and four distinct initialized outputs.
            if unsafe { GetProcessTimes(handle, &mut created, &mut exit, &mut kernel, &mut user) }
                == 0
            {
                return Err(io::Error::last_os_error());
            }
            let actual =
                (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime);
            if actual != token {
                return Ok(false);
            }
        }
        // SAFETY: zero wait returns immediately, handle retained until return.
        match unsafe { WaitForSingleObject(handle, 0) } {
            WAIT_OBJECT_0 => Ok(false),
            WAIT_TIMEOUT => Ok(true),
            WAIT_FAILED => Err(io::Error::last_os_error()),
            _ => Err(io::Error::other(
                "Unexpected process synchronization verdict",
            )),
        }
    })();
    // SAFETY: this function uniquely owns the query handle on every path.
    unsafe { CloseHandle(handle) };
    result
}

#[cfg(target_os = "linux")]
#[allow(unsafe_code)]
pub fn alive(pid: u32, token: Option<u64>) -> io::Result<bool> {
    if pid == 0 {
        return Ok(false);
    }
    if let Some(token) = token {
        match started_at(pid)? {
            Some(actual) if actual == token => {}
            _ => return Ok(false),
        }
    }
    let pid = i32::try_from(pid).map_err(io::Error::other)?;
    // SAFETY: signal zero probes existence only; never terminates a process.
    if unsafe { libc::kill(pid, 0) } == 0 {
        return Ok(true);
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ESRCH) {
        return Ok(false);
    }
    Err(error)
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn alive(_pid: u32, _token: Option<u64>) -> io::Result<bool> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Native process liveness has not been implemented for this platform",
    ))
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn started_at(pid: u32) -> io::Result<Option<u64>> {
    let _ = pid;
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Native process identity has not been implemented for this platform",
    ))
}
