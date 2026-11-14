//! Bounded diagnostics for the actual native desktop compiler and bundler.
use crate::ci;
use std::{io, path::Path, process::Command, time::Duration};

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
