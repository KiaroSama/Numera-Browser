//! Numeric lifetime accounting of the existing retained proof owner. Diagnostic
//! unavailability never changes a proof result; losing an owned handle does.
use super::{Result, fail};
use numera_core::ownership::memory::{ACCOUNTING_COUNTERS, Accounting};
pub trait Backend {
    fn begin(&mut self) -> Result<bool>;
    fn snapshot(&mut self) -> Result<Accounting>;
    fn finish(&mut self) -> Result<()>;
}
pub struct Scope<B> {
    index: usize,
    backend: B,
    opened: bool,
    before: Option<Accounting>,
    started: u64,
}
impl<B: Backend> Scope<B> {
    pub fn begin(index: usize, mut backend: B, started: u64) -> Self {
        let opened = backend.begin().unwrap_or(false);
        let before = if opened {
            backend.snapshot().ok()
        } else {
            None
        };
        Self {
            index,
            backend,
            opened,
            before,
            started,
        }
    }
    pub fn finish<T>(mut self, result: Result<T>, finished: u64) -> (Result<T>, Vec<String>) {
        let observed = (|| -> Result<String> {
            let before = self
                .before
                .as_ref()
                .ok_or("Historical accounting starting evidence unavailable")?;
            let after = self.backend.snapshot()?;
            if finished < self.started
                || before.counters.len() != ACCOUNTING_COUNTERS.len()
                || after.counters.len() != ACCOUNTING_COUNTERS.len()
            {
                return fail("Historical accounting counters incomplete");
            }
            let mut delta = std::collections::BTreeMap::new();
            for key in ACCOUNTING_COUNTERS {
                let previous = before
                    .counters
                    .get(key)
                    .ok_or("Historical accounting counter missing")?;
                let current = after
                    .counters
                    .get(key)
                    .ok_or("Historical accounting counter missing")?;
                delta.insert(
                    key,
                    current
                        .checked_sub(*previous)
                        .ok_or("Historical accounting counters decreased")?,
                );
            }
            Ok(format!(
                "Historical resources index={} coverage=owned-proof-scope-not-compiler-attribution wall_ns={} active_before={} active_after={} counters={}",
                self.index,
                finished - self.started,
                before.active,
                after.active,
                serde_json::to_string(&delta)?
            ))
        })();
        let mut events = vec![observed.unwrap_or_else(|_| {
            format!(
                "Historical resources index={} status=unavailable",
                self.index
            )
        })];
        if self.opened && self.backend.finish().is_err() {
            events.push(format!(
                "Historical resources index={} status=handle-close-failed",
                self.index
            ));
            if result.is_ok() {
                return (
                    fail("Historical resource query handle did not close"),
                    events,
                );
            }
        }
        (result, events)
    }
}
pub fn measure<B: Backend, T>(
    index: usize,
    backend: B,
    started: u64,
    finished: impl FnOnce() -> u64,
    operation: impl FnOnce() -> Result<T>,
) -> (Result<T>, Vec<String>) {
    let scope = Scope::begin(index, backend, started);
    let result = operation();
    scope.finish(result, finished())
}
pub fn available(platform: &str, inherited: bool) -> Result<()> {
    if platform != "win32" || !inherited {
        return fail("Historical accounting requires an existing retained Windows job");
    }
    Ok(())
}

#[cfg(windows)]
pub struct Native {
    query: Option<numera_core::ownership::memory::AccountingQuery>,
}
#[cfg(windows)]
impl Native {
    pub fn new(tree: &numera_core::ownership::tree::OwnedTree) -> Result<Self> {
        Ok(Self {
            query: Some(numera_core::ownership::memory::AccountingQuery::new(tree)?),
        })
    }
}
#[cfg(windows)]
impl Backend for Native {
    fn begin(&mut self) -> Result<bool> {
        Ok(self.query.is_some())
    }
    fn snapshot(&mut self) -> Result<Accounting> {
        Ok(self
            .query
            .as_ref()
            .ok_or("Historical accounting query no longer owned")?
            .snapshot()?)
    }
    fn finish(&mut self) -> Result<()> {
        Ok(self
            .query
            .take()
            .ok_or("Historical accounting query no longer owned")?
            .finish()?)
    }
}
