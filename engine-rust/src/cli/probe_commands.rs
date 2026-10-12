use super::profile_commands::Outcome;
use crate::{
    browser::{probes, runtime::Runtime},
    conversion::value_text,
    ownership::registry::Registry,
    storage::repo::{ProfileRepo, RepoError},
};

pub struct Probes<'a> {
    pub repo: &'a ProfileRepo,
    pub sessions: &'a Registry,
    pub runtime: &'a Runtime,
}
impl Probes<'_> {
    pub fn run(
        &self,
        reference: &str,
        engine: Option<&str>,
        operation: &str,
        now: f64,
    ) -> Result<Outcome, RepoError> {
        self.run_started(reference, engine, operation, now, |_| Ok(()))
    }
    pub fn run_with_output(
        &self,
        reference: &str,
        engine: Option<&str>,
        operation: &str,
        now: f64,
        output: &mut super::output::Output<impl std::io::Write, impl std::io::Write>,
    ) -> Result<Outcome, RepoError> {
        let mut outcome = self.run_started(reference, engine, operation, now, |profile| {
            super::probe_output::start(
                output,
                operation,
                &profile.name,
                profile.proxy.as_ref().map(|proxy| proxy.server.as_str()),
            )?;
            Ok(())
        })?;
        super::probe_output::render(output, operation, reference, &outcome.payload)?;
        if operation == "trust" && output.json_mode() && outcome.payload.get("score").is_some() {
            outcome.exit = 0;
        }
        Ok(outcome)
    }
    fn run_started(
        &self,
        reference: &str,
        engine: Option<&str>,
        operation: &str,
        now: f64,
        start: impl FnOnce(&crate::models::Profile) -> Result<(), RepoError>,
    ) -> Result<Outcome, RepoError> {
        if !matches!(operation, "dns" | "tls" | "proxy" | "trust") {
            return Err(RepoError::Message("Unknown browser probe".into()));
        }
        let Some(profile) = self.repo.resolve(reference, now)? else {
            return Ok(Outcome::failed(format!(
                "no profile matching '{reference}'"
            )));
        };
        start(&profile)?;
        let country = std::cell::RefCell::new(None);
        let admitted = std::cell::RefCell::new(None);
        let started = std::time::Instant::now();
        let mut result = self.sessions.with_finite_browser(
            &profile.id,
            &value_text(&profile.name),
            |session, mut current| {
                current.startup_urls.clear();
                if let Some(engine) = engine.filter(|engine| !engine.is_empty()) {
                    current.engine = engine.into();
                }
                *country.borrow_mut() = current
                    .proxy
                    .as_ref()
                    .and_then(|proxy| proxy.country.clone());
                *admitted.borrow_mut() = Some(current.clone());
                self.runtime.open(session, current, true, now)
            },
            |adapter| match operation {
                "dns" => probes::dns(adapter, country.borrow().as_deref()),
                "proxy" => {
                    crate::browser::network_probe::read(adapter, country.borrow().as_deref())
                }
                "trust" => {
                    let current = admitted.borrow();
                    let current = current
                        .as_ref()
                        .ok_or_else(|| std::io::Error::other("Admitted trust profile missing"))?;
                    crate::browser::trust_probe::observe(adapter, &current.fingerprint)
                }
                _ => probes::tls(adapter),
            },
        )?;
        if operation == "proxy" {
            let current = admitted.borrow();
            let current = current
                .as_ref()
                .ok_or_else(|| RepoError::Message("Admitted probe profile missing".into()))?;
            let latency = u64::try_from(started.elapsed().as_millis()).map_err(|_| {
                RepoError::Message("Probe elapsed time exceeds numeric range".into())
            })?;
            result = if result.get("error").is_some() {
                serde_json::json!({"ok":false,"error":result["error"],
                    "proxy":current.proxy.as_ref().map(|proxy| proxy.server.as_str()),"checks":[]})
            } else {
                crate::browser::network_grades::proxy(
                    &current,
                    &result["info"],
                    &result["leaks"],
                    &result["dns"],
                    &result["tls"],
                    latency,
                )?
            };
        }
        if operation == "trust" {
            let current = admitted.borrow();
            let current = current
                .as_ref()
                .ok_or_else(|| RepoError::Message("Admitted trust profile missing".into()))?;
            result = crate::browser::trust_grades::grade(
                &current.fingerprint,
                &result,
                &current.engine,
            )?;
        }
        Ok(Outcome {
            exit: if operation == "trust" {
                i32::from(result["score"] != 100)
            } else {
                i32::from(result["ok"] != true)
            },
            payload: result,
        })
    }
}
