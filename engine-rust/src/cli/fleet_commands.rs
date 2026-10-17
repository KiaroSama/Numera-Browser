use super::profile_commands::{Outcome, Profiles, summary};
use crate::{
    collision, identity,
    jobs::JobStore,
    storage::{
        repo::RepoError,
        schedules::{ScheduleStore, next_run_at},
    },
};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};

impl Profiles<'_> {
    pub fn health(&self, root: &std::path::Path, time: f64) -> Result<Outcome, RepoError> {
        let profiles = self.repo.list(time)?;
        let groups = collision::find(&profiles);
        let colliding = groups
            .iter()
            .flat_map(|g| g["members"].as_array().into_iter().flatten())
            .filter_map(|m| m["id"].as_str())
            .collect::<BTreeSet<_>>();
        let mut schedules = BTreeMap::<String, Value>::new();
        for schedule in ScheduleStore::new(root.into()).list()? {
            let id = schedule["profileId"]
                .as_str()
                .ok_or_else(|| {
                    RepoError::InvalidRecord("Stored schedule profile ID is not text".into())
                })?
                .to_owned();
            let interval = |value: &Value| {
                value.get("everyHours").map_or(Ok(1e9), |v| {
                    v.as_f64().ok_or_else(|| {
                        RepoError::InvalidRecord("Stored schedule interval is not numeric".into())
                    })
                })
            };
            if !schedules.contains_key(&id) || interval(&schedule)? < interval(&schedules[&id])? {
                schedules.insert(id, schedule);
            }
        }
        let jobs = JobStore::new(root.into()).listing(time)?;
        let running = jobs
            .iter()
            .filter(|j| j["running"] == true)
            .filter_map(|j| j["profile"].as_str())
            .collect::<BTreeSet<_>>();
        let mut counts = Map::from_iter(
            [
                "total",
                "running",
                "coherent",
                "withProxy",
                "scheduled",
                "colliding",
                "needsAttention",
            ]
            .map(|k| (k.into(), 0.into())),
        );
        let mut rows = Vec::new();
        for profile in &profiles {
            let issues = identity::validate(&profile.fingerprint).map_err(std::io::Error::other)?;
            let collision = colliding.contains(profile.id.as_str());
            let active = running.contains(profile.id.as_str());
            let schedule = schedules.get(&profile.id);
            let attention = !issues.is_empty() || collision;
            let mut row = summary(profile)
                .as_object()
                .cloned()
                .ok_or_else(|| RepoError::Message("Profile summary shape missing".into()))?;
            row.extend(json!({"coherent":issues.is_empty(),"issues":issues,"collides":collision,"hasProxy":profile.proxy.is_some(),"status":if active{"running"}else{"stopped"},"trust":null,
                "schedule":schedule.map(|s|Ok::<Value,RepoError>(json!({"id":s["id"],"everyHours":s["everyHours"],"preset":s["preset"],"enabled":s.get("enabled").cloned().unwrap_or(true.into()),"nextRun":next_run_at(s)?}))).transpose()?,"needsAttention":attention}).as_object().cloned().ok_or_else(||RepoError::Message("Health row shape missing".into()))?);
            for (key, on) in [
                ("total", true),
                ("running", active),
                ("coherent", issues.is_empty()),
                ("withProxy", profile.proxy.is_some()),
                ("scheduled", schedule.is_some()),
                ("colliding", collision),
                ("needsAttention", attention),
            ] {
                let n = counts[key]
                    .as_i64()
                    .ok_or_else(|| RepoError::Message("Health count missing".into()))?;
                counts[key] = (n + i64::from(on)).into();
            }
            rows.push(Value::Object(row));
        }
        Ok(Outcome::success(json!({"summary":counts,"profiles":rows})))
    }
    pub fn delete(
        &self,
        references: &[String],
        yes: bool,
        time: f64,
        confirm: impl FnOnce(usize) -> Result<bool, RepoError>,
    ) -> Result<Outcome, RepoError> {
        self.delete_observed(references, yes, time, confirm, |_, _| Ok(()))
    }
    pub fn delete_with_output(
        &self,
        references: &[String],
        yes: bool,
        time: f64,
        confirm: impl FnOnce(usize) -> Result<bool, RepoError>,
        output: &mut super::output::Output<impl std::io::Write, impl std::io::Write>,
    ) -> Result<Outcome, RepoError> {
        self.delete_observed(references, yes, time, confirm, |text, tone| {
            output.say(text, tone).map_err(RepoError::from)
        })
    }
    fn delete_observed(
        &self,
        references: &[String],
        yes: bool,
        time: f64,
        confirm: impl FnOnce(usize) -> Result<bool, RepoError>,
        mut event: impl FnMut(&str, Option<super::output::Tone>) -> Result<(), RepoError>,
    ) -> Result<Outcome, RepoError> {
        let mut targets = Vec::new();
        let mut seen = BTreeSet::new();
        let mut missing = Vec::new();
        for reference in references {
            match self.repo.resolve(reference, time) {
                Ok(Some(profile)) => {
                    if seen.insert(profile.id.clone()) {
                        targets.push((reference.clone(), profile));
                    }
                }
                Ok(None) => missing.push(reference.clone()),
                Err(error) => {
                    event(&error.to_string(), Some(super::output::Tone::Red))?;
                    return Ok(Outcome::failed(error.to_string()));
                }
            }
        }
        if !missing.is_empty() {
            event(
                &format!("No profile matching '{}'.", missing.join(", ")),
                Some(super::output::Tone::Red),
            )?;
            return Ok(Outcome::failed(format!(
                "no profile matching '{}'",
                missing.join(", ")
            )));
        }
        if targets.is_empty() {
            event(
                &format!("No profile matching '{}'.", references.join(", ")),
                Some(super::output::Tone::Red),
            )?;
            return Ok(Outcome::failed(format!(
                "no profile matching '{}'",
                references.join(", ")
            )));
        }
        if !yes && !confirm(targets.len())? {
            event("Aborted.", None)?;
            return Ok(Outcome::success(json!({"ok":false,"aborted":true})));
        }
        let mut removed = Vec::new();
        let mut failed = Vec::new();
        for (reference, profile) in targets {
            match self.repo.delete(&reference, self.host, time) {
                Ok(true) => {
                    event(
                        &format!(
                            "Deleted '{}'.",
                            crate::conversion::value_text(&profile.name)
                        ),
                        Some(super::output::Tone::Green),
                    )?;
                    removed.push(json!({"id":profile.id,"name":profile.name}));
                }
                Ok(false) => {
                    event(
                        &format!(
                            "'{}' was already gone.",
                            crate::conversion::value_text(&profile.name)
                        ),
                        Some(super::output::Tone::Dim),
                    )?;
                    missing.push(reference);
                }
                Err(error) => {
                    event(
                        &format!(
                            "Could not delete '{}': {error}",
                            crate::conversion::value_text(&profile.name)
                        ),
                        Some(super::output::Tone::Red),
                    )?;
                    failed.push(json!({"id":profile.id,"name":profile.name,"error":error.to_string().lines().next().unwrap_or("").chars().take(200).collect::<String>()}));
                }
            }
        }
        if removed.len() == 1 && failed.is_empty() && missing.is_empty() {
            let mut result = removed
                .pop()
                .and_then(|v| v.as_object().cloned())
                .ok_or_else(|| RepoError::Message("Deleted profile result missing".into()))?;
            result.insert("ok".into(), true.into());
            return Ok(Outcome::success(Value::Object(result)));
        }
        let success = failed.is_empty() && missing.is_empty();
        Ok(Outcome {
            exit: i32::from(!success),
            payload: json!({"ok":success,"removed":removed,"missing":missing,"failed":failed}),
        })
    }
}
