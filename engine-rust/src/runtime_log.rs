use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};

/// Only fixed event labels are accepted here. User records,
/// arguments, URLs, passwords and transport bodies never become log entries.
pub struct RuntimeLog {
    file: Mutex<Option<File>>,
    path: Option<PathBuf>,
}
impl RuntimeLog {
    pub fn new(directory: &Path, component: &'static str) -> Self {
        let opened = (|| -> io::Result<(File, PathBuf)> {
            fs::create_dir_all(directory)?;
            let stamp = chrono::Utc::now()
                .format("%Y-%m-%d_%H-%M-%S_UTC")
                .to_string();
            for suffix in 0..1000 {
                let name = if suffix == 0 {
                    format!("{component}_{stamp}.log")
                } else {
                    format!("{component}_{stamp}.{suffix}.log")
                };
                let path = directory.join(name);
                match OpenOptions::new().write(true).create_new(true).open(&path) {
                    Ok(file) => return Ok((file, path)),
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                    Err(error) => return Err(error),
                }
            }
            Err(io::Error::other("Runtime log filename namespace exhausted"))
        })();
        match opened {
            Ok((file, path)) => Self {
                file: Mutex::new(Some(file)),
                path: Some(path),
            },
            Err(_) => {
                eprintln!(
                    "Runtime file logging unavailable; safe lifecycle diagnostics remain on stderr"
                );
                Self {
                    file: Mutex::new(None),
                    path: None,
                }
            }
        }
    }
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }
    pub fn event(&self, level: &'static str, component: &'static str, event: &'static str) {
        let stamp = chrono::Utc::now()
            .format("%Y-%m-%d %H:%M:%S UTC")
            .to_string();
        let line = format!("[{stamp}] [{level}] [{component}] {event}\n");
        let mut file = self
            .file
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(file) = file.as_mut()
            && file
                .write_all(line.as_bytes())
                .and_then(|_| file.flush())
                .is_ok()
        {
            return;
        }
        *file = None;
        eprint!("{line}");
    }
}
#[cfg(test)]
#[path = "../tests/common/mod.rs"]
mod common;
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn runtime_logs_are_exclusive_utf8_and_never_accept_dynamic_input() -> io::Result<()> {
        let root = common::TempDirectory::new("runtime-log")?;
        let directory = root.path().join("logs with spaces");
        let first = RuntimeLog::new(&directory, "native-http");
        let second = RuntimeLog::new(&directory, "native-http");
        assert_ne!(first.path(), second.path());
        first.event("INFO", "server", "Startup complete");
        first.event("WARNING", "server", "Cleanup pending");
        first.event("ERROR", "server", "Controlled failure");
        let path = first
            .path()
            .ok_or_else(|| io::Error::other("Log file not created"))?
            .to_owned();
        assert!(
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().contains("_UTC"))
        );
        drop(first);
        drop(second);
        let text = fs::read_to_string(path)?;
        assert!(text.contains(" UTC] [INFO] [server] Startup complete\n"));
        assert!(text.contains("[WARNING] [server] Cleanup pending"));
        assert!(text.contains("[ERROR] [server] Controlled failure"));
        root.cleanup()?;
        Ok(())
    }
}
