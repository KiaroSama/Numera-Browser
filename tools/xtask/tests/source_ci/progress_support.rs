use super::*;
use std::{
    io::Write,
    process::Command,
    time::{Duration, Instant},
};

const MODE: &str = "NUMERA_BUILD_PROGRESS_FIXTURE";

fn command(name: &str, mode: &str) -> ci::Result<Command> {
    let mut command = Command::new(std::env::current_exe()?);
    command
        .args(["--exact", name, "--nocapture", "--test-threads=1"])
        .env(MODE, mode);
    Ok(command)
}

pub fn child() -> ci::Result<bool> {
    let Ok(mode) = std::env::var(MODE) else {
        return Ok(false);
    };
    let name = std::thread::current()
        .name()
        .ok_or("fixture test name")?
        .to_owned();
    match mode.as_str() {
        "writer" => {
            for _ in 0..12 {
                std::io::stdout().write_all(b"private retained child payload\n")?;
                std::io::stdout().flush()?;
                std::thread::sleep(Duration::from_millis(250));
            }
        }
        "silent" => std::thread::sleep(Duration::from_secs(5)),
        "vendor-silent" => std::thread::sleep(Duration::from_secs(45)),
        "vendor-buffered" | "vendor-buffered-silent" => {
            let mut command = command(
                &name,
                if mode == "vendor-buffered" {
                    "writer"
                } else {
                    "vendor-silent"
                },
            )?;
            let output = numera_xtask::provision::execute(&mut command, 50)?;
            assert_eq!(output.exit, 0);
            if mode == "vendor-buffered" {
                assert!(
                    String::from_utf8_lossy(&output.stdout)
                        .contains("private retained child payload")
                );
            }
        }
        "buffered" | "buffered-silent" => {
            let mut command = command(
                &name,
                if mode == "buffered" {
                    "writer"
                } else {
                    "silent"
                },
            )?;
            let output = ci::capture(
                &mut command,
                Duration::from_secs(8),
                Duration::from_secs(6),
                65536,
            )?;
            assert_eq!(output.exit, 0);
            if mode == "buffered" {
                assert!(
                    String::from_utf8_lossy(&output.stdout)
                        .contains("private retained child payload")
                );
            }
        }
        _ => return ci::fail("Unknown build progress fixture mode"),
    }
    Ok(true)
}

pub fn controls() -> TestResult {
    let root = Temp::new("actual-build-progress")?;
    let name = std::thread::current()
        .name()
        .ok_or("fixture test name")?
        .to_owned();
    let bounds = || ci::owned::limits(Duration::from_secs(9), Duration::from_secs(2), 65536);
    let output = ci::owned::run(&root.0, &mut command(&name, "buffered")?, bounds())?;
    assert_eq!(output.command.exit, 0);
    assert!(
        !String::from_utf8_lossy(&output.command.stdout).contains("private retained child payload")
    );
    assert!(String::from_utf8_lossy(&output.command.stderr).contains("operation=child-stdout"));

    let start = Instant::now();
    let mut observed = Vec::new();
    let result = ci::owned::run_streamed(
        &root.0,
        &mut command(&name, "buffered-silent")?,
        bounds(),
        |_, bytes| observed.extend_from_slice(bytes),
    );
    assert!(result.is_err(), "Silent work manufactured activity");
    assert!(start.elapsed() < Duration::from_secs(9));
    assert!(!String::from_utf8_lossy(&observed).contains("Native build activity"));
    let output = ci::owned::run(&root.0, &mut command(&name, "vendor-buffered")?, bounds())?;
    assert_eq!(output.command.exit, 0);
    assert!(String::from_utf8_lossy(&output.command.stderr).contains("operation=vendor-stdout"));
    assert!(
        !String::from_utf8_lossy(&output.command.stdout).contains("private retained child payload")
    );
    // Exceed the old 30-second timer. A timer-only heartbeat would retain the
    // silent vendor command until the outer wall bound instead of idle refusal.
    let started = Instant::now();
    let mut observed = Vec::new();
    let refused = ci::owned::run_streamed(
        &root.0,
        &mut command(&name, "vendor-buffered-silent")?,
        ci::owned::limits(Duration::from_secs(40), Duration::from_secs(31), 65536),
        |_, bytes| observed.extend_from_slice(bytes),
    );
    assert!(refused.is_err());
    assert!(
        started.elapsed() < Duration::from_secs(38),
        "Silent vendor command escaped the original idle bound"
    );
    let observed = String::from_utf8_lossy(&observed);
    assert!(!observed.contains("Vendor provisioning is still active"));
    assert!(!observed.contains("Native build activity"));

    // The shipped copier must retain complete bytes, retry interrupted reads,
    // and expose write failure instead of counting an unaccepted write.
    struct Interrupted<'a> {
        bytes: &'a [u8],
        first: bool,
    }
    impl std::io::Read for Interrupted<'_> {
        fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
            if self.first {
                self.first = false;
                return Err(std::io::ErrorKind::Interrupted.into());
            }
            std::io::Read::read(&mut self.bytes, output)
        }
    }
    struct Refused;
    impl Write for Refused {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::ErrorKind::PermissionDenied.into())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let bytes = vec![127; 131073];
    let mut source = Interrupted {
        bytes: &bytes,
        first: true,
    };
    let mut destination = Vec::new();
    assert_eq!(
        numera_xtask::progress::copy(&mut source, &mut destination)?,
        bytes.len() as u64
    );
    assert_eq!(destination, bytes);
    assert_eq!(
        numera_xtask::progress::copy(&mut bytes.as_slice(), &mut Refused)
            .expect_err("Refused destination must fail the real copy")
            .kind(),
        std::io::ErrorKind::PermissionDenied
    );
    Ok(())
}
