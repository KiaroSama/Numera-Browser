//! Native command orchestration. The selected product remains separately gated.
use crate::{
    cli::{
        self,
        application::{self, Application},
        output::{Output, Tone},
    },
    native_environment::Environment,
    ownership::registry::Registry,
    runtime_log::RuntimeLog,
    storage::data_root,
};
use serde_json::{Map, Value};
use std::{
    io::{self, BufRead, IsTerminal, Read, Write},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub fn now() -> io::Result<f64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs_f64())
        .map_err(io::Error::other)
}

pub fn run(argv: Vec<String>) -> i32 {
    if let Some(record) = std::env::var_os("NUMERA_NATIVE_SUPERVISE") {
        let result = crate::job_supervisor::run_internal(Path::new(&record));
        let pending = crate::ownership::capture::retry_retained(Duration::from_secs(10));
        if !pending.is_empty() {
            eprintln!("Native job supervisor cleanup remains incomplete");
            return 1;
        }
        return result.unwrap_or_else(|error| {
            eprintln!("Native job supervisor failed: {error}");
            1
        });
    }
    if let Some(record) = std::env::var_os("NUMERA_NATIVE_JOB_GATE")
        && let Err(error) = crate::job_supervisor::await_gate(Path::new(&record))
    {
        eprintln!("Native job admission failed: {error}");
        return 1;
    }
    let mut command = cli::registry();
    command.build();
    let matches = match command.clone().try_get_matches_from(
        std::iter::once("numera-native".to_owned()).chain(argv.iter().cloned()),
    ) {
        Ok(matches) => matches,
        Err(error) => {
            let code = error.exit_code();
            let _ = error.print();
            return code;
        }
    };
    let values = cli::reflection::values(&command, &matches);
    let Some(args) = values.as_object() else {
        return 1;
    };
    let json = args.get("json") == Some(&Value::Bool(true));
    let mut output = Output::new(
        crate::native_stdio::TextWriter::new(io::stdout()),
        crate::native_stdio::TextWriter::new(io::stderr()),
        json,
        io::stdout().is_terminal(),
    );
    let result = execute(args, &argv, &mut output);
    let mut pending = crate::job_spawn::reap_completed()
        .err()
        .map(|error| error.to_string())
        .into_iter()
        .collect::<Vec<_>>();
    pending.extend(crate::ownership::capture::retry_retained(
        Duration::from_secs(10),
    ));
    pending.extend(crate::ownership::capture_units::retry_retained(
        Duration::from_secs(10),
    ));
    pending.extend(crate::job_supervisor::retry_retained(Duration::from_secs(
        10,
    )));
    pending.extend(crate::ownership::registry::retry_retained_sessions(
        Duration::from_secs(10),
    ));
    pending.extend(crate::http::server::retry_retained(Duration::from_secs(10)));
    if !pending.is_empty() {
        return output
            .error("Native cleanup remains owned and incomplete; migration acceptance is blocked")
            .unwrap_or(1);
    }
    match result {
        Ok(code) => code,
        Err(error) => output.error(&error.to_string()).unwrap_or(1),
    }
}

