use super::profile_commands::{Outcome, Profiles};
use crate::{
    browser::{admission, runtime::Runtime},
    conversion::value_text,
    identity,
    storage::repo::RepoError,
};
use serde_json::json;
use std::{thread, time::Duration};

pub struct LaunchOptions<'a> {
    pub reference: &'a str,
    pub engine: Option<&'a str>,
    pub headless: bool,
    pub time: f64,
}
impl Profiles<'_> {
    pub fn launch(
        &self,
        runtime: &Runtime,
        options: LaunchOptions<'_>,
        cancelled: impl FnMut() -> bool,
    ) -> Result<Outcome, RepoError> {
        self.launch_started(runtime, options, cancelled, |_, _, _| Ok(()))
    }
    pub fn launch_with_output(
        &self,
        runtime: &Runtime,
        options: LaunchOptions<'_>,
        cancelled: impl FnMut() -> bool,
        output: &mut super::output::Output<impl std::io::Write, impl std::io::Write>,
    ) -> Result<Outcome, RepoError> {
        let reference = options.reference.to_owned();
        let outcome =
            self.launch_started(runtime, options, cancelled, |profile, backend, warnings| {
                if !warnings.is_empty() {
                    output.say(
                        "Warning: fingerprint has consistency issues:",
                        Some(super::output::Tone::Yellow),
                    )?;
                    for warning in warnings {
                        output.say(&format!("  - {warning}"), None)?;
                    }
                }
                output.say(
                    &format!("Launching '{}' via {backend}...", value_text(&profile.name)),
                    Some(super::output::Tone::Cyan),
                )?;
                Ok(())
            })?;
        if outcome.payload.get("error").is_some() {
            output.say(
                &format!("No profile matching '{reference}'."),
                Some(super::output::Tone::Red),
            )?;
        }
        Ok(outcome)
    }
    fn launch_started(
        &self,
        runtime: &Runtime,
        options: LaunchOptions<'_>,
        mut cancelled: impl FnMut() -> bool,
        start: impl FnOnce(&crate::models::Profile, &str, &[String]) -> Result<(), RepoError>,
    ) -> Result<Outcome, RepoError> {
        let LaunchOptions {
            reference,
            engine,
            headless,
            time,
        } = options;
        let Some(candidate) = self.repo.resolve(reference, time)? else {
            return Ok(Outcome::failed(format!(
                "no profile matching '{reference}'"
            )));
        };
        let (session, mut current) = admission::current(
            &self.repo.root,
            &candidate.id,
            &value_text(&candidate.name),
            self.host,
            self.repo.password.as_deref(),
            time,
        )
        .map_err(|error| RepoError::Message(error.to_string()))?;
        if let Some(engine) = engine.filter(|v| !v.is_empty()) {
            current.engine = engine.into();
        }
        let backend = if current.engine.is_empty() {
            "playwright"
        } else {
            &current.engine
        }
        .to_owned();
        let warnings = identity::validate(&current.fingerprint).map_err(std::io::Error::other)?;
        start(&current, &backend, &warnings)?;
        let first_page = !(backend == "camoufox" && headless);
        let result = json!({"ok":true,"id":current.id,"name":current.name,"engine":backend,"headless":headless,"warnings":warnings});
        let mut adapter = runtime
            .open_lifetime(session, current, headless, first_page, time)
            .map_err(|error| RepoError::Message(error.to_string()))?;
        if !first_page {
            let code = adapter.close(Duration::from_secs(10))?;
            if code != Some(0) {
                return Err(RepoError::Message(
                    "Browser launch did not close successfully".into(),
                ));
            }
        } else {
            let mut interrupted = false;
            let wait = (|| -> Result<(), RepoError> {
                loop {
                    if cancelled() {
                        interrupted = true;
                        return Ok(());
                    }
                    if let Some(code) = adapter.settled_exit()? {
                        if code != 0 {
                            return Err(RepoError::Message(format!(
                                "Browser launch exited {code}"
                            )));
                        }
                        return Ok(());
                    }
                    // A native child exit/first-page event drives completion;
                    // the cadence only permits the executable's cancellation check.
                    thread::sleep(Duration::from_millis(20));
                }
            })();
            if interrupted {
                if adapter.close(Duration::from_secs(10))? != Some(0) {
                    return Err(RepoError::Message(
                        "Cancelled browser did not close successfully".into(),
                    ));
                }
            } else {
                adapter.finish(Duration::from_secs(10))?;
            }
            wait?;
        }
        Ok(Outcome::success(result))
    }
}
