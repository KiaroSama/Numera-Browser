#![allow(unsafe_code)]
use std::{
    io, mem,
    os::windows::io::{AsHandle, AsRawHandle, BorrowedHandle, FromRawHandle, OwnedHandle},
    process::Child,
    ptr,
    time::Instant,
};
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, ERROR_NO_MORE_FILES, HANDLE, INVALID_HANDLE_VALUE, WAIT_FAILED, WAIT_OBJECT_0,
        WAIT_TIMEOUT,
    },
    System::{
        Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
            TH32CS_SNAPPROCESS, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
        },
        JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, IsProcessInJob,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOBOBJECT_BASIC_ACCOUNTING_INFORMATION,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectBasicAccountingInformation,
            JobObjectBasicProcessIdList, JobObjectExtendedLimitInformation, OpenJobObjectW,
            QueryInformationJobObject, SetInformationJobObject, TerminateJobObject,
        },
        SystemServices::{JOB_OBJECT_QUERY, JOB_OBJECT_TERMINATE},
        Threading::{
            GetCurrentProcess, OpenProcess, OpenThread, PROCESS_QUERY_LIMITED_INFORMATION,
            PROCESS_SYNCHRONIZE, QueryFullProcessImageNameW, ResumeThread, THREAD_SUSPEND_RESUME,
            WaitForSingleObject,
        },
    },
};

pub struct Job(OwnedHandle, String);

