//! Bounded subprocess output and capture progress.
use super::Result;
use numera_core::ownership::capture;
use std::{process::Command, time::Duration};

pub fn capture(
    command: &mut Command,
    wall: Duration,
    idle: Duration,
    bytes: usize,
) -> Result<capture::Output> {
    capture_streamed(command, wall, idle, bytes, |_, _| {})
}
pub fn capture_streamed(
    command: &mut Command,
    wall: Duration,
    idle: Duration,
    bytes: usize,
    mut on_chunk: impl FnMut(usize, &[u8]),
) -> Result<capture::Output> {
    let result = capture::run_observed_streamed(
        command,
        capture::Limits {
            wall,
            idle,
            bytes,
            cleanup: Duration::from_secs(10),
        },
        || false,
        |_, _| {},
        |stream, bytes| {
            crate::progress::completed(
                if stream == 0 {
                    "child-stdout"
                } else {
                    "child-stderr"
                },
                bytes.len(),
            );
            on_chunk(stream, bytes);
        },
    );
    match result {
        Ok(output) => Ok(output),
        Err(mut error) => {
            error.settle(Duration::from_secs(10))?;
            Err(error.into())
        }
    }
}