fn execute(
    args: &Map<String, Value>,
    argv: &[String],
    output: &mut Output<impl Write, impl Write>,
) -> io::Result<i32> {
    let environment = Environment::discover()?;
    let log = RuntimeLog::new(&environment.browser.logs, "native-cli");
    log.event("INFO", "entry", "Startup");
    let cancelled = Arc::new(AtomicBool::new(false));
    let signal = cancelled.clone();
    ctrlc::try_set_handler(move || signal.store(true, Ordering::Release))
        .map_err(io::Error::other)?;
    let explicit = args.get("data_dir").and_then(Value::as_str).map(Path::new);
    let inherited = std::env::var_os("NUMERA_DATA_DIR");
    let root = data_root::resolve(
        explicit,
        inherited.as_deref().map(Path::new),
        &environment.app,
        &environment.home,
    )?;
    let operation = args.get("command").and_then(Value::as_str).unwrap_or("");
    if operation == "serve" && args.get("detach") != Some(&Value::Bool(true)) {
        let _admission =
            crate::ownership::update_admission::Reservation::begin(&root, "native-server-prepare")?;
        let host = args
            .get("host")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|host| !host.is_empty())
            .unwrap_or("127.0.0.1");
        if let Some(refusal) = crate::http::bind_policy::refusal(
            host,
            &crate::http::tokens::TokenStore::new(root.clone()),
            &std::env::var("NUMERA_API_TOKEN").unwrap_or_default(),
            &std::env::var("NUMERA_ALLOW_INSECURE_BIND").unwrap_or_default(),
        ) {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, refusal));
        }
        if let Some(summary) = crate::storage::first_run::run_if_needed(
            &root,
            &[
                environment.home.join(".numera"),
                environment.home.join(".persona"),
            ],
        )? {
            if !summary.copied.is_empty() {
                output.say(
                    &format!(
                        "  Imported {} profile(s) from an older install",
                        summary.profiles
                    ),
                    None,
                )?;
            }
            for _ in summary.failed {
                log.event(
                    "WARNING",
                    "first-run",
                    "Import item failed; retry remains possible",
                );
            }
        }
    }
    dispatch(args, argv, output, environment, &root, cancelled, &log)
}

