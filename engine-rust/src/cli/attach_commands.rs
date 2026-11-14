use super::profile_commands::Outcome;
use crate::{
    browser::{admission, runtime::Runtime},
    conversion::value_text,
    network::devtools,
    storage::repo::{ProfileRepo, RepoError},
};
use serde_json::{Value, json};
use std::time::Duration;

pub struct Attach<'a> {
    pub repo: &'a ProfileRepo,
    pub runtime: &'a Runtime,
    pub host: &'a str,
}
pub struct Options<'a> {
    pub reference: &'a str,
    pub engine: Option<&'a str>,
    pub port: u16,
    pub headless: bool,
    pub time: f64,
}
pub fn ready(
    output: &mut super::output::Output<impl std::io::Write, impl std::io::Write>,
    info: &Value,
    port: u16,
) -> Result<(), RepoError> {
    let title = info
        .get("Browser")
        .cloned()
        .unwrap_or_else(|| "Browser".into());
    output.say(
        &format!("\n  {} ready — attach your automation:", value_text(&title)),
        Some(super::output::Tone::Bold),
    )?;
    output.say(
        &devtools::snippets(
            port,
            info.get("webSocketDebuggerUrl")
                .and_then(Value::as_str)
                .unwrap_or(""),
        ),
        None,
    )?;
    output.say(
        "Close the browser window (or press Ctrl+C) to end the session.",
        Some(super::output::Tone::Dim),
    )?;
    Ok(())
}
impl Attach<'_> {
    /// Publish the endpoint while its admitted browser is still running.
    pub fn run(
        &self,
        options: Options<'_>,
        mut publish: impl FnMut(&Value) -> Result<(), RepoError>,
        cancelled: impl FnMut() -> bool,
    ) -> Result<Outcome, RepoError> {
        self.run_started(
            options,
            |_| Ok(()),
            |payload, _| publish(payload),
            cancelled,
        )
    }
    pub fn run_with_output(
        &self,
        options: Options<'_>,
        mut cancelled: impl FnMut() -> bool,
        output: &mut super::output::Output<impl std::io::Write, impl std::io::Write>,
    ) -> Result<Outcome, RepoError> {
        let reference = options.reference.to_owned();
        let port = options.port;
        let interrupted = std::cell::Cell::new(false);
        let output = std::cell::RefCell::new(output);
        let outcome = self.run_started(
            options,
            |profile| {
                output.borrow_mut().say(
                    &format!(
                        "Launching '{}' with DevTools on port {port}...",
                        value_text(&profile.name)
                    ),
                    Some(super::output::Tone::Cyan),
                )?;
                Ok(())
            },
            |payload, info| {
                let mut output = output.borrow_mut();
                ready(&mut **output, info, port)?;
                output.emit(payload, 0)?;
                Ok(())
            },
            || {
                let stop = cancelled();
                if stop {
                    interrupted.set(true);
                }
                stop
            },
        );
        let mut output = output.borrow_mut();
        if interrupted.get() {
            output.say("\nClosing.", None)?;
        }
        let outcome = outcome?;
        if outcome.payload.get("error").is_some() {
            output.say(
                &format!("No profile matching '{reference}'."),
                Some(super::output::Tone::Red),
            )?;
        }
        Ok(outcome)
    }
    fn run_started(
        &self,
        options: Options<'_>,
        start: impl FnOnce(&crate::models::Profile) -> Result<(), RepoError>,
        mut publish: impl FnMut(&Value, &Value) -> Result<(), RepoError>,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<Outcome, RepoError> {
        let Options {
            reference,
            engine,
            port,
            headless,
            time,
        } = options;
        let Some(profile) = self.repo.resolve(reference, time)? else {
            return Ok(Outcome::failed(format!(
                "no profile matching '{reference}'"
            )));
        };
        start(&profile)?;
        let (session, mut current) = admission::current(
            &self.repo.root,
            &profile.id,
            &value_text(&profile.name),
            self.host,
            self.repo.password.as_deref(),
            time,
        )
        .map_err(|error| RepoError::Message(error.to_string()))?;
        if let Some(engine) = engine.filter(|engine| !engine.is_empty()) {
            current.engine = engine.into();
        }
        let backend = if current.engine.is_empty() {
            "playwright"
        } else {
            &current.engine
        };
        if !["playwright", "patchright", "cloak"].contains(&backend) {
            return Err(RepoError::Message(format!(
                "The '{backend}' engine has no Chrome DevTools Protocol to attach to.\nUse one of: playwright, patchright, cloak — e.g. numera attach {} --engine cloak",
                value_text(&current.name)
            )));
        }
        if port == 0 {
            return Err(RepoError::Message("DevTools port must be positive".into()));
        }
        let (id, name) = (current.id.clone(), current.name.clone());
        current
            .launch_args
            .push(format!("--remote-debugging-port={port}"));
        let mut adapter = self
            .runtime
            .open_lifetime(session, current, headless, true, time)
            .map_err(|error| RepoError::Message(error.to_string()))?;
        let mut natural_exit = false;
        let result = (|| -> Result<Outcome, RepoError> {
            let info = devtools::info(port, Duration::from_secs(20))?;
            let payload = json!({"ok":true,"id":id,"name":name,"port":port,
                "wsUrl":info.get("webSocketDebuggerUrl").cloned().unwrap_or_else(|| "".into()),
                "browser":info.get("Browser").cloned().unwrap_or_else(|| "".into()),"headless":headless});
            publish(&payload, &info)?;
            loop {
                if cancelled() {
                    return Ok(Outcome::success(payload));
                }
                if let Some(code) = adapter.settled_exit()? {
                    natural_exit = true;
                    return if code == 0 {
                        Ok(Outcome::success(payload))
                    } else {
                        Err(RepoError::Message(format!(
                            "Attached browser exited {code}"
                        )))
                    };
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        })();
        if natural_exit {
            adapter.finish(Duration::from_secs(10))?;
        } else if adapter.close(Duration::from_secs(10))? != Some(0) {
            return Err(RepoError::Message(
                "Attached browser did not close successfully".into(),
            ));
        }
        result
    }
}
