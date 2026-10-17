#![allow(unsafe_code)]
use std::{
    io, mem,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    process::Child,
    ptr,
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, ERROR_NO_MORE_FILES, HANDLE, INVALID_HANDLE_VALUE},
    System::{
        Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
        },
        JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JobObjectBasicAccountingInformation, JobObjectExtendedLimitInformation, OpenJobObjectW,
            QueryInformationJobObject, SetInformationJobObject, TerminateJobObject,
        },
        SystemServices::{JOB_OBJECT_QUERY, JOB_OBJECT_TERMINATE},
        Threading::{OpenThread, ResumeThread, THREAD_SUSPEND_RESUME},
    },
};

pub struct Job(OwnedHandle, String);
fn job_name() -> io::Result<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|e| io::Error::other(e.to_string()))?;
    Ok(format!(
        "numera-native-{}",
        bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
    ))
}
impl Job {
    pub fn name(&self) -> &str {
        &self.1
    }
    pub fn new() -> io::Result<Self> {
        // SAFETY: private unique name permits durable local queries; handle
        // stays non-inheritable and kill-on-close ends this owner's descendants.
        let name = job_name()?;
        let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
        let handle = unsafe { CreateJobObjectW(ptr::null(), wide.as_ptr()) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: non-null uniquely owned job handle requires CloseHandle;
        // OwnedHandle supplies that lifetime and safe Send/Sync semantics.
        let job = Self(unsafe { OwnedHandle::from_raw_handle(handle) }, name);
        // SAFETY: Win32 POD structure initialized before passing exact size.
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { mem::zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        // SAFETY: valid owned handle; read-only buffer lives through the call.
        if unsafe {
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                ptr::addr_of!(limits).cast(),
                mem::size_of_val(&limits) as u32,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(job)
    }
    pub fn assign(&self, child: &Child) -> io::Result<()> {
        // SAFETY: both handles are live and the child is still suspended.
        if unsafe {
            AssignProcessToJobObject(self.0.as_raw_handle(), child.as_raw_handle() as HANDLE)
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
    pub fn empty(&self) -> io::Result<bool> {
        // SAFETY: POD output initialized, exact type/size for accounting class.
        let mut info: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = unsafe { mem::zeroed() };
        if unsafe {
            QueryInformationJobObject(
                self.0.as_raw_handle(),
                JobObjectBasicAccountingInformation,
                ptr::addr_of_mut!(info).cast(),
                mem::size_of_val(&info) as u32,
                ptr::null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(info.ActiveProcesses == 0)
    }
    pub fn terminate(&self) -> io::Result<()> {
        // SAFETY: this job contains only the tree this owner spawned.
        if unsafe { TerminateJobObject(self.0.as_raw_handle(), 1) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

pub fn named_empty(name: &str) -> io::Result<bool> {
    if name.contains('\0') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Job name contains a NUL character",
        ));
    }
    let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
    // SAFETY: NUL-terminated Unicode name; read-only query access, no inheritance.
    let handle = unsafe { OpenJobObjectW(JOB_OBJECT_QUERY, 0, wide.as_ptr()) };
    if handle.is_null() {
        let error = io::Error::last_os_error();
        if matches!(error.raw_os_error(), Some(2 | 3)) {
            return Ok(true);
        }
        return Err(error);
    }
    // SAFETY: OpenJobObjectW returned a new owned CloseHandle-compatible handle.
    let job = Job(unsafe { OwnedHandle::from_raw_handle(handle) }, name.into());
    job.empty()
}

pub fn settle_named(name: &str, end: std::time::Instant) -> io::Result<bool> {
    if name.is_empty() || name.contains('\0') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Stored job name is invalid",
        ));
    }
    let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
    // SAFETY: validated NUL-terminated name, no inheritance; retain this exact
    // kernel object through termination and proof, never reopen by a recycled PID.
    let handle =
        unsafe { OpenJobObjectW(JOB_OBJECT_QUERY | JOB_OBJECT_TERMINATE, 0, wide.as_ptr()) };
    if handle.is_null() {
        let error = io::Error::last_os_error();
        return if matches!(error.raw_os_error(), Some(2 | 3)) {
            Ok(true)
        } else {
            Err(error)
        };
    }
    // SAFETY: successful OpenJobObjectW supplies one uniquely owned handle.
    let job = Job(unsafe { OwnedHandle::from_raw_handle(handle) }, name.into());
    if !job.empty()? {
        job.terminate()?;
    }
    loop {
        if job.empty()? {
            return Ok(true);
        }
        if std::time::Instant::now() >= end {
            return Ok(false);
        }
        std::thread::sleep(
            end.saturating_duration_since(std::time::Instant::now())
                .min(std::time::Duration::from_millis(20)),
        );
    }
}

pub fn resume(pid: u32) -> io::Result<()> {
    // SAFETY: read-only kernel thread snapshot, handle closed on all paths.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    let result = (|| {
        // SAFETY: required POD entry size set before enumeration.
        let mut entry: THREADENTRY32 = unsafe { mem::zeroed() };
        entry.dwSize = mem::size_of_val(&entry) as u32;
        let mut present = unsafe { Thread32First(snapshot, &mut entry) } != 0;
        let mut resumed = false;
        while present {
            if entry.th32OwnerProcessID == pid {
                // SAFETY: targeted thread from snapshot; no other access rights.
                let handle = unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID) };
                if handle.is_null() {
                    return Err(io::Error::last_os_error());
                }
                let count = unsafe { ResumeThread(handle) };
                let error = if count == u32::MAX {
                    Some(io::Error::last_os_error())
                } else {
                    None
                };
                unsafe { CloseHandle(handle) };
                if let Some(error) = error {
                    return Err(error);
                }
                resumed = true;
            }
            present = unsafe { Thread32Next(snapshot, &mut entry) } != 0;
        }
        let error = io::Error::last_os_error();
        if !resumed {
            return Err(io::Error::other("Suspended child has no resumable thread"));
        }
        if error.raw_os_error() != Some(ERROR_NO_MORE_FILES as i32) {
            return Err(error);
        }
        Ok(())
    })();
    unsafe { CloseHandle(snapshot) };
    result
}
