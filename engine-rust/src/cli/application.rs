//! Shared native dispatch for implemented services; no application forwarding.
use super::{
    cookie_commands::Cookies,
    extension_commands, job_commands,
    pool_commands::Pool,
    profile_commands::{Outcome, Profiles},
    settings_commands::Settings,
    vault_commands,
};
use crate::{
    browser::runtime::Runtime,
    ownership::registry::Registry,
    storage::{
        config::ConfigStore,
        pool::ProxyPool,
        repo::{ProfileRepo, RepoError},
    },
};
use serde_json::{Map, Value};
use std::{collections::BTreeMap, path::Path};

pub struct Application<'a> {
    pub root: &'a Path,
    pub home: &'a Path,
    pub repo: &'a ProfileRepo,
    pub installed: &'a BTreeMap<String, bool>,
    pub host: &'a str,
    pub password: Option<&'a str>,
    pub environment_token: bool,
    pub sessions: &'a Registry,
    pub browser: Option<&'a Runtime>,
}
/// Open only the selected data root; vault recovery precedes every service.
/// The executable supplies the inherited unlock without recording it in argv.
pub fn open_store(
    root: &Path,
    args: &Map<String, Value>,
    inherited_password: Option<&str>,
) -> Result<ProfileRepo, RepoError> {
    let password = text(args, "password")
        .filter(|value| !value.is_empty())
        .or_else(|| inherited_password.filter(|value| !value.is_empty()));
    if text(args, "vault_command") == Some("resume") {
        // Source resume asks for the old key when the global option is absent;
        // unlike ordinary store opening, it does not use the inherited key here.
        let old = text(args, "password").filter(|value| !value.is_empty()).ok_or_else(|| RepoError::Message(
            "this needs a master password — pass --password or set NUMERA_PASSWORD (a --json run never prompts)".into()))?;
        let new = text(args, "new_password").filter(|value| !value.is_empty())
            .ok_or_else(|| RepoError::Message(
                "this needs the password the rotation was moving to - pass --new-password (a --json run never prompts)".into()))?;
        ProfileRepo::open(root.into(), Some(new.into()), Some(old))
    } else {
        ProfileRepo::open(root.into(), password.map(str::to_owned), None)
    }
}

fn text<'a>(args: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str)
}
fn required<'a>(args: &'a Map<String, Value>, key: &str) -> Result<&'a str, RepoError> {
    text(args, key).ok_or_else(|| RepoError::Message(format!("Native command requires {key}")))
}
fn strings(args: &Map<String, Value>, key: &str) -> Vec<String> {
    args.get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}
