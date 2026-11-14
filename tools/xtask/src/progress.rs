//! Activity is published only after real I/O completes. This keeps an enclosing
//! finite owner informed without turning a silent or hung process into progress.
use std::{
    collections::BTreeMap,
    io::{self, Read, Write},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

struct State {
    last: Instant,
    bytes: BTreeMap<&'static str, u64>,
}

pub fn completed(operation: &'static str, count: usize) {
    if count == 0 {
        return;
    }
    static STATE: OnceLock<Mutex<State>> = OnceLock::new();
    let mut state = STATE
        .get_or_init(|| {
            Mutex::new(State {
                last: Instant::now(),
                bytes: BTreeMap::new(),
            })
        })
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let total = state.bytes.entry(operation).or_default();
    *total = total.saturating_add(count as u64);
    let total = *total;
    if state.last.elapsed() >= Duration::from_secs(1) {
        eprintln!("Native build activity operation={operation} completed_bytes={total}");
        let _ = io::stderr().flush();
        state.last = Instant::now();
    }
}

pub fn copy(source: &mut impl Read, destination: &mut impl Write) -> io::Result<u64> {
    let mut total = 0u64;
    let mut buffer = [0u8; 65536];
    loop {
        let count = match source.read(&mut buffer) {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        if count == 0 {
            return Ok(total);
        }
        destination.write_all(&buffer[..count])?;
        total = total
            .checked_add(count as u64)
            .ok_or_else(|| io::Error::other("Build copy byte counter overflow"))?;
        completed("copied", count);
    }
}
