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
}
impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Launch => "launch",
            Self::Attach => "attach",
            Self::ManualWarmup => "manual_warmup",
            Self::ScheduledWarmup => "scheduled_warmup",
        }
    }
}
enum Owner {
    Helper(Session),
    Browser(Adapter),
}
impl Owner {
    fn settled_exit(&mut self) -> io::Result<Option<i32>> {
        match self {
            Self::Helper(session) => session.settled_exit(),
            Self::Browser(adapter) => adapter.settled_exit(),
        }
    }
    fn finish(&mut self, budget: Duration) -> io::Result<Option<i32>> {
        match self {
            Self::Helper(session) => session.finish_with_status(budget),
            Self::Browser(adapter) => adapter.finish_with_status(budget),
        }
    }
    fn stop(&mut self, budget: Duration) -> io::Result<Option<i32>> {
        match self {
            Self::Helper(session) => session.finish_with_status(budget),
            Self::Browser(adapter) => adapter.close(budget),
        }
    }
}
struct Entry {
    owner: Owner,
    kind: Kind,
    schedule: Option<String>,
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
            .contains_key(id))
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
                },
            );
        Ok(true)
    }
    pub fn retain_browser(&self, id: &str, adapter: Adapter) -> io::Result<()> {
        let decision = self.decision(id)?;
        let _guard = decision.lock().map_err(|_| poisoned())?;
        self.retain_browser_locked(id, adapter)
    }
    fn retain_browser_locked(&self, id: &str, mut adapter: Adapter) -> io::Result<()> {
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
                kind: Kind::Launch,
                schedule: None,
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
        let failure = |error: io::Error| StartupFailed {
            kind: "failed".into(),
            message: error.to_string(),
        };
        let decision = self.decision(id).map_err(failure)?;
        let _guard = decision.lock().map_err(|_| failure(poisoned()))?;
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
        let adapter = activate(session, profile)?;
        self.retain_browser_locked(id, adapter).map_err(failure)?;
        Ok(true)
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
impl Drop for Registry {
    fn drop(&mut self) {
        let failed = self.shutdown(Duration::from_secs(2));
        if !failed.is_empty() {
            eprintln!(
                "Session registry could not settle {} owned tree(s)",
                failed.len()
            );
        }
    }
}
