use super::*;
use numera_core::ownership::tree::{WindowsCleanup, settle_windows_with};

struct Kernel {
    mode: &'static str,
    captures: usize,
    empty_queries: usize,
    signals: usize,
    waits: usize,
    leader_queries: usize,
    witnesses: Vec<u32>,
    deadline: Instant,
}
impl Kernel {
    fn new(mode: &'static str) -> Self {
        Self {
            mode,
            captures: 0,
            empty_queries: 0,
            signals: 0,
            waits: 0,
            leader_queries: 0,
            witnesses: Vec::new(),
            deadline: Instant::now(),
        }
    }
}
impl WindowsCleanup for Kernel {
    fn capture_members(&mut self, deadline: Instant) -> io::Result<()> {
        self.deadline = deadline;
        self.captures += 1;
        if self.witnesses.is_empty() {
            self.witnesses.push(1);
        }
        if self.captures >= 2 && !self.witnesses.contains(&2) {
            self.witnesses.push(2);
        }
        if self.mode == "capture-error" {
            return Err(io::Error::from_raw_os_error(5));
        }
        if self.mode == "late-capture" {
            while Instant::now() < deadline {
                thread::sleep(Duration::from_millis(1));
            }
        }
        Ok(())
    }
    fn complete_empty(&mut self) -> io::Result<bool> {
        self.empty_queries += 1;
        if self.mode == "query-error" && self.empty_queries > 1 {
            return Err(io::Error::from_raw_os_error(87));
        }
        Ok(self.empty_queries > 1)
    }
    fn terminate(&mut self) -> io::Result<()> {
        self.signals += 1;
        if self.mode == "signal-error" {
            Err(io::Error::from_raw_os_error(5))
        } else {
            Ok(())
        }
    }
    fn all_signaled(&mut self, _: Instant) -> io::Result<bool> {
        self.waits += 1;
        if self.mode == "wait-error" {
            return Err(io::Error::from_raw_os_error(6));
        }
        Ok(self.mode != "live" && (self.mode != "transient" || self.waits >= 2))
    }
    fn leader_exited(&mut self) -> io::Result<bool> {
        self.leader_queries += 1;
        if self.mode == "late-leader" {
            while Instant::now() < self.deadline {
                thread::sleep(Duration::from_millis(1));
            }
        }
        Ok(self.mode != "leader-live")
    }
}
pub(super) fn policy_controls() -> TestResult {
    for mode in ["complete", "transient"] {
        let mut kernel = Kernel::new(mode);
        settle_windows_with(Instant::now() + Duration::from_secs(1), &mut kernel)?;
        assert_eq!(kernel.signals, 1);
        assert!(kernel.captures >= 2);
        assert_eq!(
            kernel.witnesses,
            [1, 2],
            "A member visible after termination must be reconciled"
        );
        assert!(kernel.waits > 0 && kernel.leader_queries > 0);
    }
    for mode in ["live", "leader-live", "late-capture", "late-leader"] {
        let mut kernel = Kernel::new(mode);
        let start = Instant::now();
        let result = settle_windows_with(start + Duration::from_millis(25), &mut kernel);
        assert_eq!(
            result.err().map(|error| error.kind()),
            Some(io::ErrorKind::TimedOut),
            "{mode}"
        );
        assert!(start.elapsed() < Duration::from_secs(1));
        assert!(
            kernel.signals <= 1,
            "Cleanup must never repeatedly terminate while polling"
        );
        if mode == "late-capture" {
            assert_eq!(kernel.signals, 0);
        }
    }
    for (mode, code) in [
        ("capture-error", 5),
        ("signal-error", 5),
        ("query-error", 87),
        ("wait-error", 6),
    ] {
        let mut kernel = Kernel::new(mode);
        let result = settle_windows_with(Instant::now() + Duration::from_secs(1), &mut kernel);
        assert_eq!(
            result.err().and_then(|error| error.raw_os_error()),
            Some(code)
        );
        assert!(
            !kernel.witnesses.is_empty(),
            "Partial verified owners must survive refusal"
        );
        assert!(kernel.signals <= 1);
        if mode == "capture-error" {
            assert_eq!(kernel.signals, 0);
        }
    }
    let mut retry = Kernel::new("wait-error");
    assert!(settle_windows_with(Instant::now() + Duration::from_secs(1), &mut retry).is_err());
    let held = retry.witnesses.clone();
    retry.mode = "complete";
    settle_windows_with(Instant::now() + Duration::from_secs(1), &mut retry)?;
    assert_eq!(
        retry.witnesses, held,
        "A retry preserves the original observed object set"
    );
    assert_eq!(
        retry.signals, 1,
        "An already empty Job does not need another termination"
    );
    let mut expired = Kernel::new("complete");
    assert_eq!(
        settle_windows_with(Instant::now(), &mut expired)
            .err()
            .map(|error| error.kind()),
        Some(io::ErrorKind::TimedOut)
    );
    assert_eq!(
        (expired.captures, expired.signals, expired.waits),
        (0, 0, 0)
    );
    #[cfg(windows)]
    {
        let root = temp("windows-original-owner-drop")?;
        let mut command = Command::new(fixture()?);
        command.arg("forced-cleanup-drop").arg(root.path());
        let output = capture::run(&mut command, limit(Duration::from_secs(5)))?;
        assert!(
            output.exit == 0,
            "Raw Windows Drop/retry failed its original-owner proof"
        );
        let value: Value = serde_json::from_slice(&output.stdout)?;
        assert_eq!(value["retained"], true);
        assert_eq!(value["empty"], true);
        assert_eq!(
            value["after"]
                .as_u64()
                .and_then(|count| count.checked_add(3)),
            value["before"].as_u64()
        );
    }
    Ok(())
}

