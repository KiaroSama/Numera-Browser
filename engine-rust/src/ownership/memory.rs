//! Read-only numeric memory observations of an already retained process tree.
//! Samples are diagnostics; they never replace lifetime job peak acceptance.
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
};
pub const MAX_PROCESSES: usize = 512;
/// Linux's largest waited child lifetime RSS. A fresh observer must see zero
/// before spawning; this metric is never presented as concurrent tree memory.
#[cfg(target_os = "linux")]
#[allow(unsafe_code)]
pub fn child_peak_rss_bytes() -> io::Result<u64> {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
    // SAFETY: getrusage initializes this owned rusage on success.
    if unsafe { libc::getrusage(libc::RUSAGE_CHILDREN, usage.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the successful call above initialized every field.
    let value = unsafe { usage.assume_init() }.ru_maxrss;
    u64::try_from(value)
        .ok()
        .and_then(|value| value.checked_mul(1024))
        .ok_or_else(|| io::Error::other("Linux lifetime RSS is invalid"))
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub pid: u32,
    pub birth: u64,
    pub counters: BTreeMap<String, u64>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub rows: Vec<Row>,
    pub missing: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JointSnapshot {
    pub snapshot: Snapshot,
    pub started_ns: u64,
    pub finished_ns: u64,
    pub peak_before_bytes: u64,
    pub peak_after_bytes: u64,
    pub membership_changed: bool,
}
pub const ACCOUNTING_COUNTERS: [&str; 11] = [
    "user_100ns",
    "kernel_100ns",
    "page_faults",
    "processes",
    "terminated_processes",
    "read_operations",
    "write_operations",
    "other_operations",
    "read_bytes",
    "write_bytes",
    "other_bytes",
];
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Accounting {
    pub counters: BTreeMap<String, u64>,
    pub active: u64,
}
pub fn accounting_from_bytes(bytes: &[u8], returned: usize) -> io::Result<Accounting> {
    if returned != 96 || bytes.len() < 96 {
        return Err(io::Error::other(
            "Historical accounting query is incomplete",
        ));
    }
    let i64_at = |offset| -> io::Result<u64> {
        u64::try_from(i64::from_le_bytes(
            bytes[offset..offset + 8]
                .try_into()
                .map_err(io::Error::other)?,
        ))
        .map_err(io::Error::other)
    };
    let u32_at = |offset| -> io::Result<u64> {
        Ok(u64::from(u32::from_le_bytes(
            bytes[offset..offset + 4]
                .try_into()
                .map_err(io::Error::other)?,
        )))
    };
    let mut values = vec![
        i64_at(0)?,
        i64_at(8)?,
        u32_at(32)?,
        u32_at(36)?,
        u32_at(44)?,
    ];
    for offset in (48..96).step_by(8) {
        values.push(u64::from_le_bytes(
            bytes[offset..offset + 8]
                .try_into()
                .map_err(io::Error::other)?,
        ));
    }
    let active = u32_at(40)?;
    if active > values[3] || values[4] > values[3] {
        return Err(io::Error::other(
            "Historical accounting counters are invalid",
        ));
    }
    Ok(Accounting {
        counters: ACCOUNTING_COUNTERS
            .into_iter()
            .map(str::to_owned)
            .zip(values)
            .collect(),
        active,
    })
}

pub fn members_from_buffer(bytes: &[u8], returned: usize, width: usize) -> io::Result<Vec<u32>> {
    if ![4, 8].contains(&width) || returned < 8 || returned > bytes.len() {
        return Err(io::Error::other(
            "Owned memory diagnostic job list is incomplete",
        ));
    }
    let assigned = u32::from_le_bytes(bytes[..4].try_into().map_err(io::Error::other)?) as usize;
    let count = u32::from_le_bytes(bytes[4..8].try_into().map_err(io::Error::other)?) as usize;
    if assigned != count || count > MAX_PROCESSES || returned < 8 + count * width {
        return Err(io::Error::other(
            "Owned memory diagnostic job list is truncated",
        ));
    }
    let mut result = Vec::with_capacity(count);
    let mut unique = BTreeSet::new();
    for index in 0..count {
        let raw = &bytes[8 + index * width..8 + (index + 1) * width];
        let pid = if width == 4 {
            u64::from(u32::from_le_bytes(
                raw.try_into().map_err(io::Error::other)?,
            ))
        } else {
            u64::from_le_bytes(raw.try_into().map_err(io::Error::other)?)
        };
        let pid = u32::try_from(pid)
            .map_err(|_| io::Error::other("Owned memory diagnostic job PID is invalid"))?;
        if pid == 0 || !unique.insert(pid) {
            return Err(io::Error::other(
                "Owned memory diagnostic job PID is invalid",
            ));
        }
        result.push(pid);
    }
    Ok(result)
}

/// Typed OS observation boundary. Production borrows the existing job; fault
/// tests can replace calls while keeping handle-close and interval semantics.
pub trait Backend {
    type Process;
    fn members(&mut self) -> io::Result<Vec<u32>>;
    fn open(&mut self, pid: u32) -> io::Result<Option<Self::Process>>;
    fn observe(&mut self, process: &Self::Process, pid: u32) -> io::Result<Option<Row>>;
    fn close(&mut self, process: Self::Process) -> io::Result<()>;
    fn peak(&mut self) -> io::Result<u64>;
    fn ticks(&mut self) -> io::Result<u64>;
    fn snapshot(&mut self) -> io::Result<Snapshot>
    where
        Self: Sized,
    {
        observe(self)
    }
}
pub fn observe(backend: &mut impl Backend) -> io::Result<Snapshot> {
    let members = backend.members()?;
    if members.len() > MAX_PROCESSES
        || members.contains(&0)
        || members.iter().collect::<BTreeSet<_>>().len() != members.len()
    {
        return Err(io::Error::other(
            "Owned memory diagnostic membership is invalid",
        ));
    }
    let mut snapshot = Snapshot {
        rows: Vec::new(),
        missing: 0,
    };
    for pid in members {
        let Some(handle) = backend.open(pid)? else {
            snapshot.missing += 1;
            continue;
        };
        let observed = backend.observe(&handle, pid);
        let closed = backend.close(handle);
        // Every opened process handle closes, including failed queries.
        closed?;
        match observed {
            Ok(Some(row)) if row.pid == pid && row.birth > 0 => snapshot.rows.push(row),
            _ => snapshot.missing += 1,
        }
    }
    snapshot.rows.sort_by_key(|row| (row.pid, row.birth));
    Ok(snapshot)
}
pub fn observe_joint(backend: &mut impl Backend) -> io::Result<JointSnapshot> {
    let started_ns = backend.ticks()?;
    let before = backend.members()?.into_iter().collect::<BTreeSet<_>>();
    let peak_before_bytes = backend.peak()?;
    let mut snapshot = backend.snapshot()?;
    let peak_after_bytes = backend.peak()?;
    let membership_changed = backend.members()?.into_iter().collect::<BTreeSet<_>>() != before;
    let finished_ns = backend.ticks()?;
    if finished_ns < started_ns || peak_after_bytes < peak_before_bytes {
        return Err(io::Error::other(
            "Owned memory diagnostic counters decreased",
        ));
    }
    snapshot.missing += u64::from(membership_changed);
    Ok(JointSnapshot {
        snapshot,
        started_ns,
        finished_ns,
        peak_before_bytes,
        peak_after_bytes,
        membership_changed,
    })
}

#[cfg(windows)]
mod windows {
    #![allow(unsafe_code)]
    use super::*;
    use std::{
        mem,
        os::windows::io::{AsRawHandle, BorrowedHandle},
        ptr,
        time::Instant,
    };
    use windows_sys::Win32::{
        Foundation::{CloseHandle, DuplicateHandle, FILETIME, HANDLE},
        System::{
            JobObjects::{
                IsProcessInJob, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
                JobObjectBasicAndIoAccountingInformation, JobObjectBasicProcessIdList,
                JobObjectExtendedLimitInformation, QueryInformationJobObject,
            },
            ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX},
            Threading::{
                GetCurrentProcess, GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
            },
        },
    };
    pub struct Native<'a> {
        job: BorrowedHandle<'a>,
        start: Instant,
    }
    pub struct Process(HANDLE);
    /// A query-only duplicate of the retained owner. Never opens a job by name
    /// or adopts a foreign/inherited string; explicit close failures stay fatal.
    pub struct AccountingQuery(HANDLE);
    impl AccountingQuery {
        pub fn new(tree: &super::super::tree::OwnedTree) -> io::Result<Self> {
            let mut query = ptr::null_mut();
            // SAFETY: source is borrowed from the live original owner; output
            // receives one non-inheritable handle with JOB_OBJECT_QUERY only.
            if unsafe {
                DuplicateHandle(
                    GetCurrentProcess(),
                    tree.job_handle().as_raw_handle(),
                    GetCurrentProcess(),
                    &mut query,
                    4,
                    0,
                    0,
                )
            } == 0
            {
                return Err(io::Error::last_os_error());
            }
            Ok(Self(query))
        }
        pub fn snapshot(&self) -> io::Result<Accounting> {
            let mut bytes = [0u64; 12];
            let mut returned = 0;
            // SAFETY: aligned initialized 96-byte accounting output; only the
            // query-only duplicate of the exact retained owner is inspected.
            if unsafe {
                QueryInformationJobObject(
                    self.0,
                    JobObjectBasicAndIoAccountingInformation,
                    bytes.as_mut_ptr().cast(),
                    96,
                    &mut returned,
                )
            } == 0
            {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: initialized array remains live and bounds the exact read.
            let raw = unsafe { std::slice::from_raw_parts(bytes.as_ptr().cast::<u8>(), 96) };
            accounting_from_bytes(raw, returned as usize)
        }
        pub fn finish(mut self) -> io::Result<()> {
            // SAFETY: sole owned duplicate; disarm Drop only after close succeeds.
            if unsafe { CloseHandle(self.0) } == 0 {
                return Err(io::Error::other(
                    "Historical resource query handle did not close",
                ));
            }
            self.0 = ptr::null_mut();
            Ok(())
        }
    }
    impl Drop for AccountingQuery {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe { CloseHandle(self.0) };
            }
        }
    }
    impl Drop for Process {
        fn drop(&mut self) {
            if !self.0.is_null() {
                // SAFETY: the query handle belongs only to this value and is retried on an explicit close failure.
                unsafe { CloseHandle(self.0) };
            }
        }
    }
    impl<'a> Native<'a> {
        pub fn new(tree: &'a super::super::tree::OwnedTree) -> Self {
            Self {
                job: tree.job_handle(),
                start: Instant::now(),
            }
        }
    }
    impl Backend for Native<'_> {
        type Process = Process;
        fn members(&mut self) -> io::Result<Vec<u32>> {
            // usize storage gives JOBOBJECT_BASIC_PROCESS_ID_LIST pointer alignment.
            let mut bytes = vec![
                0usize;
                (8 + MAX_PROCESSES * mem::size_of::<usize>())
                    .div_ceil(mem::size_of::<usize>())
            ];
            let size = bytes.len() * mem::size_of::<usize>();
            let mut returned = 0;
            // SAFETY: exact retained query-only job and aligned initialized output buffer; the borrowed job outlives this call.
            if unsafe {
                QueryInformationJobObject(
                    self.job.as_raw_handle(),
                    JobObjectBasicProcessIdList,
                    bytes.as_mut_ptr().cast(),
                    size as u32,
                    &mut returned,
                )
            } == 0
            {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: initialized usize buffer remains live, and its exact byte capacity bounds the read-only slice.
            let raw = unsafe { std::slice::from_raw_parts(bytes.as_ptr().cast::<u8>(), size) };
            members_from_buffer(raw, returned as usize, mem::size_of::<usize>())
        }
        fn open(&mut self, pid: u32) -> io::Result<Option<Process>> {
            // SAFETY: query-limited non-inheritable handle; no creation, adoption or process mutation.
            let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
            Ok((!handle.is_null()).then_some(Process(handle)))
        }
        fn observe(&mut self, process: &Process, pid: u32) -> io::Result<Option<Row>> {
            let mut contained = 0;
            let (mut created, mut exit, mut kernel, mut user) = (
                FILETIME::default(),
                FILETIME::default(),
                FILETIME::default(),
                FILETIME::default(),
            );
            let mut counters = PROCESS_MEMORY_COUNTERS_EX::default();
            counters.cb = mem::size_of_val(&counters) as u32;
            // SAFETY: both retained handles remain live; every initialized POD output has its exact Win32 type and byte count.
            if unsafe { IsProcessInJob(process.0, self.job.as_raw_handle(), &mut contained) } == 0
                || contained == 0
                || unsafe {
                    GetProcessTimes(process.0, &mut created, &mut exit, &mut kernel, &mut user)
                } == 0
                || unsafe {
                    K32GetProcessMemoryInfo(
                        process.0,
                        ptr::addr_of_mut!(counters).cast(),
                        counters.cb,
                    )
                } == 0
            {
                return Ok(None);
            }
            let birth =
                (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime);
            if birth == 0 {
                return Ok(None);
            }
            Ok(Some(Row {
                pid,
                birth,
                counters: BTreeMap::from([
                    ("rss_bytes".into(), counters.WorkingSetSize as u64),
                    ("peak_rss_bytes".into(), counters.PeakWorkingSetSize as u64),
                    ("private_bytes".into(), counters.PrivateUsage as u64),
                    (
                        "peak_private_bytes".into(),
                        counters.PeakPagefileUsage as u64,
                    ),
                ]),
            }))
        }
        fn close(&mut self, mut process: Process) -> io::Result<()> {
            // SAFETY: this is the sole owned query handle. Success disarms Drop; failure retains the final retry.
            if unsafe { CloseHandle(process.0) } == 0 {
                return Err(io::Error::other(
                    "Owned memory diagnostic process handle did not close",
                ));
            }
            process.0 = ptr::null_mut();
            Ok(())
        }
        fn peak(&mut self) -> io::Result<u64> {
            let mut information = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            let mut returned = 0;
            // SAFETY: live borrowed job and exact initialized extended-accounting output. No lifetime peak is substituted with sampled RSS.
            if unsafe {
                QueryInformationJobObject(
                    self.job.as_raw_handle(),
                    JobObjectExtendedLimitInformation,
                    ptr::addr_of_mut!(information).cast(),
                    mem::size_of_val(&information) as u32,
                    &mut returned,
                )
            } == 0
            {
                return Err(io::Error::last_os_error());
            }
            if returned as usize != mem::size_of_val(&information) {
                return Err(io::Error::other(
                    "Owned memory diagnostic peak query is incomplete",
                ));
            }
            Ok(information.PeakJobMemoryUsed as u64)
        }
        fn ticks(&mut self) -> io::Result<u64> {
            u64::try_from(self.start.elapsed().as_nanos()).map_err(io::Error::other)
        }
    }
    pub fn snapshot(tree: &super::super::tree::OwnedTree) -> io::Result<Snapshot> {
        observe(&mut Native::new(tree))
    }
    pub fn joint_snapshot(tree: &super::super::tree::OwnedTree) -> io::Result<JointSnapshot> {
        observe_joint(&mut Native::new(tree))
    }
}
#[cfg(windows)]
pub use windows::{AccountingQuery, joint_snapshot, snapshot};