/// Application entry seam with explicit environment/root/cancellation for isolated acceptance.
pub fn dispatch(
    args: &Map<String, Value>,
    argv: &[String],
    output: &mut Output<impl Write, impl Write>,
    mut environment: Environment,
    root: &Path,
    cancelled: Arc<AtomicBool>,
    log: &RuntimeLog,
) -> io::Result<i32> {
    let password = std::env::var("NUMERA_PASSWORD").ok();
    let root = root.to_owned();
    let operation = args.get("command").and_then(Value::as_str).unwrap_or("");
    let mut admission = crate::ownership::update_admission::Reservation::for_command(&root, args)?;
    let prepared = (|| -> io::Result<_> {
        // Resume requires both keys before recovery. Other vault prompts belong
        // after ordinary store-open, so a damaged journal refuses first.
        let opened = if args.get("vault_command").and_then(Value::as_str) == Some("resume") {
            None
        } else {
            Some(
                application::open_store(&root, args, password.as_deref())
                    .map_err(io::Error::other)?,
            )
        };
        let args = crate::native_password::prepare(args, cancelled.clone())?;
        let repo = match opened {
            Some(mut repo) => {
                if let Some(password) = args
                    .get("password")
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                {
                    repo.password = Some(password.into());
                }
                repo
            }
            None => application::open_store(&root, &args, password.as_deref())
                .map_err(io::Error::other)?,
        };
        Ok((args, repo))
    })();
    let (prepared_args, repo) = match prepared {
        Ok(prepared) => prepared,
        Err(_) if cancelled.load(Ordering::Acquire) => {
            output.say("\nAborted.", None)?;
            return Ok(130);
        }
        Err(error) => return Err(error),
    };
    let args = &prepared_args;
    let selected_password = repo.password.clone();
    // Long-lived transports reserve individual requests/jobs, not their idle loop.
    if operation == "mcp" {
        if let Some(admission) = &mut admission {
            admission.release()?;
        }
        admission = None;
    }
    let result = if args.get("detach") == Some(&Value::Bool(true)) {
        match crate::native_detach::execute(
            args,
            argv,
            output,
            &repo,
            &root,
            || cancelled.load(Ordering::Acquire),
            log,
        ) {
            Err(_) if cancelled.load(Ordering::Acquire) => {
                output.say("\nAborted.", None)?;
                Ok(130)
            }
            result => result,
        }
    } else if operation == "mcp" {
        let executable = std::env::current_exe()?;
        let mut input = io::BufReader::new(crate::native_input::Input::new(
            io::stdin(),
            cancelled.clone(),
        ));
        match crate::mcp_command::serve_controlled(
            &executable,
            Some(&root),
            selected_password.as_deref(),
            &mut input,
            &mut crate::native_stdio::TextWriter::new(io::stdout().lock()),
            || cancelled.load(Ordering::Acquire),
        ) {
            Ok(()) => Ok(0),
            Err(mut failure) => {
                if failure.settle(Duration::from_secs(10)).is_err() {
                    log.event(
                        "ERROR",
                        "entry",
                        "Captured command cleanup remains owned in quarantine",
                    );
                }
                if cancelled.load(Ordering::Acquire) {
                    Ok(130)
                } else {
                    Err(io::Error::other(failure.to_string()))
                }
            }
        }
    } else {
        let metadata_needed = matches!(
            operation,
            "create"
                | "engines"
                | "default-engine"
                | "config"
                | "serve"
                | "launch"
                | "attach"
                | "cookies"
                | "tls"
                | "dns"
                | "proxy"
                | "trust"
                | "align"
                | "warmup"
        ) || (operation == "schedule"
            && args.get("schedule_command").and_then(Value::as_str) == Some("run-due"));
        let addons_needed = matches!(
            operation,
            "serve"
                | "launch"
                | "attach"
                | "cookies"
                | "tls"
                | "dns"
                | "proxy"
                | "trust"
                | "align"
                | "warmup"
        ) || (operation == "schedule"
            && args.get("schedule_command").and_then(Value::as_str) == Some("run-due"));
        let installed = match if metadata_needed {
            environment.metadata(addons_needed)
        } else {
            Ok(Default::default())
        } {
            Ok(value) => value,
            Err(mut failure) => {
                if failure.settle(Duration::from_secs(10)).is_err() {
                    if let Some(admission) = &mut admission {
                        admission.retain();
                    }
                    log.event(
                        "ERROR",
                        "entry",
                        "Captured command cleanup remains owned in quarantine",
                    );
                }
                return Err(io::Error::other(failure.to_string()));
            }
        };
        if operation == "serve" {
            if let Some(admission) = &mut admission {
                admission.release()?;
            }
            drop(admission.take());
            serve(
                &root,
                args,
                &environment,
                installed,
                selected_password,
                cancelled.clone(),
                output,
            )
        } else {
            let sessions = Registry::new(
                root.clone(),
                environment.host.clone(),
                selected_password.clone(),
            )?;
            let application = Application {
                root: &root,
                home: &environment.home,
                repo: &repo,
                installed: &installed,
                host: &environment.host,
                password: selected_password.as_deref(),
                environment_token: std::env::var("NUMERA_API_TOKEN").is_ok_and(|v| !v.is_empty()),
                sessions: &sessions,
                browser: Some(&environment.browser),
            };
            let outcome = application.execute_with_output(
                args,
                now()?,
                |count| {
                    if args.get("json") == Some(&Value::Bool(true)) {
                        return Err(crate::storage::repo::RepoError::Message(
                            "this needs confirmation — pass -y (a --json run never prompts)".into(),
                        ));
                    }
                    let prompt = if operation == "apply" {
                        "Continue? (y/N) ".to_owned()
                    } else {
                        let what = if count == 1 {
                            let reference = args
                                .get("ref")
                                .and_then(Value::as_array)
                                .and_then(|refs| refs.first())
                                .and_then(Value::as_str)
                                .ok_or_else(|| io::Error::other("Deletion reference missing"))?;
                            let profile = repo.resolve(reference, now()?)?.ok_or_else(|| {
                                io::Error::other("Deletion target disappeared before confirmation")
                            })?;
                            format!(
                                "'{}' [{}]",
                                crate::conversion::value_text(&profile.name),
                                profile.id
                            )
                        } else {
                            format!("{count} profiles")
                        };
                        format!(
                            "Delete {what}? This removes {} too. (y/N) ",
                            if count == 1 {
                                "its session"
                            } else {
                                "their sessions"
                            }
                        )
                    };
                    print!("{prompt}");
                    io::stdout().flush()?;
                    let mut answer = String::new();
                    let mut input = io::BufReader::with_capacity(
                        1,
                        crate::native_input::Input::new(io::stdin(), cancelled.clone()),
                    );
                    let bytes = input.by_ref().take(65537).read_line(&mut answer)?;
                    if bytes == 0 {
                        return Err(crate::storage::repo::RepoError::Message(
                            "nothing to read the confirmation from — pass -y".into(),
                        ));
                    }
                    if bytes > 65536 {
                        return Err(io::Error::other("Confirmation input exceeds limit").into());
                    }
                    Ok(answer.trim().eq_ignore_ascii_case("y"))
                },
                || cancelled.load(Ordering::Acquire),
                || {
                    let mut input = String::new();
                    crate::native_input::Input::new(io::stdin(), cancelled.clone())
                        .take(16_777_217)
                        .read_to_string(&mut input)?;
                    if input.len() > 16_777_216 {
                        return Err(io::Error::other("Input exceeds limit").into());
                    }
                    Ok(input)
                },
                output,
            );
            let mut pending = sessions.shutdown(Duration::from_secs(10));
            pending.extend(
                crate::ownership::registry::retry_retained_background(Duration::from_secs(10))
                    .into_iter()
                    .map(|error| (String::new(), error)),
            );
            if !pending.is_empty() {
                if let Some(admission) = &mut admission {
                    admission.retain();
                }
                log.event(
                    "ERROR",
                    "entry",
                    "Session cleanup remains owned in quarantine",
                );
                return Err(io::Error::other(
                    "Native session cleanup remains incomplete",
                ));
            }
            if cancelled.load(Ordering::Acquire) {
                output.say("\nAborted.", None)?;
                return Ok(130);
            }
            outcome.map_err(io::Error::other).and_then(|value| {
                if output.emitted() {
                    Ok(value.exit)
                } else {
                    output.emit(&value.payload, value.exit)
                }
            })
        }
    };
    log.event(
        if result.is_ok() { "INFO" } else { "ERROR" },
        "entry",
        "Shutdown",
    );
    result
}

