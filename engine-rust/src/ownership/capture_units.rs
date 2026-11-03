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
}
impl Units {
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
        })
    }
    pub fn settle(&mut self, budget: Duration) -> io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        // A borrowed parent report must retain every other live/pending scope.
        // The unfiltered supervisor sees all scopes, including this call's SDKs.
        let scope = (!self.owned).then_some(self.scope.as_str());
        if !unit_report::settle_scoped(path, budget, scope)? {
            return Err(io::Error::other(
                "Captured nested ownership remains incomplete",
            ));
        }
        if self.owned {
            fs::remove_file(path)?;
        }
        self.path = None;
        Ok(())
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
