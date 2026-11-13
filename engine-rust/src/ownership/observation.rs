//! Complete process-group observations and bounded numeric diagnostics. Reporting
//! reads only the last owned observation and never extends a cleanup deadline.
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    io,
};
const CACHE_GROUPS: usize = 64;
#[derive(Clone, Debug)]
pub struct Row {
    pub pid: u32,
    pub group: i32,
    pub state_codes: Vec<u8>,
}
pub struct Enumeration {
    pub pids: Vec<u32>,
    pub complete: bool,
    pub filtered: bool,
}
pub trait GroupSource {
    fn enumerate(&mut self, group: i32) -> io::Result<Enumeration>;
    fn process(&mut self, pid: u32) -> io::Result<Option<Row>>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignalReason {
    Delivered,
    OwnershipMismatch,
    CallerGroup,
    BirthError,
    SignalError,
    InvalidIdentity,
}
impl SignalReason {
    fn name(self) -> &'static str {
        match self {
            Self::Delivered => "delivered",
            Self::OwnershipMismatch => "ownership_mismatch",
            Self::CallerGroup => "caller_group",
            Self::BirthError => "birth_error",
            Self::SignalError => "signal_error",
            Self::InvalidIdentity => "invalid_identity",
        }
    }
}
#[derive(Clone, Debug)]
pub struct SignalAttempt {
    pub delivered: bool,
    pub force: bool,
    pub reason: SignalReason,
    pub errno: Option<i32>,
}
#[derive(Clone, Copy)]
pub struct SignalIdentity {
    pub pid: u32,
    pub token: Option<u64>,
    pub group: Option<i32>,
}
pub trait SignalSource {
    fn birth(&mut self, pid: u32) -> io::Result<Option<u64>>;
    fn caller_group(&mut self) -> i32;
    fn deliver(&mut self, target: i32, force: bool) -> io::Result<()>;
}
#[derive(Clone, Default)]
struct Cached {
    sample: Option<Row>,
    query_failed: bool,
    query_errno: Option<i32>,
    signal: Option<SignalAttempt>,
}
thread_local! {static CACHE:RefCell<BTreeMap<i32,Cached>>=const{RefCell::new(BTreeMap::new())};}
fn update(group: i32, operation: impl FnOnce(&mut Cached)) {
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if !cache.contains_key(&group)
            && cache.len() == CACHE_GROUPS
            && let Some(key) = cache.keys().next().copied()
        {
            cache.remove(&key);
        }
        operation(cache.entry(group).or_default());
    });
}
fn begin(group: i32) {
    update(group, |cache| {
        cache.sample = None;
        cache.query_failed = false;
        cache.query_errno = None;
    });
}
fn failed(group: i32, error: &io::Error) {
    update(group, |cache| {
        cache.sample = None;
        cache.query_failed = true;
        cache.query_errno = error.raw_os_error();
    });
}
fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "Process group observation is incomplete; retain its claim",
    )
}
fn valid_state(state: &[u8]) -> bool {
    state.len() <= 16
        && state
            .first()
            .is_some_and(|first| b"RSDZTtXxKWPIU".contains(first))
        && state
            .iter()
            .all(|c| c.is_ascii_alphabetic() || b"<>+-".contains(c))
}
/// `true` proves empty. An error is uncertainty, and is never interchangeable
/// with emptiness. A known live member remains authoritative despite another
/// process disappearing while the same complete table is inspected.
pub fn observe_group(
    group: i32,
    ignore: Option<u32>,
    source: &mut impl GroupSource,
) -> io::Result<bool> {
    begin(group);
    if group <= 0 {
        let error = io::Error::new(io::ErrorKind::InvalidInput, "Invalid process group");
        failed(group, &error);
        return Err(error);
    }
    let enumeration = match source.enumerate(group) {
        Ok(value) => value,
        Err(error) => {
            failed(group, &error);
            return Err(io::Error::new(
                error.kind(),
                "Process group query failed; ownership remains uncertain",
            ));
        }
    };
    if !enumeration.complete {
        let error = invalid();
        failed(group, &error);
        return Err(error);
    }
    let mut seen = BTreeSet::new();
    if enumeration
        .pids
        .iter()
        .any(|pid| *pid == 0 || !seen.insert(*pid))
    {
        let error = invalid();
        failed(group, &error);
        return Err(error);
    }
    let mut uncertain = None;
    let mut sample = None;
    for pid in enumeration.pids {
        if Some(pid) == ignore {
            continue;
        }
        let row = match source.process(pid) {
            Ok(Some(row)) => row,
            Ok(None) => continue,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => {
                uncertain = Some(error);
                continue;
            }
        };
        if row.pid != pid || row.group <= 0 || !valid_state(&row.state_codes) {
            uncertain = Some(invalid());
            continue;
        }
        if row.group != group {
            if enumeration.filtered {
                uncertain = Some(invalid());
            }
            continue;
        }
        if row.state_codes.first() != Some(&b'Z') {
            update(group, |cache| cache.sample = Some(row));
            return Ok(false);
        }
        if sample.is_none() {
            sample = Some(row);
        }
    }
    if let Some(error) = uncertain {
        failed(group, &error);
        return Err(io::Error::new(
            error.kind(),
            "Process table incomplete; tree emptiness unknown",
        ));
    }
    update(group, |cache| cache.sample = sample);
    Ok(true)
}
/// Parse the group/state fields used by the actual Linux scanner without ever
/// retaining a process command name in its diagnostic cache.
pub fn procfs_row(pid: u32, text: &str) -> io::Result<Row> {
    let (row, _) = parse_procfs_row(pid, text)?;
    if row.group <= 0 {
        return Err(invalid());
    }
    Ok(row)
}
/// A Linux kernel thread cannot belong to a positive owned process group.
/// Its kernel-provided PF_KTHREAD flag distinguishes a legitimate group-zero
/// row from a malformed observation of a user process.
pub fn procfs_process(pid: u32, text: &str) -> io::Result<Option<Row>> {
    const PF_KTHREAD: u64 = 0x0020_0000;
    let (row, flags) = parse_procfs_row(pid, text)?;
    if row.group == 0 && flags.is_some_and(|flags| flags & PF_KTHREAD != 0) {
        return Ok(None);
    }
    if row.group <= 0 {
        return Err(invalid());
    }
    Ok(Some(row))
}
fn parse_procfs_row(pid: u32, text: &str) -> io::Result<(Row, Option<u64>)> {
    let (header, rest) = text.rsplit_once(") ").ok_or_else(invalid)?;
    let reported = header
        .split_once('(')
        .ok_or_else(invalid)?
        .0
        .trim()
        .parse::<u32>()
        .map_err(|_| invalid())?;
    if pid == 0 || reported != pid {
        return Err(invalid());
    }
    let fields = rest.split_whitespace().collect::<Vec<_>>();
    if fields.len() < 3 {
        return Err(invalid());
    }
    let group = fields[2].parse::<i32>().map_err(|_| invalid())?;
    let row = Row {
        pid,
        group,
        state_codes: fields[0].as_bytes().to_vec(),
    };
    if !valid_state(&row.state_codes) {
        return Err(invalid());
    }
    let flags = fields.get(6).and_then(|flags| flags.parse().ok());
    Ok((row, flags))
}
/// Birth and caller-group checks precede one signal attempt. Kernel failures are
/// reduced to numeric errno; no OS exception string is retained or printed.
pub fn attempt_signal(
    identity: SignalIdentity,
    force: bool,
    source: &mut impl SignalSource,
) -> SignalAttempt {
    let group = identity
        .group
        .unwrap_or_else(|| i32::try_from(identity.pid).unwrap_or(0));
    let mut attempt = SignalAttempt {
        delivered: false,
        force,
        reason: SignalReason::InvalidIdentity,
        errno: None,
    };
    (|| {
        if identity.pid == 0
            || identity.pid > i32::MAX as u32
            || identity.token.is_none()
            || group <= 0
            || identity
                .group
                .is_some_and(|g| u32::try_from(g).ok() != Some(identity.pid))
        {
            return;
        }
        match source.birth(identity.pid) {
            Ok(Some(actual)) if Some(actual) != identity.token => {
                attempt.reason = SignalReason::OwnershipMismatch;
                return;
            }
            Err(error) => {
                attempt.reason = SignalReason::BirthError;
                attempt.errno = error.raw_os_error();
                return;
            }
            _ => {}
        }
        if identity
            .group
            .is_some_and(|group| group == source.caller_group())
        {
            attempt.reason = SignalReason::CallerGroup;
            return;
        }
        let target = if identity.group.is_some() {
            -group
        } else {
            identity.pid as i32
        };
        match source.deliver(target, force) {
            Ok(()) => {
                attempt.delivered = true;
                attempt.reason = SignalReason::Delivered;
            }
            Err(error) => {
                attempt.reason = SignalReason::SignalError;
                attempt.errno = error.raw_os_error();
            }
        }
    })();
    update(group, |cache| {
        cache.sample = None;
        cache.signal = Some(attempt.clone());
    });
    attempt
}
pub fn diagnostic(group: i32, graceful: bool, forced: bool, verified: bool) -> String {
    let cached = CACHE
        .with(|cache| cache.borrow().get(&group).cloned())
        .unwrap_or_default();
    let mut text = format!(
        "Owned group failure group={group} graceful_signal={graceful} force_signal={forced} verified={verified} query_failed={}",
        cached.query_failed
    );
    if let Some(errno) = cached.query_errno {
        text.push_str(&format!(" query_errno={errno}"));
    }
    if let Some(row) = cached.sample {
        text.push_str(&format!(
            " pid={} state_codes=({})",
            row.pid,
            row.state_codes
                .iter()
                .map(u8::to_string)
                .collect::<Vec<_>>()
                .join(",")
                + if row.state_codes.len() == 1 { "," } else { "" }
        ));
    }
    if let Some(signal) = cached.signal {
        text.push_str(&format!(
            " reason={} force={}",
            signal.reason.name(),
            signal.force
        ));
        if let Some(errno) = signal.errno {
            text.push_str(&format!(" signal_errno={errno}"));
        }
    }
    text
}
