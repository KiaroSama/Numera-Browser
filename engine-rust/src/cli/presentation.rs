use super::{
    application::Application,
    output::Output,
    pool_commands::Pool,
    profile_commands::{Outcome, Profiles},
};
use crate::storage::{config::ConfigStore, pool::ProxyPool, repo::RepoError};
use serde_json::{Map, Value};
use std::io::Write;

impl Application<'_> {
    /// Execute against native services with a single human/structured output owner.
    /// The caller emits the returned outcome once; live attachment is separate.
    pub fn execute_with_output(
        &self,
        args: &Map<String, Value>,
        time: f64,
        confirm: impl FnOnce(usize) -> Result<bool, RepoError>,
        cancelled: impl FnMut() -> bool,
        input: impl FnOnce() -> Result<String, RepoError>,
        output: &mut Output<impl Write, impl Write>,
    ) -> Result<Outcome, RepoError> {
        if self.repo.root != self.root
            || self.sessions.root() != self.root
            || self.sessions.host() != self.host
        {
            return Err(RepoError::Message(
                "Native application services do not belong to the same data root and host".into(),
            ));
        }
        if args.get("detach") == Some(&Value::Bool(true)) {
            return Err(RepoError::Message(
                "Detached execution requires the native job supervisor".into(),
            ));
        }
        let _admission =
            crate::ownership::update_admission::Reservation::for_command(self.root, args)?;
        let command = args
            .get("command")
            .and_then(Value::as_str)
            .ok_or_else(|| RepoError::Message("Native command requires command".into()))?;
        if command == "create" {
            let config = ConfigStore::new(self.root.into());
            return Profiles {
                repo: self.repo,
                config: &config,
                installed: self.installed,
                host: self.host,
            }
            .create_with_output(args, time, output);
        }
        if command == "stop" {
            let config = ConfigStore::new(self.root.into());
            return Profiles {
                repo: self.repo,
                config: &config,
                installed: self.installed,
                host: self.host,
            }
            .stop_with_output(
                self.root,
                args.get("ref").and_then(Value::as_str).unwrap_or(""),
                time,
                output,
            );
        }
        if matches!(command, "apply" | "delete") {
            let config = ConfigStore::new(self.root.into());
            let profiles = Profiles {
                repo: self.repo,
                config: &config,
                installed: self.installed,
                host: self.host,
            };
            return if command == "apply" {
                profiles.apply_with_output(args, self.home, time, confirm, output)
            } else {
                let references = args
                    .get("ref")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect::<Vec<_>>();
                profiles.delete_with_output(
                    &references,
                    args.get("yes") == Some(&Value::Bool(true)),
                    time,
                    confirm,
                    output,
                )
            };
        }
        if command == "attach" {
            let runtime = self.browser.ok_or_else(|| {
                RepoError::Message("Native browser runtime is not configured".into())
            })?;
            let port = args
                .get("port")
                .and_then(Value::as_u64)
                .and_then(|port| u16::try_from(port).ok())
                .filter(|port| *port != 0)
                .ok_or_else(|| {
                    RepoError::Message("DevTools port must be between 1 and 65535".into())
                })?;
            return super::attach_commands::Attach {
                repo: self.repo,
                runtime,
                host: self.host,
            }
            .run_with_output(
                super::attach_commands::Options {
                    reference: args.get("ref").and_then(Value::as_str).unwrap_or(""),
                    engine: args.get("engine").and_then(Value::as_str),
                    port,
                    headless: args.get("headless") == Some(&Value::Bool(true)),
                    time,
                },
                cancelled,
                output,
            );
        }
        if command == "launch" {
            let runtime = self.browser.ok_or_else(|| {
                RepoError::Message("Native browser runtime is not configured".into())
            })?;
            let config = ConfigStore::new(self.root.into());
            return Profiles {
                repo: self.repo,
                config: &config,
                installed: self.installed,
                host: self.host,
            }
            .launch_with_output(
                runtime,
                super::launch_commands::LaunchOptions {
                    reference: args.get("ref").and_then(Value::as_str).unwrap_or(""),
                    engine: args.get("engine").and_then(Value::as_str),
                    headless: args.get("headless") == Some(&Value::Bool(true)),
                    time,
                },
                cancelled,
                output,
            );
        }
        if command == "cookies" {
            let runtime = self.browser.ok_or_else(|| {
                RepoError::Message("Native browser runtime is not configured".into())
            })?;
            return super::cookie_commands::Cookies {
                repo: self.repo,
                sessions: self.sessions,
                runtime,
            }
            .run_with_output(args, time, output);
        }
        if matches!(
            command,
            "tls" | "dns" | "proxy" | "trust" | "align" | "warmup"
        ) {
            return self.execute_browser_with_output(args, time, cancelled, output);
        }
        if command == "ext" && args.get("ext_command").and_then(Value::as_str) == Some("install") {
            return super::extension_commands::install_with_output(
                self.root,
                args,
                crate::network::extension_download::download_sync,
                output,
            );
        }
        if command == "schedule"
            && args.get("schedule_command").and_then(Value::as_str) == Some("run-due")
        {
            let runtime = self.browser.ok_or_else(|| {
                RepoError::Message("Native browser runtime is not configured".into())
            })?;
            let service = super::warmup_commands::Warmup {
                repo: self.repo,
                sessions: self.sessions,
                runtime,
            };
            let mut cancelled = cancelled;
            return service.run_due_with_output(
                time,
                || {
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|time| time.as_secs_f64())
                        .map_err(|error| RepoError::Message(error.to_string()))
                },
                |options| {
                    let outcome = service.run(options, &mut cancelled)?;
                    if outcome.exit != 0 {
                        return Err(RepoError::Message(
                            "Scheduled warmup did not complete".into(),
                        ));
                    }
                    Ok(())
                },
                output,
            );
        }
        let supported = match command {
            "pool" | "jobs" | "engines" | "default-engine" | "config" | "folder" | "vault"
            | "token" => true,
            "schedule" => args.get("schedule_command").and_then(Value::as_str) != Some("run-due"),
            "ext" => args.get("ext_command").and_then(Value::as_str) != Some("install"),
            "list" | "show" | "create" | "regen" | "check" | "edit" | "import" | "restore"
            | "export" | "backup" | "health" | "collisions" | "bulk-create" | "import-from" => true,
            _ => false,
        };
        if !supported {
            return Err(RepoError::Message(format!(
                "Native human output for {command} is not implemented; the original application remains the default"
            )));
        }
        let reference = args.get("ref").and_then(Value::as_str).unwrap_or("");
        let resolved_name = if (command == "ext"
            && matches!(
                args.get("ext_command").and_then(Value::as_str),
                Some("list" | "add" | "remove")
            ))
            || (command == "schedule"
                && args.get("schedule_command").and_then(Value::as_str) == Some("add"))
        {
            self.repo
                .resolve(reference, time)?
                .map(|profile| profile.name)
        } else {
            None
        };
        let outcome = if command == "pool" {
            let config = ConfigStore::new(self.root.into());
            let profiles = Profiles {
                repo: self.repo,
                config: &config,
                installed: self.installed,
                host: self.host,
            };
            let store = ProxyPool::new(self.root.into(), self.password.map(str::to_owned));
            let pool = Pool {
                store: &store,
                profiles: &profiles,
            };
            let ids = args
                .get("id")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect::<Vec<_>>();
            let clock = || {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0.0, |time| time.as_secs_f64())
            };
            match args.get("pool_command").and_then(Value::as_str) {
                Some("test") => pool.test_with_output(
                    &ids,
                    args.get("all") == Some(&Value::Bool(true)),
                    |proxy| {
                        crate::network::proxy_check::check_sync(
                            proxy,
                            std::time::Duration::from_secs(12),
                        )
                    },
                    clock,
                    output,
                )?,
                Some("rotate") => pool.rotate_with_output(
                    &ids,
                    |proxy| {
                        crate::network::proxy_rotate::rotate_sync(
                            proxy,
                            std::time::Duration::from_secs(20),
                        )
                    },
                    clock,
                    output,
                )?,
                Some("assign") => pool.assign_with_output(args, time, output)?,
                _ => self.execute_with_input(args, time, confirm, cancelled, input)?,
            }
        } else {
            self.execute_with_input(args, time, confirm, cancelled, input)?
        };
        let handled = match command {
            "pool" => super::pool_output::render(output, args, &outcome.payload)?,
            "jobs" => super::job_render::render(output, args, &outcome.payload)?,
            "health" | "collisions" | "bulk-create" | "import-from" => {
                super::fleet_output::render(output, command, &outcome.payload)?
            }
            "engines" | "default-engine" | "config" | "folder" | "vault" => {
                super::settings_output::render(output, args, &outcome.payload)?
            }
            "token" | "schedule" => super::automation_output::render(
                output,
                args,
                &outcome.payload,
                time,
                resolved_name.as_ref(),
            )?,
            "ext" if args.get("ext_command").and_then(Value::as_str) != Some("install") => {
                super::extension_output::render(
                    output,
                    args,
                    &outcome.payload,
                    resolved_name.as_ref(),
                    self.home,
                    false,
                )?
            }
            _ => super::profile_output::render(output, command, &outcome.payload)?,
        };
        if !handled {
            return Err(RepoError::Message(format!(
                "Native human output for {command} is not implemented; the original application remains the default"
            )));
        }
        Ok(outcome)
    }
}
