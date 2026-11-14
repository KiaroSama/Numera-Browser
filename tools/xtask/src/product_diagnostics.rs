//! Bounded diagnostics for the actual native desktop compiler and bundler.
use crate::ci;
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

fn checked_metadata(path: &Path) -> io::Result<fs::Metadata> {
    let metadata = fs::symlink_metadata(path)?;
    #[cfg(windows)]
    let linked = {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    };
    #[cfg(not(windows))]
    let linked = metadata.file_type().is_symlink();
    if linked {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Native Tauri diagnostic source must not be a link or reparse point",
        ));
    }
    Ok(metadata)
}

fn read_log(path: &Path) -> io::Result<String> {
    const LIMIT: u64 = 16 * 1024 * 1024;
    let metadata = checked_metadata(path)?;
    if !metadata.is_file() || metadata.len() > LIMIT {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Native Tauri diagnostic source must be a regular bounded log",
        ));
    }
    let file = File::open(path)?;
    let opened = file.metadata()?;
    if !opened.is_file() || opened.len() > LIMIT {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Native Tauri diagnostic changed before retention",
        ));
    }
    let mut bytes = Vec::new();
    file.take(LIMIT + 1).read_to_end(&mut bytes)?;
    let after = checked_metadata(path)?;
    if bytes.len() as u64 != metadata.len() || !after.is_file() || after.len() != metadata.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Native Tauri diagnostic changed or exceeded its retention bound",
        ));
    }
    let redacted =
        numera_core::ownership::resources::redact_diagnostics(&String::from_utf8_lossy(&bytes));
    if redacted.len() as u64 > LIMIT {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Redacted native Tauri diagnostic exceeded its retention bound",
        ));
    }
    Ok(redacted)
}

/// Preserve only the complete bounded stdout/stderr pair before the exclusively
/// created failed product tree is removed. Never publish its vendor/installer tree.
pub fn preserve_failure(root: &Path, output: &Path) -> io::Result<Vec<PathBuf>> {
    let work = output.join(".desktop-build");
    let metadata = match checked_metadata(&work) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        result => result?,
    };
    if !metadata.is_dir() || !checked_metadata(output)?.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Native Tauri diagnostic directory is invalid",
        ));
    }
    let paths = [
        work.join("tauri-build.log"),
        work.join("tauri-build.stderr.log"),
    ];
    if paths.iter().all(|path| matches!(fs::symlink_metadata(path), Err(error) if error.kind() == io::ErrorKind::NotFound)) {
        return Ok(Vec::new());
    }
    let logs = [read_log(&paths[0])?, read_log(&paths[1])?];
    let directory = root.join("build");
    fs::create_dir_all(&directory)?;
    if !checked_metadata(&directory)?.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Native Tauri retention directory is invalid",
        ));
    }
    static NEXT: AtomicU64 = AtomicU64::new(0);
    for _ in 0..128 {
        let stem = format!(
            "native-phase-tauri-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let stdout = directory.join(format!("{stem}.log"));
        let stderr = directory.join(format!("{stem}.stderr.log"));
        let mut first = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&stdout)
        {
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            result => result?,
        };
        let mut second = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&stderr)
        {
            Ok(file) => file,
            Err(error) => {
                drop(first);
                let _ = fs::remove_file(&stdout);
                if error.kind() == io::ErrorKind::AlreadyExists {
                    continue;
                }
                return Err(error);
            }
        };
        let saved = (|| {
            first.write_all(logs[0].as_bytes())?;
            second.write_all(logs[1].as_bytes())?;
            first.sync_all()?;
            second.sync_all()
        })();
        drop(first);
        drop(second);
        if let Err(error) = saved {
            let _ = fs::remove_file(&stdout);
            let _ = fs::remove_file(&stderr);
            return Err(error);
        }
        return Ok(vec![stdout, stderr]);
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "Native Tauri diagnostic names exhausted",
    ))
}

fn tail(text: &str) -> String {
    let mut start = text.len().saturating_sub(16 * 1024);
    while !text.is_char_boundary(start) {
        start += 1;
    }
    let text = text[start..]
        .chars()
        .filter(|character| !character.is_control() || ['\n', '\r', '\t'].contains(character))
        .collect::<String>();
    if start == 0 {
        text
    } else {
        format!("[earlier output omitted]\n{text}")
    }
}

pub fn execute(root: &Path, command: &mut Command, log: &Path) -> ci::Result<()> {
    numera_core::browser::adapter::restrict_environment(command);
    command.env_remove("GH_TOKEN").env_remove("GITHUB_TOKEN");
    let mut partial = [Vec::new(), Vec::new()];
    let result = ci::owned::run_streamed(
        root,
        command,
        ci::owned::limits(
            Duration::from_secs(1800),
            Duration::from_secs(300),
            16 * 1024 * 1024,
        ),
        |stream, bytes| {
            if let Some(output) = partial.get_mut(stream) {
                output.extend_from_slice(bytes);
            }
        },
    );
    // Retain the bounded transcript and redact complete values before clipping
    // the console tails, so clipping cannot expose a secret suffix.
    let (stdout, stderr) = ci::evidence::retain_test_diagnostics(log, &partial[0], &partial[1])?;
    let stdout = tail(&stdout);
    let stderr = tail(&stderr);
    let failed = match result {
        Ok(output) if output.command.exit == 0 => None,
        Ok(output) => Some(format!("exit={}", output.command.exit)),
        Err(error) => Some(format!("capture={error}")),
    };
    if let Some(failed) = failed {
        return Err(io::Error::other(format!(
            "Native Tauri compiler/bundler failed ({failed})\nCaptured stdout:\n{stdout}\nCaptured stderr:\n{stderr}\nRetained diagnostic: {}",
            log.display()
        )).into());
    }
    eprintln!(
        "Native Tauri compiler/bundler completed\nCaptured stdout:\n{stdout}\nCaptured stderr:\n{stderr}\nRetained diagnostic: {}",
        log.display()
    );
    Ok(())
}
