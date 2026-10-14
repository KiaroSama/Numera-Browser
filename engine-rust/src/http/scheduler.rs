use super::{cookie_routes::now, profile_routes::Profiles, warmup_routes::start_owned};
use crate::{
    browser::warmup_policy,
    conversion::{truth, value_text},
    storage::{
        repo::RepoError,
        schedules::{ScheduleStore, due_schedules},
    },
};
use serde_json::{Map, json};

/// One tick has no clock thread or transport: the server lifetime drives it.
/// Each occurrence is claimed before activation and records only its own owner.
pub(crate) fn release_failed(schedules: &ScheduleStore, id: &str, claimed: bool, unsettled: bool) {
    if claimed
        && !unsettled
        && let Err(error) = schedules.release(id)
    {
        eprintln!("Schedule occurrence release failed: {error}");
    }
}

pub struct Scheduler {
    state: Profiles,
}
impl Scheduler {
    pub fn new(
        root: std::path::PathBuf,
        password: Option<String>,
        host: String,
        sessions: std::sync::Arc<crate::ownership::registry::Registry>,
        browser: crate::browser::runtime::Runtime,
    ) -> std::io::Result<Self> {
        if sessions.root() != root || sessions.host() != host {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Scheduler owner does not belong to its data root and host",
            ));
        }
        Ok(Self {
            state: Profiles {
                dashboard: crate::storage::dashboard::DashboardStore::new(
                    root.clone(),
                    password.clone(),
                ),
                engine: crate::storage::repo::ProfileRepo::new(root.clone(), password),
                root,
                host,
                sessions,
                installed: Default::default(),
                browser: Some(browser),
            },
        })
    }
    pub fn tick(&self) -> Result<(), RepoError> {
        self.state.sessions.reap()?;
        tick(&self.state)
    }
}
fn tick(state: &Profiles) -> Result<(), RepoError> {
    let schedules = ScheduleStore::new(state.root.clone());
    let rows = due_schedules(&schedules.list()?, now()?)?;
    for row in rows {
        let mut claimed = false;
        let mut unsettled = false;
        let id = row["id"]
            .as_str()
            .ok_or_else(|| RepoError::Message("Stored schedule ID is not text".into()))?;
        let pid = row["profileId"].as_str().unwrap_or("");
        let result = (|| -> Result<(), RepoError> {
            if pid.is_empty() {
                return Err(RepoError::Message(
                    "Stored schedule profile ID is not text".into(),
                ));
            }
            let Some(record) = state.dashboard.get(pid)?.filter(truth) else {
                return Ok(());
            };
            if state.sessions.running(pid)? {
                return Ok(());
            }
            if !schedules.claim_due(id, &format!("server:{}", std::process::id()), now()?)? {
                return Ok(());
            }
            claimed = true;
            state.engine.materialize(pid, now()?)?;
            let minutes = warmup_policy::validate(
                row.get("minutes").unwrap_or(&json!(5)),
                row.get("preset"),
                None,
            )?;
            let preset = row
                .get("preset")
                .filter(|value| truth(value))
                .map(value_text)
                .unwrap_or("general".into())
                .to_lowercase();
            let opened = start_owned(
                state,
                pid,
                &value_text(&record["name"]),
                minutes,
                preset,
                Some(id.into()),
            )?
            .map_err(|error| {
                unsettled = error.kind == "unsettled";
                RepoError::Message(error.to_string())
            })?;
            if !opened {
                return Err(RepoError::Message(
                    "another session took the profile first".into(),
                ));
            }
            Ok(())
        })();
        if let Err(error) = result {
            release_failed(&schedules, id, claimed, unsettled);
            let safe = error
                .to_string()
                .lines()
                .next()
                .unwrap_or("")
                .chars()
                .take(200)
                .collect::<String>();
            if let Err(record) = schedules.record(
                id,
                Map::from_iter([
                    ("lastOutcome".into(), "failed".into()),
                    ("lastError".into(), safe.into()),
                    ("lastFailedAt".into(), now()?.into()),
                ]),
            ) {
                eprintln!("Schedule failure bookkeeping failed: {record}");
            }
        }
    }
    Ok(())
}
