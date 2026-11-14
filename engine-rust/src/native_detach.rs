//! Detached CLI/MCP dispatch shares the durable supervisor, not a capture tree.
use crate::{
    cli::{
        application,
        output::{Output, Tone},
        profile_commands::Outcome,
        reflection,
    },
    job_spawn::{SpawnFailure, SpawnOptions},
    jobs::JobStore,
    native_environment::Environment,
    ownership::capture::Failure,
    runtime_log::RuntimeLog,
    storage::{data_root, repo::ProfileRepo},
};
use serde_json::{Map, Value, json};
use std::{
    io::{self, Write},
    path::Path,
    time::Duration,
};

fn start(
    args: &Map<String, Value>,
    argv: &[String],
    repo: &ProfileRepo,
    root: &Path,
    cancelled: impl FnMut() -> bool,
) -> Result<Value, SpawnFailure> {
    let time = crate::native_entry::now()?;
    let profile = args
        .get("ref")
        .and_then(Value::as_str)
        .map(|reference| repo.resolve(reference, time))
        .transpose()
        .map_err(io::Error::other)?
        .flatten()
        .map(|profile| profile.id);
    JobStore::new(root.to_owned()).spawn_detached_controlled(
        SpawnOptions {
            executable: &std::env::current_exe()?,
            argv,
            command: args.get("command").and_then(Value::as_str).unwrap_or(""),
            profile: profile.as_deref(),
            password: repo.password.as_deref(),
        },
        cancelled,
    )
}

fn settle(failure: &mut SpawnFailure, log: &RuntimeLog) {
    if failure.settle(Duration::from_secs(10)).is_err() {
        log.event(
            "ERROR",
            "entry",
            "Detached startup cleanup remains owned in quarantine",
        );
    }
}

pub(crate) fn execute(
    args: &Map<String, Value>,
    argv: &[String],
    output: &mut Output<impl Write, impl Write>,
    repo: &ProfileRepo,
    root: &Path,
    cancelled: impl FnMut() -> bool,
    log: &RuntimeLog,
) -> io::Result<i32> {
    let value = start(args, argv, repo, root, cancelled).map_err(|mut failure| {
        settle(&mut failure, log);
        failure.into_io_error()
    })?;
    output.say(
        &format!(
            "Started job {}  (pid {}, log {})",
            value["id"].as_str().unwrap_or(""),
            value["pid"],
            value["log"].as_str().unwrap_or("")
        ),
        Some(Tone::Green),
    )?;
    output.say(
        &format!(
            "  poll it with:  numera jobs show {}",
            value["id"].as_str().unwrap_or("")
        ),
        Some(Tone::Dim),
    )?;
    output.emit(&value, 0)
}

/// Only the same native executable takes this path. Foreign commands and
/// foreground calls keep their bounded capture owner and scoped unit receipts.
pub(crate) fn tool(
    root: Option<&Path>,
    password: Option<&str>,
    argv: &[String],
    mut cancelled: impl FnMut() -> bool,
) -> Result<Value, Failure> {
    if cancelled() {
        return Err(
            io::Error::new(io::ErrorKind::ConnectionAborted, "Native command cancelled").into(),
        );
    }
    let mut registry = crate::cli::registry();
    registry.build();
    let mut parse_argv = vec!["numera-native".to_owned()];
    if let Some(root) = root {
        parse_argv.extend(["--data-dir".into(), root.to_string_lossy().into_owned()]);
    }
    parse_argv.extend_from_slice(argv);
    let matches = match crate::cli::parse(parse_argv) {
        Ok(matches) => matches,
        // A parser failure is not JSON command output: preserve the MCP crash
        // shape (isError, no structuredContent/_meta) used by captured commands.
        Err(error) => {
            return crate::mcp::error_payload(json!({
                "error":error.to_string().trim(), "exit":error.exit_code()
            }))
            .map_err(Failure::from);
        }
    };
    let values = reflection::values(&registry, &matches);
    let args = values
        .as_object()
        .ok_or_else(|| io::Error::other("Native CLI values missing"))?;
    let prepared = (|| -> io::Result<_> {
        let mut environment = Environment::discover()?;
        let inherited_root = std::env::var_os("NUMERA_DATA_DIR");
        let root = data_root::resolve(
            args.get("data_dir").and_then(Value::as_str).map(Path::new),
            inherited_root.as_deref().map(Path::new),
            &environment.app,
            &environment.home,
        )?;
        environment.browser.logs = RuntimeLog::directory(&root);
        let admission =
            crate::ownership::update_admission::Reservation::begin(&root, "native-mcp-detach")?;
        let inherited_password = std::env::var("NUMERA_PASSWORD").ok();
        let password = password
            .filter(|value| !value.is_empty())
            .or(inherited_password.as_deref());
        let repo = application::open_store(&root, args, password).map_err(io::Error::other)?;
        Ok((environment, root, repo, admission))
    })();
    let (environment, root, repo, _admission) = match prepared {
        Ok(prepared) => prepared,
        Err(error) => {
            return crate::mcp::tool_result(Outcome::failed(error.to_string()))
                .map_err(Failure::from);
        }
    };
    let log = RuntimeLog::new(&environment.browser.logs, "native-cli");
    log.event("INFO", "entry", "Detached MCP startup");
    let result = match start(args, argv, &repo, &root, &mut cancelled) {
        Ok(value) => crate::mcp::tool_result(Outcome::success(value)).map_err(Failure::from),
        Err(mut failure) => {
            settle(&mut failure, &log);
            let unresolved = failure.retained_record().is_some();
            let error = failure.into_io_error();
            if unresolved || error.kind() == io::ErrorKind::ConnectionAborted || cancelled() {
                Err(error.into())
            } else {
                crate::mcp::tool_result(Outcome::failed(error.to_string())).map_err(Failure::from)
            }
        }
    };
    log.event(
        if result.is_ok() { "INFO" } else { "ERROR" },
        "entry",
        "Detached MCP shutdown",
    );
    result
}
