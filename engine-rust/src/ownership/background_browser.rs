use crate::browser::adapter::Adapter;
use std::{
    io,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

type Finished = (Adapter, i32, Option<io::Error>);
static RETAINED: OnceLock<Mutex<Vec<Background>>> = OnceLock::new();
#[cfg(test)]
#[path = "background_retention_tests.rs"]
mod tests;

pub fn retry_retained(budget: Duration) -> Vec<String> {
    let retained = RETAINED.get_or_init(|| Mutex::new(Vec::new()));
    let owners = std::mem::take(
        &mut *retained
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    );
    let end = Instant::now()
        .checked_add(budget)
        .unwrap_or_else(Instant::now);
    let mut errors = Vec::new();
    let mut pending = Vec::new();
    for mut owner in owners {
        if let Err(error) = owner.finish(end.saturating_duration_since(Instant::now())) {
            errors.push(error.to_string());
            pending.push(owner);
        }
    }
    retained
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .extend(pending);
    errors
}

/// The worker holds the admitted adapter; cancellation never drops its tree.
/// An unsettled adapter comes back to this owner for later stop/shutdown retry.
pub(super) struct Background {
    worker: Option<JoinHandle<Finished>>,
    cancelled: Arc<AtomicBool>,
    pending: Option<Finished>,
}
impl Background {
    fn retain(&mut self) {
        let retained = Self {
            worker: self.worker.take(),
            pending: self.pending.take(),
            cancelled: self.cancelled.clone(),
        };
        RETAINED
            .get_or_init(|| Mutex::new(Vec::new()))
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(retained);
    }
    pub(super) fn start(
        adapter: &mut Option<Adapter>,
        operation: impl FnOnce(&mut Adapter, Arc<AtomicBool>) -> io::Result<()> + Send + 'static,
    ) -> io::Result<Self> {
        if adapter.is_none() {
            return Err(io::Error::other("Background adapter is missing"));
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        let signal = cancelled.clone();
        let (sender, receiver) = std::sync::mpsc::sync_channel::<Adapter>(1);
        let worker = thread::Builder::new()
            .name("numera-warmup".into())
            .spawn(move || {
                let mut adapter = receiver
                    .recv()
                    .expect("Started worker receives its native owner");
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    operation(&mut adapter, signal.clone())
                }));
                let result = match result {
                    Ok(result) => result,
                    Err(_) => Err(io::Error::other("Native browser operation panicked")),
                };
                let code = if result.is_ok() && !signal.load(Ordering::Acquire) {
                    0
                } else {
                    1
                };
                let cleanup = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    adapter.close(Duration::from_secs(10))
                }));
                let error = match cleanup {
                    Ok(Err(error)) => Some(error),
                    Ok(Ok(Some(0))) => None,
                    Ok(Ok(_)) => Some(io::Error::other(
                        "Background browser did not close successfully",
                    )),
                    Err(_) => Some(io::Error::other(
                        "Background browser cleanup panicked; owner retained",
                    )),
                };
                if let Err(error) = result {
                    eprintln!("Background browser operation failed: {error}");
                }
                (adapter, if error.is_some() { 1 } else { code }, error)
            })?;
        let owner = adapter
            .take()
            .ok_or_else(|| io::Error::other("Background adapter is missing"))?;
        if let Err(error) = sender.send(owner) {
            *adapter = Some(error.0);
            worker
                .join()
                .map_err(|_| io::Error::other("Background handoff worker panicked"))?;
            return Err(io::Error::other("Background adapter handoff failed"));
        }
        Ok(Self {
            worker: Some(worker),
            cancelled,
            pending: None,
        })
    }
    fn collect(&mut self) -> io::Result<bool> {
        if self
            .worker
            .as_ref()
            .is_some_and(|worker| !worker.is_finished())
        {
            return Ok(false);
        }
        if let Some(worker) = self.worker.take() {
            self.pending = Some(worker.join().map_err(|_| {
                io::Error::other("Background browser worker panicked during cleanup")
            })?);
        }
        Ok(true)
    }
    pub(super) fn settled_exit(&mut self) -> io::Result<Option<i32>> {
        if !self.collect()? {
            return Ok(None);
        }
        let Some((_, code, error)) = &self.pending else {
            return Err(io::Error::other(
                "Background browser worker result is missing",
            ));
        };
        if error.is_some() {
            return Ok(None);
        }
        Ok(Some(*code))
    }
    pub(super) fn finish(&mut self, budget: Duration) -> io::Result<Option<i32>> {
        self.cancelled.store(true, Ordering::Release);
        let end = Instant::now()
            .checked_add(budget)
            .ok_or_else(|| io::Error::other("Background cleanup deadline is invalid"))?;
        while !self.collect()? {
            if Instant::now() >= end {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Background browser is still settling; its owner is retained",
                ));
            }
            thread::sleep(
                end.saturating_duration_since(Instant::now())
                    .min(Duration::from_millis(10)),
            );
        }
        if self.pending.is_none() && self.worker.is_none() {
            return Ok(Some(0));
        }
        let Some((adapter, code, error)) = self.pending.as_mut() else {
            return Err(io::Error::other("Background browser result is missing"));
        };
        if error.is_some() {
            adapter.finish(end.saturating_duration_since(Instant::now()))?;
            *error = None;
        }
        Ok(Some(*code))
    }
}
impl Drop for Background {
    fn drop(&mut self) {
        if self.worker.is_none() && self.pending.is_none() {
            return;
        }
        if let Err(error) = self.finish(Duration::from_secs(2)) {
            eprintln!("Background browser owner remains unsettled and retained for retry: {error}");
            self.retain();
        }
    }
}
