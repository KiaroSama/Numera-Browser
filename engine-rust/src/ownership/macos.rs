//! macOS kernel observations; uncertainty never authorizes releasing a tree.
#![allow(unsafe_code)]
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    io,
    mem::{size_of, size_of_val},
};

const GROUP_ONLY: u32 = 2; // Apple's PROC_PGRP_ONLY, absent from libc's constants.
const MAX_PIDS: usize = 65_536;

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn process(pid: u32) -> io::Result<Option<libc::proc_bsdinfo>> {
    if pid == 0 {
        return Ok(None);
    }
    let native = i32::try_from(pid)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "Invalid process PID"))?;
    // SAFETY: this C record consists only of integer fields and byte arrays.
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let (returned, error) = process_observation(native, &mut info);
    if returned == 0 && error == libc::ESRCH {
        return Ok(None);
    }
    if error != 0 {
        return Err(io::Error::from_raw_os_error(error));
    }
    if returned != size_of::<libc::proc_bsdinfo>() as i32
        || info.pbi_pid != pid
        || !(libc::SIDL..=libc::SZOMB).contains(&info.pbi_status)
    {
        return Err(invalid(
            "macOS process observation is incomplete or inconsistent",
        ));
    }
    Ok(Some(info))
}

pub fn started_at(pid: u32) -> io::Result<Option<u64>> {
    process(pid)?.map(|info| birth_token(&info)).transpose()
}
pub fn alive(pid: u32, token: Option<u64>) -> io::Result<bool> {
    let Some(info) = process(pid)? else {
        return Ok(false);
    };
    // POSIX kill(pid, 0), used by the original, also sees an unreaped zombie.
    match token {
        Some(expected) => Ok(birth_token(&info)? == expected),
        None => Ok(true),
    }
}

fn birth_token(info: &libc::proc_bsdinfo) -> io::Result<u64> {
    if info.pbi_start_tvusec >= 1_000_000 {
        return Err(invalid("macOS process birth microseconds are invalid"));
    }
    let seconds = libc::time_t::try_from(info.pbi_start_tvsec)
        .map_err(|_| invalid("macOS process birth seconds exceed the platform range"))?;
    // SAFETY: initialized output, a valid timestamp pointer, no shared tm storage.
    let mut calendar: libc::tm = unsafe { std::mem::zeroed() };
    if unsafe { libc::localtime_r(&seconds, &mut calendar) }.is_null() {
        return Err(invalid("macOS process birth time cannot be represented"));
    }
    // ps calls setlocale(LC_ALL, ""). Own the equivalent locale without changing
    // global process state; an invalid environment makes ps retain its C locale.
    // SAFETY: NUL-terminated environment selector, null base; a new owned locale.
    let mut locale =
        unsafe { libc::newlocale(libc::LC_ALL_MASK, c"".as_ptr(), std::ptr::null_mut()) };
    if locale.is_null() {
        // SAFETY: literal C locale, null base; returns a new owned locale object.
        locale = unsafe { libc::newlocale(libc::LC_ALL_MASK, c"C".as_ptr(), std::ptr::null_mut()) };
    }
    if locale.is_null() {
        return Err(io::Error::last_os_error());
    }
    struct Locale(libc::locale_t);
    impl Drop for Locale {
        fn drop(&mut self) {
            // SAFETY: uniquely owned newlocale result, released exactly once.
            unsafe { libc::freelocale(self.0) };
        }
    }
    let locale = Locale(locale);
    // Match Apple's ps lstarted buffer and format, not an incompatible epoch token.
    let mut output = [0u8; 100];
    // SAFETY: writable output with its actual bound, valid calendar/owned locale.
    let length = unsafe {
        libc::strftime_l(
            output.as_mut_ptr().cast(),
            99,
            c"%c".as_ptr(),
            &calendar,
            locale.0,
        )
    };
    if length == 0 || length >= 99 {
        return Err(invalid("macOS process birth formatting is incomplete"));
    }
    let text = std::str::from_utf8(&output[..length]).map_err(io::Error::other)?;
    let normalized = text
        .split(crate::pool::whitespace)
        .filter(|field| !field.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if normalized.is_empty() {
        return Err(invalid("macOS process birth formatting is empty"));
    }
    Ok(Sha256::digest(normalized.as_bytes())[..6]
        .iter()
        .fold(0, |value, byte| (value << 8) | u64::from(*byte)))
}

fn group_query(group: i32, buffer: &mut [i32]) -> io::Result<usize> {
    let (returned, error) = group_observation(group, buffer);
    if error != 0 {
        return Err(io::Error::from_raw_os_error(error));
    }
    let bytes = usize::try_from(returned)
        .map_err(|_| invalid("macOS process group query failed without an OS error"))?;
    if !bytes.is_multiple_of(size_of::<i32>())
        || (!buffer.is_empty() && bytes > size_of_val(buffer))
    {
        return Err(invalid(
            "macOS process group query returned a partial or oversized buffer",
        ));
    }
    Ok(bytes / size_of::<i32>())
}
pub(super) fn group_empty(group: i32) -> io::Result<bool> {
    if group <= 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Invalid process group",
        ));
    }
    let mut pids = vec![0i32; 64];
    let mut count = group_query(group, &mut pids)?;
    if count == pids.len() {
        let hint = group_query(group, &mut [])?;
        let capacity = hint.max(pids.len() * 2);
        if capacity > MAX_PIDS {
            return Err(invalid("macOS process group exceeds the observation bound"));
        }
        pids.resize(capacity, 0);
        count = group_query(group, &mut pids)?;
        if count == pids.len() {
            return Err(invalid(
                "macOS process group remains truncated; emptiness unknown",
            ));
        }
    }
    pids.truncate(count);
    let mut unique = BTreeSet::new();
    if pids.iter().any(|pid| *pid <= 0 || !unique.insert(*pid)) {
        return Err(invalid(
            "macOS process group contains invalid or duplicate PIDs",
        ));
    }
    let mut unknown = false;
    for pid in pids {
        match process(pid as u32) {
            Ok(Some(info)) if info.pbi_pgid != group as u32 => unknown = true,
            Ok(Some(info)) if info.pbi_status != libc::SZOMB => return Ok(false),
            Ok(_) => {}
            Err(_) => unknown = true,
        }
    }
    if unknown {
        return Err(invalid(
            "macOS process group observation incomplete; retain its claim",
        ));
    }
    Ok(true)
}

