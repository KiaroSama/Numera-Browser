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
/// Verify a receipt's observer against this process's actual kernel identity.
/// This only queries membership; it never joins or adopts the recorded tree.
/// Windows membership may include an ancestor Job; no innermost-Job claim or
/// signal authority is established by this read-only predicate.
pub fn current_record_matches(record: &Value) -> io::Result<bool> {
    let current = std::process::id();
    if record["pid"].as_u64() != Some(u64::from(current)) {
        return Ok(false);
    }
    let Some(birth) = record["pidToken"].as_u64() else {
        return Ok(false);
    };
    if identity::started_at(current)? != Some(birth) {
        return Ok(false);
    }
    #[cfg(unix)]
    {
        #[allow(unsafe_code)]
        // SAFETY: getpgrp has no pointers or side effects.
        let group = unsafe { libc::getpgrp() };
        Ok(record["tree"]["kind"] == "pgid"
            && group > 0
            && u32::try_from(group).ok() == Some(current)
            && record["tree"]["group"].as_u64() == Some(u64::from(current)))
    }
    #[cfg(windows)]
    {
        let Some(name) = record["tree"]["group"]
            .as_str()
            .filter(|name| !name.is_empty())
        else {
            return Ok(false);
        };
        if record["tree"]["kind"] != "job" {
            return Ok(false);
        }
        super::windows::named_contains_current(name)
    }
    #[cfg(not(any(unix, windows)))]
    Ok(false)
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
        return super::windows::settle_named(
            name,
            end,
            record["pid"]
                .as_u64()
                .and_then(|pid| u32::try_from(pid).ok()),
        );
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
    struct Native;
    impl super::observation::SignalSource for Native {
        fn birth(&mut self, pid: u32) -> io::Result<Option<u64>> {
            identity::started_at(pid)
        }
        fn caller_group(&mut self) -> i32 {
            // SAFETY: getpgrp takes no pointers and only reads caller identity.
            unsafe { libc::getpgrp() }
        }
        fn deliver(&mut self, target: i32, force: bool) -> io::Result<()> {
            #[cfg(feature = "test-support")]
            if let Some(result) = super::test_support::signal(target, force) {
                return result;
            }
            // SAFETY: the shared policy checked birth identity and excludes the
            // caller group immediately before this single signal attempt.
            if unsafe { libc::kill(target, if force { libc::SIGKILL } else { libc::SIGTERM }) } == 0
            {
                Ok(())
            } else {
                Err(io::Error::last_os_error())
            }
        }
    }
    let group = if record["tree"]["kind"] == "pgid" {
        Some(
            record["tree"]["group"]
                .as_i64()
                .and_then(|g| i32::try_from(g).ok())
                .filter(|g| *g > 0)
                .ok_or_else(|| io::Error::other("Stored process group is invalid"))?,
        )
    } else {
        None
    };
    let attempt = super::observation::attempt_signal(
        super::observation::SignalIdentity {
            pid: pid(record)?,
            token: record["pidToken"].as_u64(),
            group,
        },
        force,
        &mut Native,
    );
    if attempt.delivered || attempt.errno == Some(libc::ESRCH) {
        return Ok(());
    }
    Err(io::Error::other(super::observation::diagnostic(
        group.unwrap_or_else(|| {
            record["pid"]
                .as_i64()
                .and_then(|v| i32::try_from(v).ok())
                .unwrap_or(0)
        }),
        !force,
        false,
        true,
    )))
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
