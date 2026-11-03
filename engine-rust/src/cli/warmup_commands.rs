use super::profile_commands::Outcome;
use crate::{
    browser::{runtime::Runtime, warmup, warmup_policy},
    conversion::value_text,
    ownership::registry::Registry,
    storage::repo::{ProfileRepo, RepoError},
};
use serde_json::Value;

pub struct Warmup<'a> {
    pub repo: &'a ProfileRepo,
    pub sessions: &'a Registry,
    pub runtime: &'a Runtime,
}
pub struct Options<'a> {
    pub reference: &'a str,
    pub minutes: &'a Value,
    pub preset: Option<&'a Value>,
    pub sites: Option<&'a Value>,
    pub engine: Option<&'a str>,
    pub headless: bool,
    pub time: f64,
}
impl Warmup<'_> {
    pub fn run(
        &self,
        options: Options<'_>,
        cancelled: impl FnMut() -> bool,
    ) -> Result<Outcome, RepoError> {
        let mut control = warmup::Live::new(cancelled)?;
        self.run_controlled(options, &mut control, |_| Ok(()))
    }
    pub fn run_with_output(
        &self,
        options: Options<'_>,
        cancelled: impl FnMut() -> bool,
        output: &mut super::output::Output<impl std::io::Write, impl std::io::Write>,
    ) -> Result<Outcome, RepoError> {
        let mut control = warmup::Live::new(cancelled)?;
        self.run_output_controlled(options, &mut control, output)
    }
    pub fn run_output_controlled(
        &self,
        options: Options<'_>,
        control: &mut impl warmup::Control,
        output: &mut super::output::Output<impl std::io::Write, impl std::io::Write>,
    ) -> Result<Outcome, RepoError> {
        let output = std::cell::RefCell::new(output);
        let reference = options.reference.to_owned();
        let outcome = self.run_started(
            options,
            control,
            |profile, options| {
                let preset = options
                    .preset
                    .filter(|value| crate::conversion::truth(value));
                let where_text = if !options.sites.is_some_and(crate::conversion::truth) {
                    preset
                        .map(|value| format!(" ({})", value_text(value)))
                        .unwrap_or_default()
                } else {
                    String::new()
                };
                output.borrow_mut().say(
                    &format!(
                        "Warming up '{}'{} for {} minute(s)...",
                        value_text(&profile.name),
                        where_text,
                        value_text(options.minutes)
                    ),
                    Some(super::output::Tone::Cyan),
                )?;
                Ok(())
            },
            |event| output.borrow_mut().say(event, None),
        )?;
        let mut output = output.borrow_mut();
        if outcome.payload.get("error").is_some() {
            output.say(
                &format!("No profile matching '{reference}'."),
                Some(super::output::Tone::Red),
            )?;
        } else {
            let errors = &outcome.payload["errors"];
            let note = if crate::conversion::truth(errors) {
                format!(", {} unreachable", value_text(errors))
            } else {
                String::new()
            };
            output.say(
                &format!(
                    "Done — visited {} page(s){note}.",
                    value_text(&outcome.payload["pages"])
                ),
                Some(super::output::Tone::Green),
            )?;
        }
        Ok(outcome)
    }
    pub fn run_controlled(
        &self,
        options: Options<'_>,
        control: &mut impl warmup::Control,
        event: impl FnMut(&str) -> std::io::Result<()>,
    ) -> Result<Outcome, RepoError> {
        self.run_started(options, control, |_, _| Ok(()), event)
    }
    fn run_started(
        &self,
        options: Options<'_>,
        control: &mut impl warmup::Control,
        start: impl FnOnce(&crate::models::Profile, &Options<'_>) -> Result<(), RepoError>,
        event: impl FnMut(&str) -> std::io::Result<()>,
    ) -> Result<Outcome, RepoError> {
        let Some(profile) = self.repo.resolve(options.reference, options.time)? else {
            return Ok(Outcome::failed(format!(
                "no profile matching '{}'",
                options.reference
            )));
        };
        start(&profile, &options)?;
        let minutes = warmup_policy::validate(options.minutes, options.preset, options.sites)?;
        let urls = warmup_policy::selected(options.preset, options.sites);
        let preset = if options.sites.is_some_and(crate::conversion::truth) {
            "custom".into()
        } else {
            options
                .preset
                .and_then(Value::as_str)
                .filter(|text| !text.is_empty())
                .unwrap_or("general")
                .to_lowercase()
        };
        let started = control.elapsed();
        let result = self.sessions.with_finite_browser(
            &profile.id,
            &value_text(&profile.name),
            |session, mut current| {
                current.startup_urls.clear();
                if let Some(engine) = options.engine.filter(|value| !value.is_empty()) {
                    current.engine = engine.into();
                }
                self.runtime
                    .open(session, current, options.headless, options.time)
            },
            |adapter| warmup::run(adapter, minutes, urls, &preset, started, control, event),
        )?;
        let mut payload = result
            .as_object()
            .cloned()
            .ok_or_else(|| RepoError::Message("Warmup summary missing".into()))?;
        payload.insert("ok".into(), true.into());
        payload.insert("id".into(), profile.id.into());
        payload.insert("name".into(), profile.name);
        Ok(Outcome::success(Value::Object(payload)))
    }
}
