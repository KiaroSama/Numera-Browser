use super::profile_commands::Outcome;
use crate::{
    browser::{cookie_normalize, cookies, runtime::Runtime},
    conversion::value_text,
    cookie_export::{self, missing},
    ownership::registry::Registry,
    storage::repo::{ProfileRepo, RepoError},
};
use serde_json::{Value, json};
use std::path::Path;

pub struct Cookies<'a> {
    pub repo: &'a ProfileRepo,
    pub sessions: &'a Registry,
    pub runtime: &'a Runtime,
}
impl Cookies<'_> {
    pub fn run_with_output(
        &self,
        args: &serde_json::Map<String, serde_json::Value>,
        time: f64,
        output: &mut super::output::Output<impl std::io::Write, impl std::io::Write>,
    ) -> Result<Outcome, RepoError> {
        let reference = args
            .get("ref")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let engine = args.get("engine").and_then(serde_json::Value::as_str);
        let name = self
            .repo
            .resolve(reference, time)?
            .map(|profile| profile.name);
        let file = || {
            args.get("file")
                .and_then(serde_json::Value::as_str)
                .map(Path::new)
                .ok_or_else(|| RepoError::Message("Cookie file missing".into()))
        };
        let outcome = match args
            .get("cookies_command")
            .and_then(serde_json::Value::as_str)
        {
            Some("list") => self.list(reference, engine, time)?,
            Some("export") => cookie_export::run(
                self,
                reference,
                args.get("out")
                    .and_then(Value::as_str)
                    .or_else(|| args.get("file").and_then(Value::as_str))
                    .map(Path::new),
                args.get("format").and_then(serde_json::Value::as_str),
                engine,
                time,
                |profile| {
                    output.say(
                        &format!("Reading cookies from '{}'...", value_text(&profile.name)),
                        Some(super::output::Tone::Cyan),
                    )
                },
            )?,
            Some("import") => self.import_started(
                reference,
                file()?,
                args.get("clear") == Some(&serde_json::Value::Bool(true)),
                engine,
                time,
                |profile, count| {
                    output.say(
                        &format!(
                            "Importing {count} cookie(s) into '{}'...",
                            value_text(&profile.name)
                        ),
                        Some(super::output::Tone::Cyan),
                    )
                },
            )?,
            _ => return Err(RepoError::Message("Unknown cookie command".into())),
        };
        super::cookie_output::render(output, args, &outcome.payload, name.as_ref())?;
        Ok(outcome)
    }
    fn run<T>(
        &self,
        reference: &str,
        engine: Option<&str>,
        time: f64,
        operation: impl FnOnce(&mut crate::browser::adapter::Adapter) -> std::io::Result<T>,
    ) -> Result<Option<T>, RepoError> {
        let Some(profile) = self.repo.resolve(reference, time)? else {
            return Ok(None);
        };
        cookie_export::with_profile_browser(self, &profile, engine, time, operation).map(Some)
    }
    pub fn list(
        &self,
        reference: &str,
        engine: Option<&str>,
        time: f64,
    ) -> Result<Outcome, RepoError> {
        let Some(rows) = self.run(reference, engine, time, |adapter| {
            cookies::read(adapter).and_then(|rows| cookie_normalize::normalize_many(&rows))
        })?
        else {
            return Ok(missing(reference));
        };
        let mut domains = serde_json::Map::<String, serde_json::Value>::new();
        for row in &rows {
            let domain = row["domain"]
                .as_str()
                .ok_or_else(|| RepoError::Message("Normalized cookie domain missing".into()))?;
            let count = domains
                .get(domain)
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0);
            domains.insert(domain.into(), (count + 1).into());
        }
        Ok(Outcome::success(
            json!({"total":rows.len(),"domains":domains}),
        ))
    }
    pub fn export(
        &self,
        reference: &str,
        file: &Path,
        format: Option<&str>,
        engine: Option<&str>,
        time: f64,
    ) -> Result<Outcome, RepoError> {
        cookie_export::run(
            self,
            reference,
            Some(file),
            format,
            engine,
            time,
            |_| Ok(()),
        )
    }
    pub fn import(
        &self,
        reference: &str,
        file: &Path,
        clear: bool,
        engine: Option<&str>,
        time: f64,
    ) -> Result<Outcome, RepoError> {
        self.import_started(reference, file, clear, engine, time, |_, _| Ok(()))
    }
    fn import_started(
        &self,
        reference: &str,
        file: &Path,
        clear: bool,
        engine: Option<&str>,
        time: f64,
        start: impl FnOnce(&crate::models::Profile, usize) -> std::io::Result<()>,
    ) -> Result<Outcome, RepoError> {
        // The source resolves the profile before touching the supplied file.
        let Some(profile) = self.repo.resolve(reference, time)? else {
            return Ok(missing(reference));
        };
        let rows = cookie_normalize::read_file(file)?;
        if rows.is_empty() {
            return Ok(Outcome::failed(format!(
                "no usable cookies found in {}",
                file.display()
            )));
        }
        start(&profile, rows.len())?;
        cookies::preflight(&rows)?;
        let count = cookie_export::with_profile_browser(self, &profile, engine, time, |adapter| {
            cookies::write_open(adapter, &rows, clear)
        })?;
        Ok(Outcome::success(
            json!({"ok":true,"imported":count,"cleared":clear}),
        ))
    }
}
