use super::profile_commands::Outcome;
use crate::{
    browser::{alignment, runtime::Runtime, trust_grades, trust_probe},
    conversion::value_text,
    ownership::registry::Registry,
    storage::repo::{ProfileRepo, RepoError},
};
use serde_json::{Value, json};

pub struct Align<'a> {
    pub repo: &'a ProfileRepo,
    pub sessions: &'a Registry,
    pub runtime: &'a Runtime,
}
impl Align<'_> {
    pub fn run(
        &self,
        reference: &str,
        engine: Option<&str>,
        time: f64,
    ) -> Result<Outcome, RepoError> {
        self.run_started(reference, engine, time, |_| Ok(()))
    }
    pub fn run_with_output(
        &self,
        reference: &str,
        engine: Option<&str>,
        time: f64,
        output: &mut super::output::Output<impl std::io::Write, impl std::io::Write>,
    ) -> Result<Outcome, RepoError> {
        let mut outcome = self.run_started(reference, engine, time, |profile| {
            super::probe_output::start(output, "align", &profile.name, None)?;
            Ok(())
        })?;
        super::probe_output::render(output, "align", reference, &outcome.payload)?;
        if output.json_mode() && outcome.payload.get("after").is_some() {
            outcome.exit = 0;
        }
        Ok(outcome)
    }
    fn run_started(
        &self,
        reference: &str,
        engine: Option<&str>,
        time: f64,
        start: impl FnOnce(&crate::models::Profile) -> Result<(), RepoError>,
    ) -> Result<Outcome, RepoError> {
        let Some(profile) = self.repo.resolve(reference, time)? else {
            return Ok(Outcome::failed(format!(
                "no profile matching '{reference}'"
            )));
        };
        start(&profile)?;
        let admitted = std::cell::RefCell::new(None);
        let candidates = alignment::candidates()?;
        let seen = self.sessions.with_finite_browser(
            &profile.id,
            &value_text(&profile.name),
            |session, mut current| {
                current.startup_urls.clear();
                if let Some(engine) = engine.filter(|value| !value.is_empty()) {
                    current.engine = engine.into();
                }
                *admitted.borrow_mut() = Some(current.clone());
                self.runtime.open(session, current, true, time)
            },
            |adapter| {
                let current = admitted.borrow();
                let current = current
                    .as_ref()
                    .ok_or_else(|| std::io::Error::other("Admitted alignment profile missing"))?;
                trust_probe::observe_fonts(adapter, &current.fingerprint, &candidates)
            },
        )?;
        let current = admitted
            .into_inner()
            .ok_or_else(|| RepoError::Message("Admitted alignment profile missing".into()))?;
        let before = trust_grades::grade(&current.fingerprint, &seen, &current.engine)?;
        let (aligned, changed) = alignment::rewrite(&current.fingerprint, &seen)?;
        let mut saved = self.repo.patch_many(
            std::slice::from_ref(&current.id),
            |latest| {
                alignment::patch(&mut latest.fingerprint, &aligned)?;
                Ok(true)
            },
            Default::default(),
            "reject",
            time,
        )?;
        let fp = saved.pop().map_or(aligned, |profile| profile.fingerprint);
        let mut observed = seen;
        observed
            .as_object_mut()
            .ok_or_else(|| RepoError::Message("Alignment observation is not an object".into()))?
            .insert("userAgent".into(), fp.user_agent.clone().into());
        let after = trust_grades::grade(&fp, &observed, &current.engine)?;
        let summary = |report: &Value| json!({"score":report["score"],"grade":report["grade"],"passed":report["passed"],"total":report["total"]});
        Ok(Outcome {
            exit: i32::from(after["score"] != 100),
            payload: json!({"before":summary(&before),"after":summary(&after),"changed":changed,
            "checks":after["checks"],"os":fp.os,"fingerprint":{
                "os":fp.os,"userAgent":fp.user_agent,"browserVersion":fp.chrome_version,"platform":fp.platform,
                "cores":fp.hardware_concurrency,"memory":fp.device_memory,"screen":format!("{}x{}",fp.screen_width,fp.screen_height),
                "webglVendor":fp.webgl_vendor,"webglRenderer":fp.webgl_renderer,"cameras":fp.cameras,
                "microphones":fp.microphones,"languages":fp.languages,"locale":fp.locale,"timezone":fp.timezone,
            }}),
        })
    }
}
