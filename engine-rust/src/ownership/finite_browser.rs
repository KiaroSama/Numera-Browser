use super::{Adapter, Entry, Kind, Owner, Profile, Registry, Session, admission, now, poisoned};
use crate::ownership::ready::StartupFailed;
use std::{io, time::Duration};

impl Registry {
    /// A finite operation holds its admission decision through settlement, so
    /// shutdown joins it before returning and cannot miss an in-flight SDK tree.
    pub fn with_finite_browser<T>(
        &self,
        id: &str,
        requested_name: &str,
        activate: impl FnOnce(Session, Profile) -> Result<Adapter, StartupFailed>,
        operation: impl FnOnce(&mut Adapter) -> io::Result<T>,
    ) -> io::Result<T> {
        let decision = self.decision(id)?;
        let _guard = decision.lock().map_err(|_| poisoned())?;
        if *self.closed.lock().map_err(|_| poisoned())? {
            return Err(io::Error::other("Session registry is shutting down"));
        }
        if self.reap_one(id)? {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "Profile session is in use",
            ));
        }
        let (session, profile) = admission::current(
            &self.root,
            id,
            requested_name,
            &self.host,
            self.password.as_deref(),
            now()?,
        )
        .map_err(io::Error::other)?;
        let mut adapter = activate(session, profile).map_err(io::Error::other)?;
        let result = if *self.closed.lock().map_err(|_| poisoned())? {
            Err(io::Error::other("Session registry is shutting down"))
        } else {
            operation(&mut adapter)
        };
        let cleanup = adapter.close(Duration::from_secs(10));
        match cleanup {
            Err(error) => {
                self.entries.lock().map_err(|_| poisoned())?.insert(
                    id.into(),
                    Entry {
                        owner: Owner::Browser(adapter),
                        kind: Kind::Finite,
                        schedule: None,
                        details: None,
                    },
                );
                Err(error)
            }
            Ok(Some(0)) => result,
            Ok(_) => Err(io::Error::other(
                "Browser cookie session did not close successfully",
            )),
        }
    }
}
