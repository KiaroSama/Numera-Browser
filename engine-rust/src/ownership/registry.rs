use super::{claims, ready::StartupFailed, session::Session};
use crate::storage::{
    dashboard::DashboardStore, locks::SessionLease, paths::safe_id, schedules::ScheduleStore,
};
use crate::{
    browser::{adapter::Adapter, admission},
    models::Profile,
};
use serde_json::{Map, Value, json};
use std::{
    collections::BTreeMap,
    io,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Launch,
    Attach,
    ManualWarmup,
    ScheduledWarmup,
    Finite,
}
impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Launch => "launch",
            Self::Attach => "attach",
            Self::ManualWarmup => "manual_warmup",
            Self::ScheduledWarmup => "scheduled_warmup",
            Self::Finite => "finite",
        }
    }
}
#[cfg(test)]
#[path = "attachment_tests.rs"]
mod attachment_tests;
#[path = "background_browser.rs"]
mod background_browser;
#[path = "finite_browser.rs"]
mod finite_browser;
use background_browser::Background;
pub use background_browser::retry_retained as retry_retained_background;
#[path = "background_registry.rs"]
mod background_registry;
pub use background_registry::BackgroundOptions;
#[cfg(test)]
#[path = "background_tests.rs"]
mod background_tests;

enum Owner {
    Helper(Session),
    Browser(Adapter),
    Background(Background),
}
impl Owner {
    fn settled_exit(&mut self) -> io::Result<Option<i32>> {
        match self {
            Self::Helper(session) => session.settled_exit(),
            Self::Browser(adapter) => adapter.settled_exit(),
            Self::Background(background) => background.settled_exit(),
        }
    }
    fn finish(&mut self, budget: Duration) -> io::Result<Option<i32>> {
        match self {
            Self::Helper(session) => session.finish_with_status(budget),
            Self::Browser(adapter) => adapter.finish_with_status(budget),
            Self::Background(background) => background.finish(budget),
        }
    }
    fn stop(&mut self, budget: Duration) -> io::Result<Option<i32>> {
        match self {
            Self::Helper(session) => session.finish_with_status(budget),
            Self::Browser(adapter) => adapter.close(budget),
            Self::Background(background) => background.finish(budget),
        }
    }
}
struct Entry {
    owner: Owner,
    kind: Kind,
    schedule: Option<String>,
    details: Option<Value>,
}
/// One server lifetime owns only its own admitted sessions. Foreign claims are
/// observed, never placed in this registry or terminated by it.
pub struct Registry {
    root: PathBuf,
    host: String,
    dashboard: DashboardStore,
    password: Option<String>,
    schedules: ScheduleStore,
    entries: Mutex<BTreeMap<String, Entry>>,
    decisions: Mutex<BTreeMap<String, Arc<Mutex<()>>>>,
    closed: Mutex<bool>,
}
fn poisoned() -> io::Error {
    io::Error::other("Session registry lock is poisoned; ownership is not free")
}
fn now() -> io::Result<f64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_secs_f64())
}
impl Registry {
    pub fn new(root: PathBuf, host: String, password: Option<String>) -> io::Result<Self> {
        if host.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Session host identity is missing",
            ));
        }
        Ok(Self {
            dashboard: DashboardStore::new(root.clone(), password.clone()),
            password,
            schedules: ScheduleStore::new(root.clone()),
            root,
            host,
            entries: Mutex::new(BTreeMap::new()),
            decisions: Mutex::new(BTreeMap::new()),
            closed: Mutex::new(false),
        })
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn host(&self) -> &str {
        &self.host
    }

    fn decision(&self, id: &str) -> io::Result<Arc<Mutex<()>>> {
        safe_id(id).map_err(io::Error::other)?;
        Ok(self
            .decisions
            .lock()
            .map_err(|_| poisoned())?
            .entry(id.into())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone())
    }
    fn finish_metadata(&self, id: &str, entry: &Entry, code: i32, time: f64) {
        if entry.kind == Kind::Finite {
            return;
        }
        // Cleanup is already proved. A damaged/deleted record must not strand
        // another session; failed bookkeeping remains an explicit diagnostic.
        if let Err(error) = self.dashboard.patch(
            id,
            |mut record| {
                record["_last_active"] = time.into();
                Ok((record, Map::new()))
            },
            None,
            Some("preserve"),
            time,
        ) {
            eprintln!("Session activity update failed: {error}");
        }
        if entry.kind == Kind::ScheduledWarmup
            && let Some(schedule) = &entry.schedule
        {
            let outcome = json!({"lastOutcome":if code==0{"ok"}else{"failed"},"lastError":if code==0{Value::Null}else{format!("warm-up exited {code}").into()}});
            if let Err(error) = self
                .schedules
                .record(schedule, outcome.as_object().cloned().unwrap_or_default())
            {
                eprintln!("Session schedule update failed: {error}");
            }
        }
    }
    fn reap_one(&self, id: &str) -> io::Result<bool> {
        let mut entries = self.entries.lock().map_err(|_| poisoned())?;
        let Some(entry) = entries.get_mut(id) else {
            return Ok(false);
        };
        let Some(code) = entry.owner.settled_exit()? else {
            return Ok(true);
        };
        entry.owner.finish(Duration::from_secs(5))?;
        let entry = entries
            .remove(id)
            .ok_or_else(|| io::Error::other("Settled session disappeared"))?;
        drop(entries);
        self.finish_metadata(id, &entry, code, now()?);
        Ok(false)
    }
    pub fn running(&self, id: &str) -> io::Result<bool> {
        let decision = self.decision(id)?;
        let _guard = decision.lock().map_err(|_| poisoned())?;
        if self.reap_one(id)? {
            return Ok(true);
        }
        match SessionLease::acquire(&SessionLease::path_for(&self.root, id)?) {
            Ok(lease) => drop(lease),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(true),
            Err(error) => return Err(error),
        }
        Ok(claims::state(&claims::read(&self.root, id), &self.host) != claims::State::Free)
    }
    pub fn owns(&self, id: &str) -> io::Result<bool> {
        Ok(self
            .entries
            .lock()
            .map_err(|_| poisoned())?
            .get(id)
            .is_some_and(|entry| entry.kind != Kind::Finite))
    }
    pub fn spawn(
        &self,
        id: &str,
        command: &mut Command,
        ready: &Path,
        kind: Kind,
        schedule: Option<String>,
        budget: Duration,
    ) -> Result<bool, StartupFailed> {
        let failure = |error: io::Error| StartupFailed {
            kind: if error.kind() == io::ErrorKind::WouldBlock {
                "busy"
            } else {
                "failed"
            }
            .into(),
            message: error.to_string(),
        };
        let decision = self.decision(id).map_err(failure)?;
        let _guard = decision.lock().map_err(|_| failure(poisoned()))?;
        if *self.closed.lock().map_err(|_| failure(poisoned()))? {
            return Err(StartupFailed {
                kind: "failed".into(),
                message: "Session registry is shutting down".into(),
            });
        }
        if self.reap_one(id).map_err(failure)? {
            return Ok(false);
        }
        let mut session = Session::admit(&self.root, id, &self.host).map_err(failure)?;
        session.spawn(command, kind.name()).map_err(failure)?;
        let result = session.await_ready(ready, budget);
        if let Err(error) = result {
            if session.tree_record().is_some() {
                let _closed = self.closed.lock().map_err(|_| failure(poisoned()))?;
                self.entries
                    .lock()
                    .map_err(|_| failure(poisoned()))?
                    .insert(
                        id.into(),
                        Entry {
                            owner: Owner::Helper(session),
                            kind,
                            schedule,
                            details: None,
                        },
                    );
            }
            return Err(error);
        }
        // Shutdown may begin while the SDK opens. Never register an untracked
        // child after the sweep took its snapshot.
        let closed = self.closed.lock().map_err(|_| failure(poisoned()))?;
        if *closed {
            let cleanup = session.finish(Duration::from_secs(5));
            if let Err(error) = cleanup {
                self.entries
                    .lock()
                    .map_err(|_| failure(poisoned()))?
                    .insert(
                        id.into(),
                        Entry {
                            owner: Owner::Helper(session),
                            kind,
                            schedule,
                            details: None,
                        },
                    );
                return Err(failure(error));
            }
            return Err(StartupFailed {
                kind: "failed".into(),
                message: "Session registry is shutting down".into(),
            });
        }
        self.entries
            .lock()
            .map_err(|_| failure(poisoned()))?
            .insert(
                id.into(),
                Entry {
                    owner: Owner::Helper(session),
                    kind,
                    schedule,
                    details: None,
                },
            );
        Ok(true)
    }
    pub fn retain_browser(&self, id: &str, adapter: Adapter) -> io::Result<()> {
        let decision = self.decision(id)?;
        let _guard = decision.lock().map_err(|_| poisoned())?;
        self.retain_browser_locked(id, adapter, Kind::Launch, None)
    }
    fn retain_browser_locked(
        &self,
        id: &str,
        mut adapter: Adapter,
        kind: Kind,
        details: Option<Value>,
    ) -> io::Result<()> {
        if !adapter.belongs_to(&self.root, id, &self.host) {
            adapter.finish(Duration::from_secs(5))?;
            return Err(io::Error::other(
                "Browser adapter belongs to another session",
            ));
        }
        let closed = self.closed.lock().map_err(|_| poisoned())?;
        let mut entries = self.entries.lock().map_err(|_| poisoned())?;
        if entries.contains_key(id) {
            adapter.finish(Duration::from_secs(5))?;
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Registry already owns this profile",
            ));
        }
        let shutting_down = *closed;
        entries.insert(
            id.into(),
            Entry {
                owner: Owner::Browser(adapter),
                kind,
                schedule: None,
                details,
            },
        );
        if shutting_down {
            let entry = entries
                .get_mut(id)
                .ok_or_else(|| io::Error::other("Browser owner missing"))?;
            entry.owner.stop(Duration::from_secs(5))?;
            entries.remove(id);
            return Err(io::Error::other("Session registry is shutting down"));
        }
        Ok(())
    }

    /// The SDK closure receives the current profile with its already-held lease.
    pub fn open_browser(
        &self,
        id: &str,
        requested_name: &str,
        activate: impl FnOnce(Session, Profile) -> Result<Adapter, StartupFailed>,
    ) -> Result<bool, StartupFailed> {
        self.open_browser_kind(id, requested_name, Kind::Launch, |session, profile| {
            activate(session, profile).map(|adapter| (adapter, None))
        })
    }
    pub fn open_browser_kind(
        &self,
        id: &str,
        requested_name: &str,
        kind: Kind,
        activate: impl FnOnce(Session, Profile) -> Result<(Adapter, Option<Value>), StartupFailed>,
    ) -> Result<bool, StartupFailed> {
        let failure = |error: io::Error| StartupFailed {
            kind: "failed".into(),
            message: error.to_string(),
        };
        let decision = self.decision(id).map_err(failure)?;
        let _guard = decision.lock().map_err(|_| failure(poisoned()))?;
        self.open_browser_locked(id, requested_name, kind, activate)
    }

    fn open_browser_locked(
        &self,
        id: &str,
        requested_name: &str,
        kind: Kind,
        activate: impl FnOnce(Session, Profile) -> Result<(Adapter, Option<Value>), StartupFailed>,
    ) -> Result<bool, StartupFailed> {
        let failure = |error: io::Error| StartupFailed {
            kind: "failed".into(),
            message: error.to_string(),
        };
        if *self.closed.lock().map_err(|_| failure(poisoned()))? {
            return Err(failure(io::Error::other(
                "Session registry is shutting down",
            )));
        }
        if self.reap_one(id).map_err(failure)? {
            return Ok(false);
        }
        let (session, profile) = admission::current(
            &self.root,
            id,
            requested_name,
            &self.host,
            self.password.as_deref(),
            now().map_err(failure)?,
        )?;
        let (adapter, details) = activate(session, profile)?;
        self.retain_browser_locked(id, adapter, kind, details)
            .map_err(failure)?;
        Ok(true)
    }

    pub fn attached(&self, id: &str) -> io::Result<Option<Value>> {
        let decision = self.decision(id)?;
        let _guard = decision.lock().map_err(|_| poisoned())?;
        if !self.reap_one(id)? {
            return Ok(None);
        }
        Ok(self
            .entries
            .lock()
            .map_err(|_| poisoned())?
            .get(id)
            .filter(|entry| entry.kind == Kind::Attach)
            .and_then(|entry| entry.details.clone()))
    }

    /// Admission and readiness are one decision; an older request must never
    /// query or settle a replacement attachment's owner.
    pub fn open_attachment(
        &self,
        id: &str,
        requested_name: &str,
        activate: impl FnOnce(Session, Profile) -> Result<Adapter, StartupFailed>,
        observe: impl FnOnce() -> io::Result<Value>,
    ) -> Result<Option<Value>, StartupFailed> {
        let failure = |error: io::Error| StartupFailed {
            kind: "failed".into(),
            message: error.to_string(),
        };
        let decision = self.decision(id).map_err(failure)?;
        let _guard = decision.lock().map_err(|_| failure(poisoned()))?;
        if !self.open_browser_locked(id, requested_name, Kind::Attach, |session, profile| {
            activate(session, profile).map(|adapter| (adapter, None))
        })? {
            return Ok(None);
        }
        self.attachment_details_locked(id, observe)
            .map(Some)
            .map_err(failure)
    }

    fn attachment_details_locked(
        &self,
        id: &str,
        observe: impl FnOnce() -> io::Result<Value>,
    ) -> io::Result<Value> {
        if !self.reap_one(id)? {
            return Err(io::Error::other(
                "Attached browser ended before DevTools was ready",
            ));
        }
        if !self
            .entries
            .lock()
            .map_err(|_| poisoned())?
            .get(id)
            .is_some_and(|entry| entry.kind == Kind::Attach)
        {
            return Err(io::Error::other("Profile is not an owned attachment"));
        }
        match observe() {
            Ok(details) => {
                if !self.reap_one(id)? {
                    return Err(io::Error::other(
                        "Attached browser ended before DevTools was ready",
                    ));
                }
                let mut entries = self.entries.lock().map_err(|_| poisoned())?;
                let entry = entries
                    .get_mut(id)
                    .ok_or_else(|| io::Error::other("Attachment disappeared"))?;
                entry.details = Some(details.clone());
                Ok(details)
            }
            Err(error) => {
                let mut entries = self.entries.lock().map_err(|_| poisoned())?;
                let entry = entries
                    .get_mut(id)
                    .ok_or_else(|| io::Error::other("Attachment disappeared"))?;
                // A failed settlement leaves the original owner for retry/shutdown.
                entry.owner.stop(Duration::from_secs(10))?;
                entries.remove(id);
                Err(error)
            }
        }
    }

    pub fn stop(&self, id: &str, budget: Duration) -> io::Result<bool> {
        let decision = self.decision(id)?;
        let _guard = decision.lock().map_err(|_| poisoned())?;
        let mut entries = self.entries.lock().map_err(|_| poisoned())?;
        let Some(entry) = entries.get_mut(id) else {
            return Ok(false);
        };
        let code = entry.owner.stop(budget)?.unwrap_or(-1);
        let entry = entries
            .remove(id)
            .ok_or_else(|| io::Error::other("Stopped session disappeared"))?;
        drop(entries);
        self.finish_metadata(id, &entry, code, now()?);
        Ok(true)
    }
    pub fn reap(&self) -> io::Result<()> {
        let ids = self
            .entries
            .lock()
            .map_err(|_| poisoned())?
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for id in ids {
            let decision = self.decision(&id)?;
            let _guard = decision.lock().map_err(|_| poisoned())?;
            self.reap_one(&id)?;
        }
        Ok(())
    }
    pub fn shutdown(&self, budget: Duration) -> Vec<(String, String)> {
        let mut failed = Vec::new();
        match self.closed.lock() {
            Ok(mut closed) => *closed = true,
            Err(_) => return vec![(String::new(), poisoned().to_string())],
        }
        // Include in-flight admissions, not just already-ready entries. Their
        // per-profile decision lock makes shutdown wait for readiness cleanup.
        let ids = match self.decisions.lock() {
            Ok(decisions) => decisions.keys().cloned().collect::<Vec<_>>(),
            Err(_) => return vec![(String::new(), poisoned().to_string())],
        };
        for id in ids {
            if let Err(error) = self.stop(&id, budget) {
                failed.push((id, error.to_string()));
            }
        }
        failed
    }
}
fn retained_entries() -> &'static std::sync::Mutex<Vec<Entry>> {
    static OWNERS: std::sync::OnceLock<std::sync::Mutex<Vec<Entry>>> = std::sync::OnceLock::new();
    OWNERS.get_or_init(|| std::sync::Mutex::new(Vec::new()))
}
impl Drop for Registry {
    fn drop(&mut self) {
        let failed = self.shutdown(Duration::from_secs(2));
        if !failed.is_empty() {
            eprintln!(
                "Session registry retains {} unsettled owned tree(s)",
                failed.len()
            );
            let entries = self
                .entries
                .get_mut()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            retained_entries()
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .extend(std::mem::take(entries).into_values());
        }
    }
}
pub fn retry_retained_sessions(budget: Duration) -> Vec<String> {
    let entries = std::mem::take(
        &mut *retained_entries()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    );
    let deadline = std::time::Instant::now()
        .checked_add(budget)
        .unwrap_or_else(std::time::Instant::now);
    let mut pending = Vec::new();
    let mut failures = Vec::new();
    for mut entry in entries {
        if let Err(error) = entry
            .owner
            .stop(deadline.saturating_duration_since(std::time::Instant::now()))
        {
            failures.push(error.to_string());
            pending.push(entry);
        }
    }
    retained_entries()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .extend(pending);
    failures
}
