use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};

pub(crate) enum NativePhase {
    Startup,
    Prepare,
    Metadata,
    Execute,
    Shutdown,
    BrowserSpawn,
    BrowserHello,
    BrowserOpen,
    BrowserCookiesRead,
    BrowserClose,
    BrowserSettle,
}
impl NativePhase {
    fn label(&self) -> &'static str {
        match self {
            Self::Startup => "startup",
            Self::Prepare => "prepare",
            Self::Metadata => "metadata",
            Self::Execute => "execute",
            Self::Shutdown => "shutdown",
            Self::BrowserSpawn => "browser_spawn",
            Self::BrowserHello => "browser_hello",
            Self::BrowserOpen => "browser_open",
            Self::BrowserCookiesRead => "browser_cookies_read",
            Self::BrowserClose => "browser_close",
            Self::BrowserSettle => "browser_settle",
        }
    }
}

/// Only fixed event labels are accepted here. User records,
/// arguments, URLs, passwords and transport bodies never become log entries.
pub struct RuntimeLog {
    file: Mutex<Option<File>>,
    path: Option<PathBuf>,
    console_fallback: bool,
}
impl RuntimeLog {
    pub(crate) fn directory(root: &Path) -> PathBuf {
        std::env::var_os("NUMERA_LOG_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| root.join("logs"))
    }
    pub fn new(directory: &Path, component: &'static str) -> Self {
        Self::open(directory, component, true)
    }
    pub(crate) fn file_only(directory: &Path, component: &'static str) -> Self {
        Self::open(directory, component, false)
    }
    fn open(directory: &Path, component: &'static str, console_fallback: bool) -> Self {
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
                console_fallback,
            },
            Err(_) => {
                if console_fallback {
                    eprintln!(
                        "Runtime file logging unavailable; safe lifecycle diagnostics remain on stderr"
                    );
                }
                Self {
                    file: Mutex::new(None),
                    path: None,
                    console_fallback,
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
        self.write(&line, self.console_fallback);
    }
    pub(crate) fn timing(&self, phase: NativePhase, elapsed: Duration) {
        let stamp = chrono::Utc::now()
            .format("%Y-%m-%d %H:%M:%S UTC")
            .to_string();
        let elapsed = u64::try_from(elapsed.as_nanos()).unwrap_or(u64::MAX);
        let line = format!(
            "[{stamp}] [INFO] [entry] Native timing phase={} elapsed_ns={elapsed}\n",
            phase.label()
        );
        // Diagnostic timing must never change command stderr on logging failure.
        self.write(&line, false);
    }
    fn write(&self, line: &str, console_fallback: bool) {
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
        if console_fallback {
            eprint!("{line}");
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::common;
    #[test]
    fn file_only_diagnostics_do_not_enable_console_fallback()
    -> Result<(), Box<dyn std::error::Error>> {
        use crate::ownership::capture;
        use std::{process::Command, time::Duration};

        if let Some(root) = std::env::var_os("NUMERA_LOG_FAILURE_FIXTURE") {
            let root = PathBuf::from(root);
            let unavailable = root.join("not-a-directory");
            fs::write(&unavailable, b"retained")?;
            let quiet = std::env::var("NUMERA_LOG_FAILURE_MODE")? == "file-only";
            let log = if quiet {
                RuntimeLog::file_only(&unavailable, "native-vault-enable")
            } else {
                RuntimeLog::new(&unavailable, "native-cli")
            };
            assert!(log.path().is_none());
            log.timing(NativePhase::Prepare, Duration::from_nanos(123));
            log.timing(NativePhase::BrowserSpawn, Duration::from_nanos(123));
            log.timing(NativePhase::BrowserSettle, Duration::from_nanos(123));
            log.event("INFO", "vault", "Enable lock requested");
            assert_eq!(fs::read(&unavailable)?, b"retained");
            if quiet {
                let log = RuntimeLog::file_only(&root.join("logs"), "native-vault-enable");
                let path = log.path().ok_or("File log missing")?.to_owned();
                // A read-only handle deterministically exercises a failed write.
                *log.file
                    .lock()
                    .map_err(|_| io::Error::other("Log mutex poisoned"))? =
                    Some(File::open(&path)?);
                log.timing(NativePhase::Prepare, Duration::from_nanos(123));
                log.event("INFO", "vault", "Enable descriptor derivation started");
                assert!(fs::read(path)?.is_empty());
            }
            return Ok(());
        }
        let root = common::TempDirectory::new("file-only-log")?;
        for mode in ["file-only", "default"] {
            let mut command = Command::new(std::env::current_exe()?);
            command
                .args([
                    "--exact",
                    "runtime_log::tests::file_only_diagnostics_do_not_enable_console_fallback",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env("NUMERA_LOG_FAILURE_FIXTURE", root.path())
                .env("NUMERA_LOG_FAILURE_MODE", mode);
            let output = capture::run(
                &mut command,
                capture::Limits {
                    wall: Duration::from_secs(5),
                    idle: Duration::from_secs(5),
                    bytes: 65536,
                    cleanup: Duration::from_secs(5),
                },
            )?;
            assert_eq!(output.exit, 0, "Log failure fixture failed");
            let stderr = String::from_utf8(output.stderr)?;
            let stdout = String::from_utf8(output.stdout)?;
            assert!(stdout.lines().any(|line| line == "running 1 test"));
            assert!(stdout.contains("test runtime_log::tests::file_only_diagnostics_do_not_enable_console_fallback ... ok"));
            assert!(!stdout.contains("Native timing") && !stderr.contains("Native timing"));
            assert!(
                !stdout.contains("Enable lock requested")
                    && !stdout.contains("Enable descriptor derivation started"),
                "Diagnostics wrote to captured stdout"
            );
            if mode == "file-only" {
                assert!(
                    stderr.is_empty(),
                    "File-only diagnostics wrote to captured stderr"
                );
            } else {
                assert!(stderr.contains("Runtime file logging unavailable"));
                assert!(stderr.contains("Enable lock requested"));
            }
        }
        root.cleanup()?;
        Ok(())
    }
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
        for phase in [
            NativePhase::Startup,
            NativePhase::Prepare,
            NativePhase::Metadata,
            NativePhase::Execute,
            NativePhase::Shutdown,
            NativePhase::BrowserSpawn,
            NativePhase::BrowserHello,
            NativePhase::BrowserOpen,
            NativePhase::BrowserCookiesRead,
            NativePhase::BrowserClose,
            NativePhase::BrowserSettle,
        ] {
            first.timing(phase, std::time::Duration::from_nanos(123));
        }
        first.timing(NativePhase::Execute, std::time::Duration::ZERO);
        first.timing(
            NativePhase::Shutdown,
            std::time::Duration::from_secs(u64::MAX),
        );
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
        for phase in [
            "startup",
            "prepare",
            "metadata",
            "execute",
            "shutdown",
            "browser_spawn",
            "browser_hello",
            "browser_open",
            "browser_cookies_read",
            "browser_close",
            "browser_settle",
        ] {
            assert!(text.contains(&format!(
                "[INFO] [entry] Native timing phase={phase} elapsed_ns=123\n"
            )));
        }
        assert!(text.contains("Native timing phase=execute elapsed_ns=0\n"));
        assert!(text.contains(&format!(
            "Native timing phase=shutdown elapsed_ns={}\n",
            u64::MAX
        )));
        assert_eq!(
            text.lines()
                .filter(|line| line.contains("Native timing"))
                .count(),
            13
        );
        root.cleanup()?;
        Ok(())
    }
}
