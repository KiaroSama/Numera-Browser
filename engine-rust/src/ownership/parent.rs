//! Pollable parent ownership for the native server's existing wait loop.
//! A birth token prevents a reused PID from keeping an orphaned server alive.
use std::io;

#[derive(Clone, Copy, Debug)]
pub struct Monitor {
    pid: u32,
    token: Option<u64>,
}
impl Monitor {
    pub fn from_text(value: Option<&str>) -> io::Result<Option<Self>> {
        let Some(pid) = value
            .and_then(|value| value.parse::<u32>().ok())
            .filter(|pid| *pid > 0)
        else {
            return Ok(None);
        };
        // A configured parent that is already gone still closes the lifetime.
        // Treating that observation as "no parent configured" would orphan it.
        let token = super::identity::started_at(pid)?;
        Ok(Some(Self { pid, token }))
    }
    pub fn from_environment() -> io::Result<Option<Self>> {
        Self::from_text(std::env::var("NUMERA_PARENT_PID").ok().as_deref())
    }
    pub fn alive(&self) -> io::Result<bool> {
        match self.token {
            Some(token) => super::identity::alive(self.pid, Some(token)),
            None => Ok(false),
        }
    }
}
