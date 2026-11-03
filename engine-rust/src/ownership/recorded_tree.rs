//! Reopen a durable tree capability, verify ownership, signal and prove empty.
use super::{identity, tree};
use serde_json::Value;
#[cfg(unix)]
use std::thread;
use std::{
    io,
    time::{Duration, Instant},
};

pub fn verified(record: &Value) -> bool {
    (record["tree"]["kind"] == "job"
        && record["tree"]["group"]
            .as_str()
            .is_some_and(|s| !s.is_empty()))
        || record["pidToken"].as_u64().is_some()
}
fn pid(record: &Value) -> io::Result<u32> {
    record["pid"]
        .as_u64()
        .and_then(|v| u32::try_from(v).ok())
        .filter(|v| *v > 0)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Stored job PID is invalid"))
}
pub fn owned(record: &Value) -> io::Result<bool> {
    if record["tree"]["kind"] == "job"
        && record["tree"]["group"]
            .as_str()
            .is_some_and(|s| !s.is_empty())
    {
        return Ok(true);
    }
    let Some(token) = record["pidToken"].as_u64() else {
        return Ok(false);
    };
    let pid = pid(record)?;
    if record["tree"]["kind"] == "pgid" && record["tree"]["group"].as_u64() != Some(u64::from(pid))
    {
        return Err(io::Error::other(
            "Stored group does not match its captured leader",
        ));
    }
    Ok(identity::started_at(pid)?.is_none_or(|actual| actual == token))
}
pub fn empty(record: &Value) -> io::Result<bool> {
    match record["tree"]["kind"].as_str() {
        Some("job" | "pgid") => tree::record_empty(record),
        _ => Ok(!identity::alive(pid(record)?, record["pidToken"].as_u64())?),
    }
}
pub fn settle(record: &Value, budget: Duration) -> io::Result<bool> {
    if !verified(record) {
        return Err(io::Error::other(
            "Stored job has no verified process identity",
        ));
    }
    let end = Instant::now()
        .checked_add(budget)
        .ok_or_else(|| io::Error::other("Invalid job stop budget"))?;
    #[cfg(windows)]
    if record["tree"]["kind"] == "job" {
        let name = record["tree"]["group"]
            .as_str()
            .ok_or_else(|| io::Error::other("Stored job name missing"))?;
        return super::windows::settle_named(name, end);
    }
    if empty(record)? {
        return Ok(true);
    }
    #[cfg(unix)]
    {
        if !owned(record)? {
            return Ok(false);
        }
        signal(record, false)?;
        let midpoint = Instant::now() + end.saturating_duration_since(Instant::now()) / 2;
        if prove(record, midpoint)? {
            return Ok(true);
        }
        if !owned(record)? {
            return Ok(false);
        }
        signal(record, true)?;
        prove(record, end)
    }
    #[cfg(windows)]
    {
        // Legacy records contain no named tree. Never walk recycled parent links.
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Legacy Windows job has no durable tree handle; end it where it was started",
        ))
    }
    #[cfg(not(any(unix, windows)))]
    {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Stored job stop is unsupported on this platform",
        ))
    }
}
#[cfg(unix)]
#[allow(unsafe_code)]
fn signal(record: &Value, force: bool) -> io::Result<()> {
    let target = if record["tree"]["kind"] == "pgid" {
        let group = record["tree"]["group"]
            .as_i64()
            .and_then(|v| i32::try_from(v).ok())
            .filter(|v| *v > 0)
            .ok_or_else(|| io::Error::other("Stored process group is invalid"))?;
        // SAFETY: getpgrp has no arguments or borrowed storage.
        if group == unsafe { libc::getpgrp() } {
            return Err(io::Error::other(
                "Refusing to stop the current process group",
            ));
        }
        -group
    } else {
        i32::try_from(pid(record)?).map_err(io::Error::other)?
    };
    // SAFETY: the durable birth identity was rechecked immediately before signal;
    // positive PID or a validated distinct group, never zero or the current group.
    if unsafe { libc::kill(target, if force { libc::SIGKILL } else { libc::SIGTERM }) } != 0 {
        let error = io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::ESRCH) {
            return Err(error);
        }
    }
    Ok(())
}
#[cfg(unix)]
fn prove(record: &Value, end: Instant) -> io::Result<bool> {
    loop {
        if empty(record)? {
            return Ok(true);
        }
        if Instant::now() >= end {
            return Ok(false);
        }
        thread::sleep(
            end.saturating_duration_since(Instant::now())
                .min(Duration::from_millis(20)),
        );
    }
}
