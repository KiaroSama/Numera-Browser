//! Retain nested trusted spawn receipts across captured native command failure.
use super::unit_report;
use std::{
    fs, io,
    path::{Path, PathBuf},
    process::Command,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

pub struct Units {
    path: Option<PathBuf>,
    scope: String,
    owned: bool,
    producer: Option<serde_json::Value>,
}
impl Units {
    pub fn pending(&self) -> bool {
        self.path.is_some()
    }
    pub fn diagnostic(&self, budget: Duration) -> io::Result<serde_json::Value> {
        let Some(path) = &self.path else {
            return Ok(serde_json::json!({"schema":1,"receipt_present":false}));
        };
        unit_report::diagnostic(path, budget, (!self.owned).then_some(self.scope.as_str()))
    }
    pub fn new(root: &Path, command: &mut Command) -> io::Result<Self> {
        let mut nonce = [0u8; 16];
        getrandom::fill(&mut nonce)
            .map_err(|_| io::Error::other("Capture receipt identity unavailable"))?;
        let scope = nonce
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let (path, owned) = if let Some(inherited) = std::env::var_os(unit_report::ENV) {
            (unit_report::validate(Path::new(&inherited))?, false)
        } else {
            let directory = root.join("jobs");
            fs::create_dir_all(&directory)?;
            let path = directory.join(format!("capture-{scope}.units"));
            unit_report::initialize(&path)?;
            (path, true)
        };
        command
            .env(unit_report::ENV, &path)
            .env(unit_report::SCOPE_ENV, &scope);
        Ok(Self {
            path: Some(path),
            scope,
            owned,
            producer: None,
        })
    }
    pub fn settle(&mut self, budget: Duration) -> io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if let Some(producer) = &self.producer
            && !super::recorded_tree::empty(producer)?
        {
            return Err(io::Error::other(
                "Captured producer is not proved empty; its receipt remains owned",
            ));
        }
        // A borrowed parent report must retain every other live/pending scope.
        // The unfiltered supervisor sees all scopes, including this call's SDKs.
        let scope = (!self.owned).then_some(self.scope.as_str());
        if !unit_report::settle_scoped(path, budget, scope)? {
            return Err(io::Error::other(
                "Captured nested ownership remains incomplete",
            ));
        }
        if self.owned {
            #[cfg(feature = "test-support")]
            crate::vault::testing::event("before-capture-report-remove", path)?;
            fs::remove_file(path)?;
        }
        self.path = None;
        self.producer = None;
        Ok(())
    }

    /// Settle reported SDK trees as soon as the producer tree ends, before waiting
    /// for EOF from writers those SDKs may have inherited. A live producer is
    /// ended first so it cannot add another ownership transfer after proof.
    pub fn run(
        &mut self,
        command: &mut Command,
        limits: super::capture::Limits,
        cancelled: impl FnMut() -> bool,
    ) -> Result<super::capture::Output, super::capture::Failure> {
        self.run_observed(command, limits, cancelled, |_, _| {})
    }
    pub fn run_observed(
        &mut self,
        command: &mut Command,
        limits: super::capture::Limits,
        cancelled: impl FnMut() -> bool,
        observe: impl FnMut(super::capture::Observation, &super::tree::OwnedTree),
    ) -> Result<super::capture::Output, super::capture::Failure> {
        self.run_observed_streamed(command, limits, cancelled, observe, |_, _| {})
    }
    pub fn run_observed_streamed(
        &mut self,
        command: &mut Command,
        limits: super::capture::Limits,
        mut cancelled: impl FnMut() -> bool,
        observe: impl FnMut(super::capture::Observation, &super::tree::OwnedTree),
        on_chunk: impl FnMut(usize, &[u8]),
    ) -> Result<super::capture::Output, super::capture::Failure> {
        use std::cell::Cell;
        if self.path.is_none() {
            return Err(io::Error::other("Capture receipt was already settled").into());
        }
        if let Some(producer) = &self.producer
            && !super::recorded_tree::empty(producer)?
        {
            return Err(io::Error::other("Capture still owns an unsettled producer").into());
        }
        let failed = Cell::new(false);
        let mut receipt_error = None;
        let mut attempted = false;
        let cleanup = limits.cleanup;
        let captured = super::capture::run_observed_then(
            command,
            limits,
            || cancelled() || failed.get(),
            observe,
            |settled, tree, remaining| {
                let mut record = tree.record();
                record["pidToken"] = record["token"].take();
                self.producer = Some(record);
                attempted = true;
                let settlement = if settled {
                    self.settle(remaining)
                } else {
                    Err(io::Error::other(
                        "Captured producer cleanup remains unverified; its receipt is retained",
                    ))
                };
                if let Err(error) = settlement {
                    receipt_error = Some(error);
                    failed.set(true);
                }
            },
            on_chunk,
        );
        if !attempted && let Err(error) = self.settle(cleanup) {
            receipt_error = Some(error);
        }
        if let Some(error) = receipt_error {
            return Err(match captured {
                Err(failure) => failure.context(error),
                Ok(_) => error.into(),
            });
        }
        captured
    }
}
fn retained() -> &'static Mutex<Vec<Units>> {
    static OWNERS: OnceLock<Mutex<Vec<Units>>> = OnceLock::new();
    OWNERS.get_or_init(|| Mutex::new(Vec::new()))
}
impl Drop for Units {
    fn drop(&mut self) {
        if self.settle(Duration::from_secs(2)).is_err() {
            if let Some(path) = self.path.take() {
                retained()
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .push(Self {
                        path: Some(path),
                        scope: self.scope.clone(),
                        owned: self.owned,
                        producer: self.producer.take(),
                    });
            }
            eprintln!("Captured nested ownership remains retained in its receipt");
        }
    }
}
pub fn retry_retained(budget: Duration) -> Vec<String> {
    let mut owners = std::mem::take(
        &mut *retained()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    );
    let end = Instant::now()
        .checked_add(budget)
        .unwrap_or_else(Instant::now);
    let mut errors = Vec::new();
    owners.retain_mut(
        |owner| match owner.settle(end.saturating_duration_since(Instant::now())) {
            Ok(()) => false,
            Err(error) => {
                errors.push(error.to_string());
                true
            }
        },
    );
    retained()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .extend(owners);
    errors
}