impl Application<'_> {
    pub fn execute_browser_with_output(
        &self,
        args: &Map<String, Value>,
        time: f64,
        cancelled: impl FnMut() -> bool,
        output: &mut super::output::Output<impl std::io::Write, impl std::io::Write>,
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
        let operation = required(args, "command")?;
        if !matches!(
            operation,
            "tls" | "dns" | "proxy" | "trust" | "align" | "warmup"
        ) {
            return Err(RepoError::Message(
                "Native browser output requires a browser command".into(),
            ));
        }
        if operation == "proxy" && required(args, "proxy_command")? != "test" {
            return Err(RepoError::Message("Unknown browser proxy command".into()));
        }
        let runtime = self
            .browser
            .ok_or_else(|| RepoError::Message("Native browser runtime is not configured".into()))?;
        let reference = text(args, "ref").unwrap_or("");
        if operation == "warmup" {
            super::warmup_commands::Warmup {
                repo: self.repo,
                sessions: self.sessions,
                runtime,
            }
            .run_with_output(
                super::warmup_commands::Options {
                    reference,
                    minutes: args.get("minutes").unwrap_or(&Value::Null),
                    preset: args.get("preset"),
                    sites: args.get("site"),
                    engine: text(args, "engine"),
                    headless: args.get("headless") == Some(&Value::Bool(true)),
                    time,
                },
                cancelled,
                output,
            )
        } else if operation == "align" {
            super::align_commands::Align {
                repo: self.repo,
                sessions: self.sessions,
                runtime,
            }
            .run_with_output(reference, text(args, "engine"), time, output)
        } else {
            super::probe_commands::Probes {
                repo: self.repo,
                sessions: self.sessions,
                runtime,
            }
            .run_with_output(reference, text(args, "engine"), operation, time, output)
        }
    }
    pub fn execute(
        &self,
        args: &Map<String, Value>,
        time: f64,
        confirm: impl FnOnce(usize) -> Result<bool, RepoError>,
    ) -> Result<Outcome, RepoError> {
        self.execute_controlled(args, time, confirm, || false)
    }
    pub fn execute_controlled(
        &self,
        args: &Map<String, Value>,
        time: f64,
        confirm: impl FnOnce(usize) -> Result<bool, RepoError>,
        cancelled: impl FnMut() -> bool,
    ) -> Result<Outcome, RepoError> {
        self.execute_with_input(args, time, confirm, cancelled, || {
            Err(RepoError::Message(
                "Pool stdin input must be supplied by the native input owner".into(),
            ))
        })
    }
    pub fn execute_with_input(
        &self,
        args: &Map<String, Value>,
        time: f64,
        confirm: impl FnOnce(usize) -> Result<bool, RepoError>,
        cancelled: impl FnMut() -> bool,
        input: impl FnOnce() -> Result<String, RepoError>,
    ) -> Result<Outcome, RepoError> {
        self.execute_streaming(
            args,
            time,
            confirm,
            cancelled,
            (input, |_| {
                Err(RepoError::Message(
                    "Live command output must be supplied by the native output owner".into(),
                ))
            }),
        )
    }
    pub fn execute_streaming(
        &self,
        args: &Map<String, Value>,
        time: f64,
        confirm: impl FnOnce(usize) -> Result<bool, RepoError>,
        cancelled: impl FnMut() -> bool,
        io: (
            impl FnOnce() -> Result<String, RepoError>,
            impl FnMut(&Value) -> Result<(), RepoError>,
        ),
    ) -> Result<Outcome, RepoError> {
        let (input, publish) = io;
        if args.get("detach") == Some(&Value::Bool(true)) {
            return Err(RepoError::Message(
                "Detached execution requires the native job supervisor".into(),
            ));
        }
        if self.repo.root != self.root
            || self.sessions.root() != self.root
            || self.sessions.host() != self.host
        {
            return Err(RepoError::Message(
                "Native application services do not belong to the same data root and host".into(),
            ));
        }
        let _admission =
            crate::ownership::update_admission::Reservation::for_command(self.root, args)?;
        let config = ConfigStore::new(self.root.into());
        let profiles = Profiles {
            repo: self.repo,
            config: &config,
            installed: self.installed,
            host: self.host,
        };
        let settings = Settings {
            root: self.root,
            repo: self.repo,
            installed: self.installed,
            environment_token: self.environment_token,
        };
        let reference = text(args, "ref").unwrap_or("");
        match required(args, "command")? {
            "create" => profiles.create(args, time),
            "list" => profiles.list(text(args, "tag"), time),
            "show" => profiles.show(reference, time),
            "check" => profiles.check(reference, time),
            "edit" => profiles.edit(reference, args, time),
            "regen" => profiles.regen(reference, text(args, "os"), text(args, "locale"), time),
            "collisions" => profiles.collisions(time),
            "health" => profiles.health(self.root, time),
            "delete" => profiles.delete(
                &strings(args, "ref"),
                args.get("yes") == Some(&Value::Bool(true)),
                time,
                confirm,
            ),
            "apply" => profiles.apply(args, self.home, time, confirm),
            "bulk-create" => profiles.bulk_create(args, time),
            "export" | "backup" => profiles.export(
                reference,
                Path::new(required(args, "file")?),
                text(args, "command") == Some("backup"),
                time,
            ),
            "import" => profiles.import(Path::new(required(args, "file")?), time),
            "import-from" => profiles.import_from(Path::new(required(args, "file")?), time),
            "restore" => profiles.restore(
                Path::new(required(args, "file")?),
                args.get("new_id") == Some(&Value::Bool(true)),
                time,
            ),
            "launch" => profiles.launch(
                self.browser.ok_or_else(|| {
                    RepoError::Message("Native SDK runtime is not configured".into())
                })?,
                super::launch_commands::LaunchOptions {
                    reference,
                    engine: text(args, "engine"),
                    headless: args.get("headless") == Some(&Value::Bool(true)),
                    time,
                },
                cancelled,
            ),
            "attach" => {
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
                super::attach_commands::Attach {
                    repo: self.repo,
                    runtime,
                    host: self.host,
                }
                .run(
                    super::attach_commands::Options {
                        reference,
                        engine: text(args, "engine"),
                        port,
                        headless: args.get("headless") == Some(&Value::Bool(true)),
                        time,
                    },
                    publish,
                    cancelled,
                )
            }
            "warmup" => super::warmup_commands::Warmup {
                repo: self.repo,
                sessions: self.sessions,
                runtime: self.browser.ok_or_else(|| {
                    RepoError::Message("Native browser runtime is not configured".into())
                })?,
            }
            .run(
                super::warmup_commands::Options {
                    reference,
                    minutes: args.get("minutes").unwrap_or(&Value::Null),
                    preset: args.get("preset"),
                    sites: args.get("site"),
                    engine: text(args, "engine"),
                    headless: args.get("headless") == Some(&Value::Bool(true)),
                    time,
                },
                cancelled,
            ),
            "stop" => profiles.stop(self.root, reference, time),
            "jobs" => job_commands::run(self.root, self.host, args, time),
            "engines" => Ok(settings.engines()),
            "default-engine" => settings.default_engine(text(args, "name")),
            "config" => settings.config(args, self.home),
            "folder" => settings.folder(required(args, "folder_command")?, args),
            "token" => settings.tokens(required(args, "token_command")?, args, time),
            "schedule" if text(args, "schedule_command") == Some("run-due") => {
                let service = super::warmup_commands::Warmup {
                    repo: self.repo,
                    sessions: self.sessions,
                    runtime: self.browser.ok_or_else(|| {
                        RepoError::Message("Native browser runtime is not configured".into())
                    })?,
                };
                let mut cancelled = cancelled;
                service.run_due(
                    time,
                    || {
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|value| value.as_secs_f64())
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
                )
            }
            "schedule" => settings.schedule(required(args, "schedule_command")?, args, time),
            "vault" => vault_commands::run(
                self.root,
                required(args, "vault_command")?,
                self.password,
                text(args, "new_password"),
            ),
            "pool" => {
                let store = ProxyPool::new(self.root.into(), self.password.map(str::to_owned));
                let service = Pool {
                    store: &store,
                    profiles: &profiles,
                };
                let ids = strings(args, "id");
                match required(args, "pool_command")? {
                    "list" => service.list(),
                    "add" => {
                        let file = required(args, "source")?;
                        let text = if file == "-" {
                            input()?
                        } else {
                            std::fs::read_to_string(file)?
                        };
                        service.add(&text)
                    }
                    "remove" => service.remove(&ids),
                    "edit" => service.edit(required(args, "id")?, args),
                    "archive" => {
                        service.archive(&ids, args.get("restore") == Some(&Value::Bool(true)))
                    }
                    "export" => service.export(&ids, text(args, "file").map(Path::new)),
                    "assign" => service.assign(args, time),
                    "test" => service.test(
                        &ids,
                        args.get("all") == Some(&Value::Bool(true)),
                        |proxy| {
                            crate::network::proxy_check::check_sync(
                                proxy,
                                std::time::Duration::from_secs(12),
                            )
                        },
                        || {
                            std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map_or(0.0, |now| now.as_secs_f64())
                        },
                    ),
                    "rotate" => service.rotate(
                        &ids,
                        |proxy| {
                            crate::network::proxy_rotate::rotate_sync(
                                proxy,
                                std::time::Duration::from_secs(20),
                            )
                        },
                        || {
                            std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map_or(0.0, |now| now.as_secs_f64())
                        },
                    ),
                    _ => Err(RepoError::Message("Unknown proxy command".into())),
                }
            }
            "ext" => match required(args, "ext_command")? {
                "list" | "add" | "remove" => profiles.extensions(
                    required(args, "ext_command")?,
                    reference,
                    text(args, "path"),
                    self.home,
                    time,
                ),
                verb if verb.starts_with("default") => {
                    extension_commands::defaults(self.root, verb, text(args, "path"), self.home)
                }
                "installed" => extension_commands::installed(self.root),
                "uninstall" => Ok(extension_commands::uninstall(
                    self.root,
                    required(args, "url")?,
                )),
                "install" => extension_commands::install(
                    self.root,
                    required(args, "url")?,
                    args.get("force") == Some(&Value::Bool(true)),
                    args.get("no_default") == Some(&Value::Bool(true)),
                    crate::network::extension_download::download_sync,
                ),
                _ => Err(RepoError::Message("Unknown extension command".into())),
            },
            operation @ ("tls" | "dns" | "proxy" | "trust") => {
                if operation == "proxy" && required(args, "proxy_command")? != "test" {
                    return Err(RepoError::Message("Unknown browser proxy command".into()));
                }
                let runtime = self.browser.ok_or_else(|| {
                    RepoError::Message("Native browser runtime is not configured".into())
                })?;
                let mut outcome = super::probe_commands::Probes {
                    repo: self.repo,
                    sessions: self.sessions,
                    runtime,
                }
                .run(reference, text(args, "engine"), operation, time)?;
                if operation == "trust"
                    && args.get("json") == Some(&Value::Bool(true))
                    && outcome.payload.get("score").is_some()
                {
                    outcome.exit = 0;
                }
                Ok(outcome)
            }
            "align" => {
                let runtime = self.browser.ok_or_else(|| {
                    RepoError::Message("Native browser runtime is not configured".into())
                })?;
                let mut outcome = super::align_commands::Align {
                    repo: self.repo,
                    sessions: self.sessions,
                    runtime,
                }
                .run(reference, text(args, "engine"), time)?;
                if args.get("json") == Some(&Value::Bool(true))
                    && outcome.payload.get("after").is_some()
                {
                    outcome.exit = 0;
                }
                Ok(outcome)
            }
            "cookies" => {
                let runtime = self.browser.ok_or_else(|| {
                    RepoError::Message("Native SDK runtime is not configured".into())
                })?;
                let cookies = Cookies {
                    repo: self.repo,
                    sessions: self.sessions,
                    runtime,
                };
                match required(args, "cookies_command")? {
                    "list" => cookies.list(reference, text(args, "engine"), time),
                    "export" => cookies.export(
                        reference,
                        Path::new(required(args, "file")?),
                        text(args, "format"),
                        text(args, "engine"),
                        time,
                    ),
                    "import" => cookies.import(
                        reference,
                        Path::new(required(args, "file")?),
                        args.get("clear") == Some(&Value::Bool(true)),
                        text(args, "engine"),
                        time,
                    ),
                    _ => Err(RepoError::Message("Unknown cookie command".into())),
                }
            }
            command => Err(RepoError::Message(format!(
                "Native service for {command} is not implemented; the original application remains the default"
            ))),
        }
    }
}
