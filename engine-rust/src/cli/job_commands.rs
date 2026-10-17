use super::profile_commands::{Outcome, Profiles};
use crate::{
    jobs::JobStore,
    ownership::claims,
    storage::{locks::SessionLease, repo::RepoError},
};
use serde_json::{Map, Value, json};
use std::{io, path::Path};

pub fn run(
    root: &Path,
    host: &str,
    args: &Map<String, Value>,
    now: f64,
) -> Result<Outcome, RepoError> {
    let jobs = JobStore::new(root.into());
    let action = args
        .get("jobs_command")
        .and_then(Value::as_str)
        .unwrap_or("");
    match action {
        "list" => Ok(Outcome::success(json!({"jobs":jobs.listing(now)?}))),
        "clear" => Ok(Outcome::success(json!({"cleared":jobs.prune(now)?}))),
        "show" | "stop" => {
            let id = args
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| RepoError::Message("Job ID missing".into()))?;
            let Some(mut record) = jobs.get(id, now)? else {
                return Ok(Outcome::failed(format!("no job with id '{id}'")));
            };
            if action == "stop" {
                record = jobs
                    .stop(id, host, now)?
                    .ok_or_else(|| RepoError::Message("Job disappeared during stop".into()))?;
            } else {
                let lines = args.get("lines").and_then(Value::as_i64).ok_or_else(|| {
                    RepoError::Message(
                        "Job line count is outside the native supported range".into(),
                    )
                })?;
                let tail = jobs.tail(&record, lines)?;
                record
                    .as_object_mut()
                    .ok_or_else(|| RepoError::Message("Job result is not an object".into()))?
                    .insert("tail".into(), json!(tail));
            }
            Ok(Outcome::success(record))
        }
        _ => Err(RepoError::Message("Unknown jobs command".into())),
    }
}
impl Profiles<'_> {
    pub fn stop(&self, root: &Path, reference: &str, now: f64) -> Result<Outcome, RepoError> {
        self.stop_observed(root, reference, now, |_, _| Ok(()))
    }
    pub fn stop_with_output(
        &self,
        root: &Path,
        reference: &str,
        now: f64,
        output: &mut super::output::Output<impl std::io::Write, impl std::io::Write>,
    ) -> Result<Outcome, RepoError> {
        self.stop_observed(root, reference, now, |text, tone| {
            output.say(text, Some(tone)).map_err(RepoError::from)
        })
    }
    fn stop_observed(
        &self,
        root: &Path,
        reference: &str,
        now: f64,
        mut event: impl FnMut(&str, super::output::Tone) -> Result<(), RepoError>,
    ) -> Result<Outcome, RepoError> {
        let Some(profile) = self.repo.resolve(reference, now)? else {
            event(
                &format!("No profile matching '{reference}'."),
                super::output::Tone::Red,
            )?;
            return Ok(Outcome::failed(format!(
                "no profile matching '{reference}'"
            )));
        };
        let stopped = JobStore::new(root.into()).stop_profile(&profile.id, self.host, now)?;
        if !stopped.is_empty() {
            claims::clear(root, &profile.id, self.host)?;
            let ids = stopped
                .iter()
                .map(|job| job.get("id").cloned().unwrap_or(Value::Null))
                .collect::<Vec<_>>();
            event(
                &format!(
                    "Stopped {} session(s) for '{}'.",
                    ids.len(),
                    crate::conversion::value_text(&profile.name)
                ),
                super::output::Tone::Green,
            )?;
            return Ok(Outcome::success(
                json!({"ok":true,"id":profile.id,"name":profile.name,"stopped":ids}),
            ));
        }
        let claim = claims::read(root, &profile.id);
        let lease = SessionLease::acquire(&SessionLease::path_for(root, &profile.id)?);
        let busy = match &lease {
            Ok(_) => {
                claims::state(&claims::read(root, &profile.id), self.host) != claims::State::Free
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => true,
            Err(error) => return Err(RepoError::Message(error.to_string())),
        };
        if busy {
            event(
                &format!(
                    "'{}' is open in another process, which this command cannot stop: {}.",
                    crate::conversion::value_text(&profile.name),
                    claims::describe(&claim, self.host)
                ),
                super::output::Tone::Yellow,
            )?;
            return Ok(Outcome {
                exit: 1,
                payload: json!({"ok":false,"id":profile.id,"name":profile.name,"stopped":[],"error":"the session belongs to another process; stop it where it was started"}),
            });
        }
        let cleared = claims::clear(root, &profile.id, self.host)?;
        if !matches!(claim, claims::Claim::Absent) && cleared {
            event(
                &format!(
                    "Cleared a stale session claim for '{}'.",
                    crate::conversion::value_text(&profile.name)
                ),
                super::output::Tone::Dim,
            )?;
        }
        event(
            &format!(
                "'{}' has nothing running.",
                crate::conversion::value_text(&profile.name)
            ),
            super::output::Tone::Dim,
        )?;
        Ok(Outcome::success(
            json!({"ok":true,"id":profile.id,"name":profile.name,"stopped":[]}),
        ))
    }
}
