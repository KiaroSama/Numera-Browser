//! A child whose birth identity could not be published is still an owned handle.
//! Only direct-child exit plus a complete kernel-tree proof resolves its intent.
use super::unit_report::SpawnIntent;
use std::{
    io,
    process::Child,
    sync::{Mutex, OnceLock},
    thread,
    time::{Duration, Instant},
};

pub(super) enum KernelTree {
    #[cfg(unix)]
    Group(i32),
    #[cfg(unix)]
    Unobservable,
    #[cfg(windows)]
    Job(super::windows::Job),
}
impl KernelTree {
    #[cfg(unix)]
    #[allow(unsafe_code)]
    pub(super) fn captured_group(child: &Child, expected: i32) -> Self {
        // SAFETY: passive scalar query while the original Child remains
        // retained and unreaped. A custom pre_exec may have changed groups;
        // the requested process_group setting alone is not a kernel proof.
        if unsafe { libc::getpgid(expected) } == expected && child.id() == expected as u32 {
            Self::Group(expected)
        } else {
            Self::Unobservable
        }
    }
}
pub(super) struct Unpublished {
    child: Child,
    tree: KernelTree,
    intent: Option<SpawnIntent>,
}
impl Unpublished {
    pub(super) fn new(child: Child, tree: KernelTree) -> Self {
        Self {
            child,
            tree,
            intent: None,
        }
    }
    pub(super) fn failed(mut self, intent: Option<SpawnIntent>) -> bool {
        self.intent = intent;
        if self.settle(Duration::from_secs(5)).is_err() {
            retained()
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(self);
            return false;
        }
        true
    }
    fn settle(&mut self, budget: Duration) -> io::Result<()> {
        let end = Instant::now()
            .checked_add(budget)
            .ok_or_else(|| io::Error::other("Invalid unpublished child cleanup budget"))?;
        within(end)?;
        #[cfg(windows)]
        {
            // This retained Job handle is authority even when process birth
            // lookup failed. An unassigned child remains suspended and is
            // independently killed/waited below; an assigned tree is ended here.
            let KernelTree::Job(job) = &self.tree;
            let _ = job.terminate();
        }
        within(end)?;
        if self.child.try_wait()?.is_none() {
            // A failed signal is not proof: keep the original Child until its
            // actual exit is observed within this same deadline.
            within(end)?;
            let _ = self.child.kill();
            loop {
                within(end)?;
                if self.child.try_wait()?.is_some() {
                    break;
                }
                thread::sleep(
                    end.saturating_duration_since(Instant::now())
                        .min(Duration::from_millis(10)),
                );
            }
        }
        within(end)?;
        let empty = match &self.tree {
            #[cfg(unix)]
            KernelTree::Group(group) => super::tree::record_empty(
                &serde_json::json!({"tree":{"kind":"pgid","group":group}}),
            )?,
            #[cfg(unix)]
            KernelTree::Unobservable => {
                return Err(io::Error::other(
                    "Unpublished child tree identity remains invalid",
                ));
            }
            #[cfg(windows)]
            KernelTree::Job(job) => job.empty()?,
        };
        if !empty {
            // Without birth identity a PGID is observation, not signal
            // authority. In particular, never kill a surviving Unix leaf here.
            return Err(io::Error::other("Unpublished child tree remains nonempty"));
        }
        if let Some(intent) = &self.intent {
            within(end)?;
            intent.cancel_before(end)?;
        }
        self.intent = None;
        Ok(())
    }
}
fn within(deadline: Instant) -> io::Result<()> {
    if Instant::now() >= deadline {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "Unpublished child cleanup deadline expired",
        ));
    }
    Ok(())
}
fn retained() -> &'static Mutex<Vec<Unpublished>> {
    static OWNERS: OnceLock<Mutex<Vec<Unpublished>>> = OnceLock::new();
    OWNERS.get_or_init(|| Mutex::new(Vec::new()))
}
pub(super) fn retry_retained(budget: Duration) -> Vec<String> {
    let mut owners = std::mem::take(
        &mut *retained()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    );
    let end = Instant::now()
        .checked_add(budget)
        .unwrap_or_else(Instant::now);
    let mut failures = Vec::new();
    owners.retain_mut(
        |owner| match owner.settle(end.saturating_duration_since(Instant::now())) {
            Ok(()) => false,
            Err(error) => {
                failures.push(error.to_string());
                true
            }
        },
    );
    retained()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .extend(owners);
    failures
}