fn process_observation(pid: i32, info: &mut libc::proc_bsdinfo) -> (i32, i32) {
    #[cfg(test)]
    if let Some(observed) = OBSERVATIONS.with(|slot| {
        slot.borrow_mut()
            .as_mut()
            .map(|rows| rows.pop_front().expect("Missing process observation"))
    }) {
        let Observation::Process {
            pid: expected,
            info: supplied,
            returned,
            error,
        } = observed
        else {
            panic!("Expected process observation");
        };
        assert_eq!(pid as u32, expected);
        *info = supplied;
        return (returned, error);
    }
    // SAFETY: errno is thread-local. Complete initialized C output with its exact
    // size; arg=1 permits zombie records. No signal or process capability is taken.
    unsafe {
        *libc::__error() = 0;
        let returned = libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            1,
            (info as *mut libc::proc_bsdinfo).cast(),
            size_of::<libc::proc_bsdinfo>() as i32,
        );
        (returned, *libc::__error())
    }
}
fn group_observation(group: i32, buffer: &mut [i32]) -> (i32, i32) {
    #[cfg(test)]
    if let Some(observed) = OBSERVATIONS.with(|slot| {
        slot.borrow_mut()
            .as_mut()
            .map(|rows| rows.pop_front().expect("Missing group observation"))
    }) {
        let Observation::Group {
            group: expected,
            capacity,
            pids,
            returned,
            error,
        } = observed
        else {
            panic!("Expected group observation");
        };
        assert_eq!(group, expected);
        assert_eq!(buffer.len(), capacity);
        assert!(pids.len() <= buffer.len());
        buffer[..pids.len()].copy_from_slice(&pids);
        return (returned, error);
    }
    let pointer = if buffer.is_empty() {
        std::ptr::null_mut()
    } else {
        buffer.as_mut_ptr().cast()
    };
    // SAFETY: valid positive group, writable initialized PID slice and actual byte
    // capacity, or a null size query. Byte API preserves partial-record evidence.
    unsafe {
        *libc::__error() = 0;
        let returned = libc::proc_listpids(
            GROUP_ONLY,
            group as u32,
            pointer,
            size_of_val(buffer) as i32,
        );
        (returned, *libc::__error())
    }
}

#[cfg(test)]
#[derive(Clone)]
enum Observation {
    Process {
        pid: u32,
        info: libc::proc_bsdinfo,
        returned: i32,
        error: i32,
    },
    Group {
        group: i32,
        capacity: usize,
        pids: Vec<i32>,
        returned: i32,
        error: i32,
    },
}
#[cfg(test)]
std::thread_local! {
    static OBSERVATIONS: std::cell::RefCell<Option<std::collections::VecDeque<Observation>>> =
        const { std::cell::RefCell::new(None) };
}
#[cfg(test)]
#[path = "macos_tests.rs"]
mod tests;
