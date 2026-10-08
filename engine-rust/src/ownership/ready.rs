use crate::conversion::{truth, value_text};
use serde_json::Value;
use std::{
    fmt, fs, io,
    path::Path,
    thread,
    time::{Duration, Instant},
};

#[derive(Debug)]
pub struct StartupFailed {
    pub kind: String,
    pub message: String,
}
impl fmt::Display for StartupFailed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for StartupFailed {}
fn result(path: &Path) -> Option<Value> {
    let bytes = fs::read(path).ok()?;
    if bytes.len() > 8192 {
        return None;
    }
    serde_json::from_slice::<Value>(&bytes)
        .ok()
        .filter(Value::is_object)
}
fn answer(value: &Value) -> Result<(), StartupFailed> {
    if truth(&value["ok"]) {
        return Ok(());
    }
    Err(StartupFailed {
        kind: value
            .get("kind")
            .filter(|value| truth(value))
            .map_or_else(|| "failed".into(), value_text),
        message: value
            .get("error")
            .filter(|value| truth(value))
            .map_or_else(|| "the session failed to start".into(), value_text),
    })
}
/// The adapter writes success only after its persistent browser context opens.
/// An ownership lease or a spawned process alone is never a readiness result.
pub fn await_ready(
    path: &Path,
    timeout: Duration,
    mut exited: impl FnMut() -> io::Result<Option<i32>>,
) -> Result<(), StartupFailed> {
    let deadline = Instant::now()
        .checked_add(timeout)
        .ok_or_else(|| StartupFailed {
            kind: "failed".into(),
            message: "Session readiness budget is invalid".into(),
        })?;
    loop {
        if let Some(value) = result(path) {
            return answer(&value);
        }
        let code = exited().map_err(|error| StartupFailed {
            kind: "failed".into(),
            message: format!("Session process status could not be read: {error}"),
        })?;
        if let Some(code) = code {
            if let Some(value) = result(path) {
                return answer(&value);
            }
            return Err(StartupFailed {
                kind: "failed".into(),
                message: format!(
                    "the session process exited with code {code} before it opened the profile"
                ),
            });
        }
        if Instant::now() >= deadline {
            return Err(StartupFailed {
                kind: "timeout".into(),
                message: format!(
                    "the session did not report itself started within {:.0}s",
                    timeout.as_secs_f64()
                ),
            });
        }
        thread::sleep(
            deadline
                .saturating_duration_since(Instant::now())
                .min(Duration::from_millis(50)),
        );
    }
}
