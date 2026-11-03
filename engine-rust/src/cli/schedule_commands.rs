use super::{
    profile_commands::Outcome,
    warmup_commands::{Options, Warmup},
};
use crate::storage::{
    repo::RepoError,
    schedules::{ScheduleStore, due_schedules},
};
use serde_json::{Value, json};

impl Warmup<'_> {
    pub fn run_due(
        &self,
        now: f64,
        clock: impl FnMut() -> Result<f64, RepoError>,
        run: impl FnMut(Options<'_>) -> Result<(), RepoError>,
    ) -> Result<Outcome, RepoError> {
        self.run_due_observed(now, clock, run, |_, _| Ok(()))
    }
    pub fn run_due_with_output(
        &self,
        now: f64,
        clock: impl FnMut() -> Result<f64, RepoError>,
        run: impl FnMut(Options<'_>) -> Result<(), RepoError>,
        output: &mut super::output::Output<impl std::io::Write, impl std::io::Write>,
    ) -> Result<Outcome, RepoError> {
        self.run_due_observed(now, clock, run, |text, tone| {
            output.say(text, Some(tone)).map_err(RepoError::from)
        })
    }
    fn run_due_observed(
        &self,
        now: f64,
        mut clock: impl FnMut() -> Result<f64, RepoError>,
        mut run: impl FnMut(Options<'_>) -> Result<(), RepoError>,
        mut event: impl FnMut(&str, super::output::Tone) -> Result<(), RepoError>,
    ) -> Result<Outcome, RepoError> {
        let schedules = ScheduleStore::new(self.repo.root.clone());
        let due = due_schedules(&schedules.list()?, now)?;
        if due.is_empty() {
            event("Nothing due.", super::output::Tone::Dim)?;
            return Ok(Outcome::success(json!({"due":0,"ran":0,"skipped":[]})));
        }
        let profiles = self.repo.list(now)?;
        let owner = format!("cli:{}", std::process::id());
        let mut ran = Vec::new();
        let mut skipped = Vec::new();
        for schedule in &due {
            let id = schedule["id"]
                .as_str()
                .ok_or_else(|| RepoError::Message("Stored schedule ID is not text".into()))?;
            let Some(profile) = profiles
                .iter()
                .find(|profile| schedule["profileId"] == profile.id)
            else {
                event(
                    &format!(
                        "  skip {id} — profile {} not in this store",
                        crate::conversion::value_text(&schedule["profileId"])
                    ),
                    super::output::Tone::Yellow,
                )?;
                skipped.push(id.to_owned());
                continue;
            };
            if !schedules.claim_due(id, &owner, clock()?)? {
                event(
                    &format!("  skip {id} - another worker has this occurrence"),
                    super::output::Tone::Dim,
                )?;
                skipped.push(id.to_owned());
                continue;
            }
            let result = (|| {
                event(
                    &format!(
                        "  warming '{}' ({}, {}min)...",
                        crate::conversion::value_text(&profile.name),
                        crate::conversion::value_text(&schedule["preset"]),
                        crate::conversion::value_text(&schedule["minutes"])
                    ),
                    super::output::Tone::Cyan,
                )?;
                run(Options {
                    reference: &profile.id,
                    minutes: schedule.get("minutes").unwrap_or(&Value::Null),
                    preset: schedule.get("preset"),
                    sites: None,
                    engine: None,
                    headless: true,
                    time: clock()?,
                })
            })();
            if let Err(error) = result {
                schedules.release(id)?;
                return Err(error);
            }
            schedules.mark_run(id, clock()?)?;
            ran.push(id.to_owned());
        }
        event(
            &format!("Ran {} due schedule(s).", ran.len()),
            super::output::Tone::Green,
        )?;
        Ok(Outcome::success(
            json!({"due":due.len(),"ran":ran.len(),"ranIds":ran,"skipped":skipped}),
        ))
    }
}
