use super::profile_commands::Outcome;
use crate::{
    browser::{cookie_format, cookie_normalize, cookies, runtime::Runtime},
    conversion::value_text,
    ownership::registry::Registry,
    storage::repo::{ProfileRepo, RepoError},
};
use serde_json::json;
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
            Some("export") => self.export_started(
                reference,
                file()?,
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
        self.run_started(reference, engine, time, |_| Ok(()), operation)
    }
    fn run_started<T>(
        &self,
        reference: &str,
        engine: Option<&str>,
        time: f64,
        start: impl FnOnce(&crate::models::Profile) -> std::io::Result<()>,
        operation: impl FnOnce(&mut crate::browser::adapter::Adapter) -> std::io::Result<T>,
    ) -> Result<Option<T>, RepoError> {
        let Some(profile) = self.repo.resolve(reference, time)? else {
            return Ok(None);
        };
        start(&profile)?;
        self.sessions
            .with_finite_browser(
                &profile.id,
                &value_text(&profile.name),
                |session, mut current| {
                    current.startup_urls.clear();
                    if let Some(engine) = engine.filter(|v| !v.is_empty()) {
                        current.engine = engine.into();
                    }
                    self.runtime.open(session, current, true, time)
                },
                operation,
            )
            .map(Some)
            .map_err(RepoError::from)
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
        self.export_started(reference, file, format, engine, time, |_| Ok(()))
    }
    fn export_started(
        &self,
        reference: &str,
        file: &Path,
        format: Option<&str>,
        engine: Option<&str>,
        time: f64,
        start: impl FnOnce(&crate::models::Profile) -> std::io::Result<()>,
    ) -> Result<Outcome, RepoError> {
        let Some(rows) = self.run_started(reference, engine, time, start, |adapter| {
            cookies::read(adapter).and_then(|rows| cookie_normalize::normalize_many(&rows))
        })?
        else {
            return Ok(missing(reference));
        };
        let format = format.filter(|v| !v.is_empty()).unwrap_or_else(|| {
            if file.to_string_lossy().to_lowercase().ends_with(".txt") {
                "netscape"
            } else {
                "json"
            }
        });
        let output = cookie_format::dumps(&rows, format)?;
        crate::storage::atomic::write_bytes(file, output.as_bytes())?;
        Ok(Outcome::success(
            json!({"ok":true,"count":rows.len(),"file":file,"format":format}),
        ))
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
        let count = self.sessions.with_finite_browser(
            &profile.id,
            &value_text(&profile.name),
            |session, mut current| {
                current.startup_urls.clear();
                if let Some(engine) = engine.filter(|v| !v.is_empty()) {
                    current.engine = engine.into();
                }
                self.runtime.open(session, current, true, time)
            },
            |adapter| cookies::write_open(adapter, &rows, clear),
        )?;
        Ok(Outcome::success(
            json!({"ok":true,"imported":count,"cleared":clear}),
        ))
    }
}
fn missing(reference: &str) -> Outcome {
    Outcome::failed(format!("no profile matching '{reference}'"))
}