struct ProcessWitness {
    pid: u32,
    handle: OwnedHandle,
}
#[derive(Default)]
pub(super) struct CleanupWitnesses(Vec<ProcessWitness>);
fn cleanup_before(deadline: Instant) -> io::Result<()> {
    if Instant::now() >= deadline {
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "Windows cleanup exceeded its deadline",
        ))
    } else {
        Ok(())
    }
}
fn process_signaled(process: HANDLE) -> io::Result<bool> {
    // SAFETY: caller retains the process handle; zero timeout never waits.
    match unsafe { WaitForSingleObject(process, 0) } {
        WAIT_OBJECT_0 => Ok(true),
        WAIT_TIMEOUT => Ok(false),
        WAIT_FAILED => Err(io::Error::last_os_error()),
        _ => Err(io::Error::other("Process exit observation is incomplete")),
    }
}
impl CleanupWitnesses {
    pub(super) fn retain_members(
        &mut self,
        job: &Job,
        leader: &Child,
        deadline: Instant,
    ) -> io::Result<()> {
        cleanup_before(deadline)?;
        let members = job.members()?;
        cleanup_before(deadline)?;
        // Allow an old terminating generation plus one complete current list.
        // Refuse before opening more handles rather than truncate a proof.
        let additional = members
            .iter()
            .filter(|pid| **pid != leader.id() && !self.0.iter().any(|known| known.pid == **pid))
            .count();
        if self.0.len().saturating_add(additional) > super::memory::MAX_PROCESSES * 2 {
            return Err(io::Error::other("Retained cleanup witness bound exceeded"));
        }
        for pid in members {
            cleanup_before(deadline)?;
            if pid == leader.id() || self.0.iter().any(|known| known.pid == pid) {
                // Retain and reuse that original verified process object; do
                // not reopen an exiting member by PID on a cleanup retry.
                continue;
            }
            // SAFETY: passive, non-inheritable rights; no per-PID signal power.
            let raw = unsafe {
                OpenProcess(
                    PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                    0,
                    pid,
                )
            };
            if raw.is_null() {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: successful OpenProcess transfers a CloseHandle owner.
            let handle = unsafe { OwnedHandle::from_raw_handle(raw) };
            cleanup_before(deadline)?;
            let mut member = 0;
            // SAFETY: both retained handles and initialized output remain live.
            if unsafe { IsProcessInJob(handle.as_raw_handle(), job.0.as_raw_handle(), &mut member) }
                == 0
            {
                return Err(io::Error::last_os_error());
            }
            if member == 0 {
                return Err(io::Error::other(
                    "Cleanup process is not in the retained Job",
                ));
            }
            // The retained kernel handle is the process identity. Keep the
            // verified witness even if the next deadline check refuses.
            self.0.push(ProcessWitness { pid, handle });
            // Save a newly verified witness before checking an elapsed budget.
            cleanup_before(deadline)?;
        }
        Ok(())
    }
    pub(super) fn all_signaled(&self, deadline: Instant) -> io::Result<bool> {
        let mut all = true;
        for witness in &self.0 {
            cleanup_before(deadline)?;
            all &= process_signaled(witness.handle.as_raw_handle())?;
            cleanup_before(deadline)?;
        }
        Ok(all)
    }
    pub(super) fn clear_proved(&mut self) {
        self.0.clear();
    }
}
fn image_class(path: &str) -> &'static str {
    let name = path.rsplit(['\\', '/']).next().unwrap_or_default();
    match name.to_ascii_lowercase().as_str() {
        "cargo.exe" => "cargo",
        "cargo-fmt.exe" => "cargo-fmt",
        "rustc.exe" => "rustc",
        "rustfmt.exe" => "rustfmt",
        "lld-link.exe" => "lld-link",
        "link.exe" => "link",
        "mspdbsrv.exe" => "mspdbsrv",
        "vctip.exe" => "vctip",
        "conhost.exe" => "conhost",
        "cmd.exe" => "cmd",
        "powershell.exe" | "pwsh.exe" => "powershell",
        "rustup.exe" => "rustup",
        "node.exe" => "node",
        "numera-engine.exe" => "numera-engine",
        "numera-browser-runtime.exe" => "numera-browser-runtime",
        "numera-xtask.exe" => "numera-xtask",
        "numera-native-candidate.exe" => "numera-native-candidate",
        "numera-http-observer.exe" => "numera-http-observer",
        _ => "unclassified",
    }
}
fn snapshot_image_class(pid: u32) -> io::Result<Option<&'static str>> {
    // This is diagnostic only. The caller still holds the original process
    // handle and has verified its retained Job membership, preventing PID reuse
    // while this separate snapshot supplies only a static basename class.
    // SAFETY: a read-only process snapshot has no caller-provided pointers.
    let raw = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if raw == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: successful snapshot creation transfers one CloseHandle owner.
    let snapshot = unsafe { OwnedHandle::from_raw_handle(raw) };
    // SAFETY: PROCESSENTRY32W is a plain integer/UTF-16 record; initialize its
    // required size before passing the writable bounded record to Toolhelp.
    let mut entry: PROCESSENTRY32W = unsafe { mem::zeroed() };
    entry.dwSize = mem::size_of::<PROCESSENTRY32W>() as u32;
    // SAFETY: snapshot is live and entry has the exact declared record size.
    let mut present = unsafe { Process32FirstW(snapshot.as_raw_handle(), &mut entry) };
    for _ in 0..65536 {
        if present == 0 {
            let error = io::Error::last_os_error();
            return if error.raw_os_error() == Some(ERROR_NO_MORE_FILES as i32) {
                Ok(None)
            } else {
                Err(error)
            };
        }
        if entry.th32ProcessID == pid {
            let length = entry
                .szExeFile
                .iter()
                .position(|value| *value == 0)
                .unwrap_or(entry.szExeFile.len());
            return Ok(Some(image_class(&String::from_utf16_lossy(
                &entry.szExeFile[..length],
            ))));
        }
        // SAFETY: the same owned snapshot and initialized record remain live.
        present = unsafe { Process32NextW(snapshot.as_raw_handle(), &mut entry) };
    }
    Err(io::Error::other(
        "Process diagnostic snapshot exceeds its bound",
    ))
}
fn job_name() -> io::Result<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|e| io::Error::other(e.to_string()))?;
    Ok(format!(
        "numera-native-{}",
        bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
    ))
}
impl Job {
    pub(crate) fn handle(&self) -> BorrowedHandle<'_> {
        self.0.as_handle()
    }
    pub fn name(&self) -> &str {
        &self.1
    }
    pub fn new() -> io::Result<Self> {
        #[cfg(feature = "test-support")]
        if let Some(result) = super::test_support::job_create() {
            result?;
        }
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
        #[cfg(feature = "test-support")]
        if let Some(result) = super::test_support::job_assign(child.id()) {
            result?;
        }
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
        Ok(self.accounting()?.ActiveProcesses == 0)
    }
    pub fn complete_empty(&self) -> io::Result<bool> {
        Ok(self.members()?.is_empty() && self.accounting()?.ActiveProcesses == 0)
    }
    fn accounting(&self) -> io::Result<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION> {
        #[cfg(feature = "test-support")]
        if let Some(result) = super::test_support::job_query() {
            result?;
        }
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
        Ok(info)
    }
    pub fn peak_memory(&self) -> io::Result<u64> {
        // SAFETY: retain this job handle after child reaping; the kernel keeps
        // lifetime peak commit for this exact job, rather than a sampled RSS.
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { mem::zeroed() };
        if unsafe {
            QueryInformationJobObject(
                self.0.as_raw_handle(),
                JobObjectExtendedLimitInformation,
                ptr::addr_of_mut!(info).cast(),
                mem::size_of_val(&info) as u32,
                ptr::null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        u64::try_from(info.PeakJobMemoryUsed).map_err(io::Error::other)
    }
    /// Read only: every member in a complete snapshot must already be signaled.
    /// This does not prove an empty Job and never permits accepting a survivor.
    pub fn all_members_exited(&self, leader: &Child) -> io::Result<bool> {
        let members = self.members()?;
        for pid in members {
            // Keep the original leader handle: exited processes may disappear
            // from OpenProcess before the Job's accounting finishes updating.
            let queried;
            let process = if pid == leader.id() {
                leader.as_raw_handle() as HANDLE
            } else {
                // SAFETY: passive, non-inheritable query/synchronize handle.
                // A recycled or foreign PID is rejected against our exact Job.
                let raw = unsafe {
                    OpenProcess(
                        PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                        0,
                        pid,
                    )
                };
                if raw.is_null() {
                    return Err(io::Error::last_os_error());
                }
                // SAFETY: successful OpenProcess transfers one owned handle.
                queried = unsafe { OwnedHandle::from_raw_handle(raw) };
                let mut contained = 0;
                if unsafe {
                    IsProcessInJob(
                        queried.as_raw_handle(),
                        self.0.as_raw_handle(),
                        &mut contained,
                    )
                } == 0
                {
                    return Err(io::Error::last_os_error());
                }
                if contained == 0 {
                    return Err(io::Error::other(
                        "Observed process no longer belongs to the retained Job",
                    ));
                }
                queried.as_raw_handle()
            };
            // SAFETY: either a borrowed retained child or the live query owner
            // above. Zero timeout observes state without waiting or signaling.
            match unsafe { WaitForSingleObject(process, 0) } {
                WAIT_OBJECT_0 => {}
                WAIT_TIMEOUT => return Ok(false),
                WAIT_FAILED => return Err(io::Error::last_os_error()),
                _ => {
                    return Err(io::Error::other(
                        "Retained Job member exit observation is incomplete",
                    ));
                }
            }
        }
        Ok(true)
    }
    fn members(&self) -> io::Result<Vec<u32>> {
        #[cfg(feature = "test-support")]
        if let Some(result) = super::test_support::job_query() {
            result?;
        }
        let mut buffer = vec![
            0usize;
            (8 + super::memory::MAX_PROCESSES * mem::size_of::<usize>())
                .div_ceil(mem::size_of::<usize>())
        ];
        let size = buffer.len() * mem::size_of::<usize>();
        let mut returned = 0;
        // SAFETY: initialized, pointer-aligned output for this retained Job;
        // no process or Job is reopened to establish tree ownership.
        if unsafe {
            QueryInformationJobObject(
                self.0.as_raw_handle(),
                JobObjectBasicProcessIdList,
                buffer.as_mut_ptr().cast(),
                size as u32,
                &mut returned,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: the initialized allocation bounds this temporary byte view.
        let bytes = unsafe { std::slice::from_raw_parts(buffer.as_ptr().cast::<u8>(), size) };
        super::memory::members_from_buffer(bytes, returned as usize, mem::size_of::<usize>())
    }
    /// Passive, bounded diagnostic evidence. Never used as an ownership proof.
    /// Paths, arguments and arbitrary executable names cannot escape this API.
    pub fn member_diagnostics(&self, leader: &Child) -> io::Result<serde_json::Value> {
        let members = self.members()?;
        let accounting = match self.accounting() {
            Ok(info) => serde_json::json!({
                "active_processes":info.ActiveProcesses,
                "total_processes":info.TotalProcesses,
                "total_terminated_processes":info.TotalTerminatedProcesses
            }),
            Err(error) => serde_json::json!({"error_code":error.raw_os_error()}),
        };
        let mut rows = Vec::new();
        for pid in members {
            let is_leader = pid == leader.id();
            let mut row = serde_json::json!({
                "pid":pid, "leader":is_leader, "membership":"unknown",
                "wait":"unknown", "image_class":"unclassified"
            });
            let queried;
            let process = if is_leader {
                row["membership"] = "retained_leader".into();
                leader.as_raw_handle() as HANDLE
            } else {
                // SAFETY: passive and non-inheritable. Membership is checked
                // against the retained Job before inspecting this process.
                let raw = unsafe {
                    OpenProcess(
                        PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                        0,
                        pid,
                    )
                };
                if raw.is_null() {
                    row["open_error_code"] = io::Error::last_os_error().raw_os_error().into();
                    rows.push(row);
                    continue;
                }
                // SAFETY: the successful call transfers a CloseHandle owner.
                queried = unsafe { OwnedHandle::from_raw_handle(raw) };
                let mut contained = 0;
                if unsafe {
                    IsProcessInJob(
                        queried.as_raw_handle(),
                        self.0.as_raw_handle(),
                        &mut contained,
                    )
                } == 0
                {
                    row["membership_error_code"] = io::Error::last_os_error().raw_os_error().into();
                    rows.push(row);
                    continue;
                }
                if contained == 0 {
                    row["membership"] = "foreign".into();
                    rows.push(row);
                    continue;
                }
                row["membership"] = "verified".into();
                queried.as_raw_handle()
            };
            // SAFETY: the retained leader or the scoped owned query handle is
            // still live. A zero timeout neither waits nor signals the member.
            match unsafe { WaitForSingleObject(process, 0) } {
                WAIT_OBJECT_0 => row["wait"] = "exited".into(),
                WAIT_TIMEOUT => row["wait"] = "live".into(),
                WAIT_FAILED => {
                    row["wait_error_code"] = io::Error::last_os_error().raw_os_error().into();
                }
                status => row["wait_status_code"] = status.into(),
            }
            let mut image = vec![0u16; 4096];
            let mut length = image.len() as u32;
            // SAFETY: initialized bounded UTF-16 output and a retained process
            // handle. Only the static classification is returned to callers.
            if unsafe { QueryFullProcessImageNameW(process, 0, image.as_mut_ptr(), &mut length) }
                != 0
            {
                if let Some(image) = image.get(..length as usize)
                    && let Ok(name) = String::from_utf16(image)
                {
                    row["image_class"] = image_class(&name).into();
                    row["image_source"] = "retained-process-handle".into();
                }
            } else {
                row["image_error_code"] = io::Error::last_os_error().raw_os_error().into();
                match snapshot_image_class(pid) {
                    Ok(Some(class)) => {
                        row["image_class"] = class.into();
                        row["image_source"] = "process-snapshot".into();
                    }
                    Ok(None) => row["image_source"] = "process-snapshot-no-match".into(),
                    Err(error) => {
                        row["image_source"] = "unavailable".into();
                        row["image_snapshot_error_code"] = error.raw_os_error().into();
                    }
                }
            }
            rows.push(row);
        }
        Ok(serde_json::json!({
            "complete":true, "member_count":rows.len(), "members":rows,
            "accounting":accounting,
            "observation_order":"member_ids_then_accounting_then_member_states"
        }))
    }
    pub fn terminate(&self) -> io::Result<()> {
        // SAFETY: this job contains only the tree this owner spawned.
        if unsafe { TerminateJobObject(self.0.as_raw_handle(), 1) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

/// Query an existing named Job for current-process membership, without adoption.
pub fn named_contains_current(name: &str) -> io::Result<bool> {
    if name.is_empty() || name.contains('\0') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Job name is invalid",
        ));
    }
    let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
    // SAFETY: bounded live Unicode name, query-only access, no inheritance.
    let raw = unsafe { OpenJobObjectW(JOB_OBJECT_QUERY, 0, wide.as_ptr()) };
    if raw.is_null() {
        let error = io::Error::last_os_error();
        return if matches!(error.raw_os_error(), Some(2 | 3)) {
            Ok(false)
        } else {
            Err(error)
        };
    }
    // SAFETY: a successful open transfers one CloseHandle owner.
    let job = unsafe { OwnedHandle::from_raw_handle(raw) };
    let mut contained = 0;
    // SAFETY: the pseudo process handle is borrowed; the Job and output live.
    if unsafe { IsProcessInJob(GetCurrentProcess(), job.as_raw_handle(), &mut contained) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(contained != 0)
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

pub fn settle_named(name: &str, end: std::time::Instant, pid: Option<u32>) -> io::Result<bool> {
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
    let empty = || -> io::Result<bool> {
        #[cfg(feature = "test-support")]
        if let Some(pid) = pid.and_then(|pid| i32::try_from(pid).ok())
            && let Some(result) = super::test_support::group_empty(pid)
        {
            return result;
        }
        job.empty()
    };
    #[cfg(not(feature = "test-support"))]
    let _ = pid;
    if !empty()? {
        #[cfg(feature = "test-support")]
        match pid
            .and_then(|pid| i32::try_from(pid).ok())
            .and_then(|pid| super::test_support::signal(-pid, true))
        {
            Some(result) => result?,
            None => job.terminate()?,
        }
        #[cfg(not(feature = "test-support"))]
        job.terminate()?;
    }
    loop {
        if empty()? {
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
