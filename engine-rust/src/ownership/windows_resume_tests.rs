use super::{Job, resume};
use crate::common::TempDirectory;
use std::{
    fs,
    io::{self, Read},
    os::windows::{
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
        process::{ChildExt, CommandExt},
    },
    path::Path,
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::System::Threading::{
    CREATE_NO_WINDOW, CREATE_SUSPENDED, GetCurrentThreadId, GetProcessIdOfThread, OpenThread,
    ResumeThread, SuspendThread, THREAD_QUERY_LIMITED_INFORMATION, THREAD_SUSPEND_RESUME,
};

#[path = "windows_resume_legacy.rs"]
mod legacy;

const CHILD_MODE: &str = "NUMERA_TEST_RETAINED_PRIMARY_FIXTURE";
const CHILD_TEST: &str =
    "ownership::windows::tests::retained_primary_resume_preserves_other_thread_suspension";
const BUDGET: Duration = Duration::from_secs(5);

struct Fixture {
    child: Child,
    job: Job,
    directory: TempDirectory,
    secondary: Option<OwnedHandle>,
    assigned: bool,
    settled: bool,
}

impl Fixture {
    fn suspended() -> io::Result<Self> {
        let directory = TempDirectory::new("windows-retained-primary")?;
        let job = Job::new()?;
        let child = Command::new(std::env::current_exe()?)
            .args(["--exact", CHILD_TEST, "--test-threads=1", "--nocapture"])
            .env(CHILD_MODE, directory.path())
            .creation_flags(CREATE_SUSPENDED | CREATE_NO_WINDOW)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()?;
        let mut fixture = Self {
            child,
            job,
            directory,
            secondary: None,
            assigned: false,
            settled: false,
        };
        fixture.job.assign(&fixture.child)?;
        fixture.assigned = true;
        Ok(fixture)
    }

    fn primary(&self) -> windows_sys::Win32::Foundation::HANDLE {
        self.child.main_thread_handle().as_raw_handle()
    }

    fn start_secondary(&mut self) -> io::Result<()> {
        // Bootstrap the fixture independently of either mechanism under test.
        // SAFETY: Child retains this original CREATE_SUSPENDED thread handle.
        require_count(unsafe { ResumeThread(self.primary()) }, 1)?;
        let ready = self.directory.path().join("ready");
        let end = Instant::now() + BUDGET;
        let thread_id = loop {
            match fs::read_to_string(&ready) {
                Ok(text) => break text.parse::<u32>().map_err(io::Error::other)?,
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
            if self.child.try_wait()?.is_some() {
                return Err(io::Error::other("Secondary-thread fixture exited early"));
            }
            if Instant::now() >= end {
                return Err(io::Error::new(io::ErrorKind::TimedOut, "Fixture readiness"));
            }
            thread::sleep(Duration::from_millis(10));
        };
        // SAFETY: the child keeps this secondary thread alive until stdin EOF;
        // verify its process before exercising any suspension capability.
        let raw = unsafe {
            OpenThread(
                THREAD_SUSPEND_RESUME | THREAD_QUERY_LIMITED_INFORMATION,
                0,
                thread_id,
            )
        };
        if raw.is_null() {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: successful OpenThread transfers one uniquely owned handle.
        let handle = unsafe { OwnedHandle::from_raw_handle(raw) };
        if unsafe { GetProcessIdOfThread(handle.as_raw_handle()) } != self.child.id() {
            return Err(io::Error::other(
                "Secondary thread belongs to another child",
            ));
        }
        self.secondary = Some(handle);
        Ok(())
    }

    fn settle(&mut self) -> io::Result<()> {
        if self.settled {
            return Ok(());
        }
        // Termination also settles suspended threads; no guessed resume count
        // or thread-ID reopen is needed on any assertion or error path.
        if self.assigned {
            self.job.terminate()?;
        } else {
            self.child.kill()?;
        }
        drop(self.child.stdin.take());
        let end = Instant::now() + BUDGET;
        loop {
            let exited = self.child.try_wait()?.is_some();
            if exited && self.job.complete_empty()? {
                self.settled = true;
                return Ok(());
            }
            if Instant::now() >= end {
                return Err(io::Error::new(io::ErrorKind::TimedOut, "Fixture cleanup"));
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn finish<T>(&mut self, result: io::Result<T>) -> io::Result<T> {
        let cleanup = self.settle();
        result.and_then(|value| cleanup.map(|()| value))
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if let Err(error) = self.settle() {
            super::diagnostic("resume_test_cleanup", &error);
            // The original kill-on-close Job remains owned until field drop.
        }
    }
}

fn require_count(actual: u32, expected: u32) -> io::Result<()> {
    if actual == u32::MAX {
        Err(io::Error::last_os_error())
    } else if actual == expected {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "Unexpected fixture suspend count: expected {expected}, observed {actual}"
        )))
    }
}

fn fixture_child(root: &Path) -> io::Result<()> {
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    let (stop_tx, stop_rx) = mpsc::sync_channel::<()>(0);
    let secondary = thread::spawn(move || {
        // SAFETY: querying this live thread creates no new resource or rights.
        let _ = ready_tx.send(unsafe { GetCurrentThreadId() });
        let _ = stop_rx.recv();
    });
    let thread_id = ready_rx.recv().map_err(io::Error::other)?;
    fs::write(root.join("ready.tmp"), thread_id.to_string())?;
    fs::rename(root.join("ready.tmp"), root.join("ready"))?;
    let mut byte = [0];
    let result = io::stdin().read(&mut byte);
    drop(stop_tx);
    secondary
        .join()
        .map_err(|_| io::Error::other("Secondary-thread fixture panicked"))?;
    result.map(|_| ())
}

fn secondary_count_after(release: impl FnOnce(&Child) -> io::Result<()>) -> io::Result<u32> {
    let mut fixture = Fixture::suspended()?;
    let result = (|| {
        fixture.start_secondary()?;
        let secondary = fixture
            .secondary
            .as_ref()
            .ok_or_else(|| io::Error::other("Secondary thread is absent"))?
            .as_raw_handle();
        // SAFETY: both original handles stay owned through Job settlement.
        require_count(unsafe { SuspendThread(secondary) }, 0)?;
        require_count(unsafe { SuspendThread(fixture.primary()) }, 0)?;
        release(&fixture.child)?;
        let count = unsafe { SuspendThread(secondary) };
        if count == u32::MAX {
            return Err(io::Error::last_os_error());
        }
        Ok(count)
    })();
    fixture.finish(result)
}

#[test]
fn retained_primary_resume_preserves_other_thread_suspension() -> io::Result<()> {
    if let Some(root) = std::env::var_os(CHILD_MODE) {
        return fixture_child(Path::new(&root));
    }
    // Real Win32 negative control: the literal previous mechanism consumes
    // another thread's suspension, independently of a scheduling race.
    let old = secondary_count_after(|child| legacy::resume(child.id()))?;
    assert_eq!(
        old, 0,
        "The original all-thread mechanism must expose the bug"
    );
    let current = secondary_count_after(resume)?;
    assert_eq!(
        current, 1,
        "Only the retained primary thread may be released"
    );
    println!(
        "{} [INFO] [ownership-test] legacy_secondary_suspend_count={old} retained_primary_secondary_suspend_count={current}",
        chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ")
    );
    Ok(())
}

#[test]
fn retained_primary_resume_refuses_unexpected_suspend_counts() -> io::Result<()> {
    let mut fixture = Fixture::suspended()?;
    let result = (|| {
        // SAFETY: this is the original, still-suspended thread, retained by Child.
        require_count(unsafe { SuspendThread(fixture.primary()) }, 1)?;
        let still_suspended = resume(&fixture.child)
            .expect_err("A remaining suspension cannot be reported as a successful start");
        assert_eq!(still_suspended.kind(), io::ErrorKind::Other);
        require_count(unsafe { ResumeThread(fixture.primary()) }, 1)?;
        let already_running = resume(&fixture.child)
            .expect_err("An already-running primary is outside the suspended-start contract");
        assert_eq!(already_running.kind(), io::ErrorKind::Other);
        Ok(())
    })();
    fixture.finish(result)
}
