use super::stat_text;
use crate::{
    common::TempDirectory,
    ownership::{
        observation,
        test_support::{Backend, with_backend},
        tree::{OwnedTree, record_empty},
    },
};
use serde_json::json;
use std::{
    fs::{self, File},
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicI32, AtomicUsize, Ordering},
    },
    time::Duration,
};

enum StatRead {
    Descriptor(Mutex<File>),
    Errno(i32),
    Text(&'static str),
}

struct Procfs {
    root: PathBuf,
    path: PathBuf,
    read: StatRead,
    reads: AtomicUsize,
    errno: AtomicI32,
}

impl Backend for Procfs {
    fn procfs_root(&self) -> Option<PathBuf> {
        Some(self.root.clone())
    }

    fn stat(&self, path: &Path) -> Option<io::Result<String>> {
        if path != self.path {
            return None;
        }
        self.reads.fetch_add(1, Ordering::SeqCst);
        let result = match &self.read {
            StatRead::Descriptor(file) => {
                let mut text = String::new();
                file.lock()
                    .map_err(|_| io::Error::other("Fixture stat descriptor lock poisoned"))
                    .and_then(|mut file| file.read_to_string(&mut text))
                    .map(|_| text)
            }
            StatRead::Errno(errno) => Err(io::Error::from_raw_os_error(*errno)),
            StatRead::Text(text) => Ok((*text).into()),
        };
        self.errno.store(
            result
                .as_ref()
                .err()
                .and_then(io::Error::raw_os_error)
                .unwrap_or(0),
            Ordering::SeqCst,
        );
        Some(result)
    }
}

fn scan(root: &Path, pid: u32, read: StatRead) -> (io::Result<bool>, Arc<Procfs>) {
    let source = Arc::new(Procfs {
        root: root.into(),
        path: root.join(pid.to_string()).join("stat"),
        read,
        reads: AtomicUsize::new(0),
        errno: AtomicI32::new(0),
    });
    let result = with_backend(source.clone(), || {
        record_empty(&json!({"tree":{"kind":"pgid","group":pid}}))
    });
    (result, source)
}

#[test]
fn exited_stat_descriptor_is_absent_in_a_complete_group_scan() -> io::Result<()> {
    let root = TempDirectory::new("procfs-exited-descriptor")?;
    let mut command = Command::new("/bin/sleep");
    command
        .arg("60")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = OwnedTree::spawn(&mut command)?;
    let pid = child.pid();
    // Keep the real kernel descriptor open across exit and complete reaping.
    // Reading this descriptor exercises proc_single_show's ESRCH path; opening
    // the path only after exit would merely reproduce ENOENT.
    let file = File::open(format!("/proc/{pid}/stat"))?;
    child.settle(Duration::from_secs(5))?;
    assert!(child.child_mut().try_wait()?.is_some());
    fs::create_dir(root.path().join(pid.to_string()))?;
    let (result, source) = scan(root.path(), pid, StatRead::Descriptor(Mutex::new(file)));
    assert_eq!(source.reads.load(Ordering::SeqCst), 1);
    assert_eq!(source.errno.load(Ordering::SeqCst), libc::ESRCH);
    assert!(
        result?,
        "An exited PID is absent from the complete process table"
    );
    eprintln!(
        "{} [INFO] [ownership-test] retained_procfs_descriptor_errno={} complete_group_empty=true",
        chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ"),
        source.errno.load(Ordering::SeqCst)
    );
    Ok(())
}

#[test]
fn only_not_found_and_exact_esrch_are_pid_disappearance() -> io::Result<()> {
    const PID: u32 = 4242;
    let root = TempDirectory::new("procfs-disappearance-errors")?;
    fs::create_dir(root.path().join(PID.to_string()))?;
    // ENXIO and ESRCH share an ErrorKind on this pinned Rust/Linux toolchain.
    // A broad ErrorKind exemption would incorrectly accept this refusal case.
    assert_eq!(
        io::Error::from_raw_os_error(libc::ENXIO).kind(),
        io::Error::from_raw_os_error(libc::ESRCH).kind()
    );
    for (errno, absent) in [
        (libc::ENOENT, true),
        (libc::ESRCH, true),
        (libc::EACCES, false),
        (libc::EPERM, false),
        (libc::EIO, false),
        (libc::ENXIO, false),
        (libc::EBADF, false),
        (libc::EINVAL, false),
    ] {
        let classified = stat_text(Err(io::Error::from_raw_os_error(errno)));
        let (observed, source) = scan(root.path(), PID, StatRead::Errno(errno));
        assert_eq!(source.reads.load(Ordering::SeqCst), 1);
        if absent {
            assert!(classified?.is_none(), "errno={errno}");
            assert!(observed?, "errno={errno}");
        } else {
            let error = classified.expect_err("A non-disappearance error must be retained");
            assert_eq!(error.raw_os_error(), Some(errno));
            assert_eq!(
                observed
                    .expect_err("Unknown PID state cannot prove emptiness")
                    .kind(),
                error.kind()
            );
            assert!(
                observation::diagnostic(PID as i32, false, false, false)
                    .contains(&format!("query_errno={errno}"))
            );
        }
    }
    assert!(
        stat_text(Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Absent fixture"
        )))?
        .is_none()
    );
    assert!(stat_text(Err(io::Error::other("Unknown fixture read"))).is_err());
    assert_eq!(
        stat_text(Ok("complete fixture bytes".into()))?,
        Some("complete fixture bytes".into())
    );
    Ok(())
}

#[test]
fn malformed_stat_and_incomplete_enumeration_remain_unknown() -> io::Result<()> {
    const PID: u32 = 4242;
    let root = TempDirectory::new("procfs-disappearance-boundaries")?;
    fs::create_dir(root.path().join(PID.to_string()))?;
    let (malformed, source) = scan(root.path(), PID, StatRead::Text("incomplete stat"));
    assert_eq!(source.reads.load(Ordering::SeqCst), 1);
    assert!(
        malformed.is_err(),
        "Malformed stat must not become disappearance"
    );
    let (live, _) = scan(root.path(), PID, StatRead::Text("4242 (fixture) S 1 4242"));
    assert!(!live?, "A live group member must still prevent emptiness");
    // ENOENT from directory enumeration is incomplete evidence, unlike ENOENT
    // from an individually enumerated PID's stat file.
    let (missing, source) = scan(
        &root.path().join("missing"),
        PID,
        StatRead::Errno(libc::ESRCH),
    );
    assert_eq!(
        missing
            .expect_err("Missing table cannot prove emptiness")
            .kind(),
        io::ErrorKind::NotFound
    );
    assert_eq!(source.reads.load(Ordering::SeqCst), 0);
    let file = root.path().join("not-a-table");
    fs::write(&file, b"fixture")?;
    let (incomplete, source) = scan(&file, PID, StatRead::Errno(libc::ESRCH));
    assert!(incomplete.is_err());
    assert_eq!(source.reads.load(Ordering::SeqCst), 0);
    Ok(())
}