#[cfg(windows)]
pub(super) struct LeafWitness(std::os::windows::io::OwnedHandle);
#[cfg(windows)]
impl LeafWitness {
    pub(super) fn retain(pid: u32, token: Option<u64>, record: &Value) -> io::Result<Self> {
        use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
        use windows_sys::Win32::{
            Foundation::FILETIME,
            System::{
                JobObjects::{IsProcessInJob, OpenJobObjectW},
                SystemServices::JOB_OBJECT_QUERY,
                Threading::{
                    GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
                    PROCESS_SYNCHRONIZE,
                },
            },
        };
        // SAFETY: query/synchronize only; no process termination authority.
        let process = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                0,
                pid,
            )
        };
        if process.is_null() {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: successful open transfers a CloseHandle owner.
        let process = unsafe { OwnedHandle::from_raw_handle(process) };
        let mut creation = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let mut exit = creation;
        let mut kernel = creation;
        let mut user = creation;
        if unsafe {
            GetProcessTimes(
                process.as_raw_handle(),
                &mut creation,
                &mut exit,
                &mut kernel,
                &mut user,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let birth = (u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime);
        if token != Some(birth) {
            return Err(io::Error::other("Leaf birth differs before cleanup"));
        }
        let name = record["tree"]["group"]
            .as_str()
            .ok_or_else(|| io::Error::other("Job name missing"))?;
        let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
        let job = unsafe { OpenJobObjectW(JOB_OBJECT_QUERY, 0, wide.as_ptr()) };
        if job.is_null() {
            return Err(io::Error::last_os_error());
        }
        let job = unsafe { OwnedHandle::from_raw_handle(job) };
        let mut member = 0;
        if unsafe { IsProcessInJob(process.as_raw_handle(), job.as_raw_handle(), &mut member) } == 0
        {
            return Err(io::Error::last_os_error());
        }
        if member == 0 {
            return Err(io::Error::other("Leaf was not in its retained session Job"));
        }
        let result = Self(process);
        result.assert_live()?;
        Ok(result)
    }
    fn state(&self) -> u32 {
        use std::os::windows::io::AsRawHandle;
        // SAFETY: this exact original process handle stays owned across finish.
        unsafe {
            windows_sys::Win32::System::Threading::WaitForSingleObject(self.0.as_raw_handle(), 0)
        }
    }
    fn assert_live(&self) -> io::Result<()> {
        if self.state() == windows_sys::Win32::Foundation::WAIT_TIMEOUT {
            Ok(())
        } else {
            Err(io::Error::other("Leaf was not live before session cleanup"))
        }
    }
    pub(super) fn assert_exited(&self) {
        assert_eq!(
            self.state(),
            windows_sys::Win32::Foundation::WAIT_OBJECT_0,
            "The ORIGINAL descendant process object must be signaled before claim release"
        );
    }
}
