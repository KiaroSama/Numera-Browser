//! Linux per-PID stat disappearance, separate from complete-table enumeration.
use std::io;

pub(super) fn stat_text(result: io::Result<String>) -> io::Result<Option<String>> {
    match result {
        Ok(text) => Ok(Some(text)),
        // An opened /proc/<pid>/stat descriptor reports ESRCH when its task
        // exits before the read. Only these per-PID absence errors are safe;
        // directory enumeration and every other read failure stay uncertain.
        Err(error)
            if error.kind() == io::ErrorKind::NotFound
                || error.raw_os_error() == Some(libc::ESRCH) =>
        {
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

#[cfg(all(test, feature = "test-support"))]
#[path = "procfs_disappearance_tests.rs"]
mod tests;
