//! Explicit scheduling mutation controls for native worker regression tests.
//! This entire module and its call sites are absent without `test-support`.
//! The driver protocol cannot select a mutation or a trace destination.
use crate::native_input::ReadObserver;
use serde_json::{Value, json};
use std::{
    fs::{File, OpenOptions},
    io::{self, Write},
    sync::{Arc, Mutex},
    time::Duration,
};
struct Trace {
    file: Option<File>,
    events: u16,
    last_wait: Option<u64>,
    idle_recorded: bool,
}
pub(super) struct Schedule {
    mutation: String,
    trace: Mutex<Trace>,
}
impl Schedule {
    pub(super) fn from_environment() -> io::Result<Arc<Self>> {
        let mutation = std::env::var("NUMERA_TEST_SDK_SCHEDULING").unwrap_or_default();
        if !matches!(mutation.as_str(), "" | "poll" | "pump" | "fragment-wait") {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Unknown test scheduling mutation",
            ));
        }
        let file = std::env::var_os("NUMERA_TEST_SDK_TRACE")
            .map(|p| OpenOptions::new().create_new(true).write(true).open(p))
            .transpose()?;
        Ok(Arc::new(Self {
            mutation,
            trace: Mutex::new(Trace {
                file,
                events: 0,
                last_wait: None,
                idle_recorded: false,
            }),
        }))
    }
    pub(super) fn wait(&self) -> Duration {
        if matches!(self.mutation.as_str(), "poll" | "fragment-wait") {
            Duration::ZERO
        } else {
            Duration::from_millis(25)
        }
    }
    fn event(trace: &mut Trace, value: Value) {
        if trace.events >= 128 {
            return;
        }
        trace.events += 1;
        if let Some(file) = &mut trace.file {
            let _ = writeln!(file, "{value}");
            let _ = file.flush();
        }
    }
    pub(super) fn idle(&self, context_open: bool) {
        if !context_open {
            return;
        }
        let delay = if self.mutation == "pump" { 25 } else { 0 };
        if let Ok(mut trace) = self.trace.lock()
            && !trace.idle_recorded
        {
            trace.idle_recorded = true;
            Self::event(&mut trace, json!({"event":"idle_delay","ms":delay}));
        }
        if delay > 0 {
            std::thread::park_timeout(Duration::from_millis(delay));
        }
    }
}
impl ReadObserver for Schedule {
    fn waiting(&self, wait: Duration) {
        if let Ok(mut trace) = self.trace.lock() {
            let ms = wait.as_millis() as u64;
            if trace.last_wait != Some(ms) {
                trace.last_wait = Some(ms);
                Self::event(&mut trace, json!({"event":"input_wait","ms":ms}));
            }
        }
    }
    fn read(&self, count: usize, contains_newline: bool) {
        let delay = if self.mutation == "fragment-wait" && count > 0 && !contains_newline {
            25
        } else {
            0
        };
        if let Ok(mut trace) = self.trace.lock() {
            Self::event(
                &mut trace,
                json!({"event":"read","count":count,"complete":contains_newline}),
            );
            if count > 0 && !contains_newline {
                Self::event(&mut trace, json!({"event":"fragment_delay","ms":delay}));
            }
        }
        if delay > 0 {
            std::thread::park_timeout(Duration::from_millis(delay));
        }
    }
}