fn serve(
    root: &Path,
    args: &Map<String, Value>,
    environment: &Environment,
    installed: std::collections::BTreeMap<String, bool>,
    password: Option<String>,
    cancelled: Arc<AtomicBool>,
    output: &mut Output<impl Write, impl Write>,
) -> io::Result<i32> {
    let host = args
        .get("host")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|host| !host.is_empty())
        .unwrap_or("127.0.0.1");
    let port = args
        .get("port")
        .and_then(Value::as_u64)
        .and_then(|v| u16::try_from(v).ok())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "Server port must be between 0 and 65535",
            )
        })?;
    let address = (host, port)
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| io::Error::other("Server address unavailable"))?;
    let mut server = crate::http::server::Server::start(crate::http::server::Options {
        root: root.into(),
        address,
        token: std::env::var("NUMERA_API_TOKEN").unwrap_or_default(),
        origins: std::env::var("NUMERA_ALLOWED_ORIGINS").unwrap_or_default(),
        insecure_override: std::env::var("NUMERA_ALLOW_INSECURE_BIND").unwrap_or_default(),
        password,
        host: environment.host.clone(),
        installed,
        browser: environment.browser.clone(),
        dashboard: crate::http::dashboard::discover(&environment.app),
    })?;
    let parent = std::env::var("NUMERA_PARENT_PID")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|pid| *pid > 0)
        .map(|pid| crate::ownership::identity::started_at(pid).map(|token| (pid, token)))
        .transpose();
    let (parent, mut failure) = match parent {
        Ok(parent) => (parent, server.wait_ready(Duration::from_secs(10)).err()),
        Err(error) => (None, Some(error)),
    };
    if failure.is_none() {
        failure = output
            .say(
                &format!("Numera listening on http://{}", server.address()),
                Some(Tone::Cyan),
            )
            .err();
    }
    while failure.is_none() && !cancelled.load(Ordering::Acquire) {
        if let Some((pid, token)) = parent {
            match crate::ownership::identity::alive(pid, token) {
                Ok(true) => {}
                Ok(false) => break,
                Err(error) => {
                    failure = Some(error);
                    break;
                }
            }
        }
        if let Err(error) = server.wait_ready(Duration::ZERO) {
            failure = Some(error);
            break;
        }
        std::thread::park_timeout(Duration::from_millis(25));
    }
    if let Err(error) = server.stop(Duration::from_secs(10)) {
        failure.get_or_insert(error);
    }
    failure.map_or(Ok(0), Err)
}
use std::net::ToSocketAddrs;
