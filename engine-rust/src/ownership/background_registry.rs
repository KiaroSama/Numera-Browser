use super::{
    Adapter, Background, Entry, Kind, Owner, Profile, Registry, Session, StartupFailed, admission,
    now, poisoned,
};
use std::{
    io,
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

impl Registry {
    pub fn start_background_browser(
        &self,
        id: &str,
        requested_name: &str,
        kind: Kind,
        schedule: Option<String>,
        activate: impl FnOnce(Session, Profile) -> Result<Adapter, StartupFailed>,
        operation: impl FnOnce(&mut Adapter, Arc<AtomicBool>) -> io::Result<()> + Send + 'static,
    ) -> Result<bool, StartupFailed> {
        self.start_background_checked(
            id,
            requested_name,
            kind,
            schedule,
            activate,
            operation,
            || Ok(()),
        )
    }
    pub fn start_background_checked(
        &self,
        id: &str,
        requested_name: &str,
        kind: Kind,
        schedule: Option<String>,
        activate: impl FnOnce(Session, Profile) -> Result<Adapter, StartupFailed>,
        operation: impl FnOnce(&mut Adapter, Arc<AtomicBool>) -> io::Result<()> + Send + 'static,
        commit: impl FnOnce() -> io::Result<()>,
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
        let result =
            self.start_background_locked(id, requested_name, kind, schedule, activate, operation);
        let result = match result {
            Ok(true) => commit().map(|_| true).map_err(failure),
            other => other,
        };
        self.background_result_locked(id, result, |owner| owner.stop(Duration::from_secs(10)))
    }
    fn background_result_locked(
        &self,
        id: &str,
        result: Result<bool, StartupFailed>,
        stop: impl FnOnce(&mut Owner) -> io::Result<Option<i32>>,
    ) -> Result<bool, StartupFailed> {
        if let Err(error) = result {
            let mut entries = self.entries.lock().map_err(|_| StartupFailed {
                kind: "unsettled".into(),
                message: poisoned().to_string(),
            })?;
            if let Some(entry) = entries.get_mut(id) {
                if let Err(cleanup) = stop(&mut entry.owner) {
                    return Err(StartupFailed {
                        kind: "unsettled".into(),
                        message: format!("{error}; cleanup failed: {cleanup}"),
                    });
                }
                entries.remove(id);
            }
            return Err(error);
        }
        result
    }
    fn start_background_locked(
        &self,
        id: &str,
        requested_name: &str,
        kind: Kind,
        schedule: Option<String>,
        activate: impl FnOnce(Session, Profile) -> Result<Adapter, StartupFailed>,
        operation: impl FnOnce(&mut Adapter, Arc<AtomicBool>) -> io::Result<()> + Send + 'static,
    ) -> Result<bool, StartupFailed> {
        let failure = |error: io::Error| StartupFailed {
            kind: "failed".into(),
            message: error.to_string(),
        };
        if !matches!(kind, Kind::ManualWarmup | Kind::ScheduledWarmup) {
            return Err(failure(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Background browser kind is invalid",
            )));
        }
        let (session, profile) = admission::current(
            &self.root,
            id,
            requested_name,
            &self.host,
            self.password.as_deref(),
            now().map_err(failure)?,
        )?;
        let mut adapter = activate(session, profile)?;
        if !adapter.belongs_to(&self.root, id, &self.host) {
            adapter.finish(Duration::from_secs(5)).map_err(failure)?;
            return Err(failure(io::Error::other(
                "Browser adapter belongs to another session",
            )));
        }
        let closed = self.closed.lock().map_err(|_| failure(poisoned()))?;
        let mut entries = self.entries.lock().map_err(|_| failure(poisoned()))?;
        if entries.contains_key(id) {
            adapter.finish(Duration::from_secs(5)).map_err(failure)?;
            return Err(failure(io::Error::other(
                "Registry already owns this profile",
            )));
        }
        let mut pending = Some(adapter);
        let owner = match Background::start(&mut pending, operation) {
            Ok(owner) => owner,
            Err(error) => {
                if let Some(mut adapter) = pending {
                    if let Err(cleanup) = adapter.close(Duration::from_secs(5)) {
                        entries.insert(
                            id.into(),
                            Entry {
                                owner: Owner::Browser(adapter),
                                kind,
                                schedule,
                                details: None,
                            },
                        );
                        return Err(failure(io::Error::other(format!(
                            "Background start failed: {error}; cleanup failed: {cleanup}"
                        ))));
                    }
                }
                return Err(failure(error));
            }
        };
        entries.insert(
            id.into(),
            Entry {
                owner: Owner::Background(owner),
                kind,
                schedule,
                details: None,
            },
        );
        if *closed {
            let entry = entries
                .get_mut(id)
                .ok_or_else(|| failure(io::Error::other("Background owner disappeared")))?;
            entry.owner.stop(Duration::from_secs(5)).map_err(failure)?;
            entries.remove(id);
            return Err(failure(io::Error::other(
                "Session registry is shutting down",
            )));
        }
        Ok(true)
    }
}
