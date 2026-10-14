//! Execute MCP tools in the actual native executable, with owned bounded pipes.
use crate::{
    cli::reflection,
    mcp,
    ownership::capture::{self, Failure, Limits},
};
use serde_json::{Map, Value, json};
use std::{io, path::Path, process::Command, time::Duration};

fn prepare(
    executable: &Path,
    root: Option<&Path>,
    password: Option<&str>,
    argv: Vec<String>,
) -> Command {
    let mut command = Command::new(executable);
    if let Some(root) = root {
        command.arg("--data-dir").arg(root);
    }
    command
        .args(argv)
        .env_remove("NUMERA_NATIVE_SUPERVISE")
        .env_remove("NUMERA_NATIVE_JOB_GATE")
        .env_remove("NUMERA_JOB_FILE")
        .env_remove("NUMERA_TREE")
        .env_remove("NUMERA_READY_FILE");
    if let Some(password) = password.filter(|value| !value.is_empty()) {
        command.env("NUMERA_PASSWORD", password);
    }
    command
}

pub fn call(
    executable: &Path,
    root: Option<&Path>,
    password: Option<&str>,
    name: &str,
    arguments: &Map<String, Value>,
) -> Result<Value, Failure> {
    call_controlled(executable, root, password, name, arguments, || false)
}

pub fn call_controlled(
    executable: &Path,
    root: Option<&Path>,
    password: Option<&str>,
    name: &str,
    arguments: &Map<String, Value>,
    mut cancelled: impl FnMut() -> bool,
) -> Result<Value, Failure> {
    let (path, argv) = match reflection::argv_with_path(name, arguments) {
        Ok(command) => command,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return mcp::error_result(format!("no such tool '{name}'")).map_err(Failure::from);
        }
        Err(error) => return Err(error.into()),
    };
    if arguments
        .get("detach")
        .is_some_and(crate::conversion::truth)
        && executable.canonicalize()? == std::env::current_exe()?.canonicalize()?
    {
        if cancelled() {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "Native command cancelled",
            )
            .into());
        }
        return crate::native_detach::tool(root, password, &argv, &mut cancelled);
    }
    let mut command = prepare(executable, root, password, argv);
    let mut units = root
        .map(|root| crate::ownership::capture_units::Units::new(root, &mut command))
        .transpose()?;
    let captured = capture::run_controlled(
        &mut command,
        Limits {
            wall: Duration::from_secs(mcp::CALL_TIMEOUT_SECONDS),
            idle: Duration::from_secs(mcp::CALL_TIMEOUT_SECONDS),
            bytes: mcp::MAX_FRAME,
            cleanup: Duration::from_secs(10),
        },
        cancelled,
    );
    // The leader's tree may be empty while separately-owned SDK trees remain.
    // Settle their durable receipts before returning a cancellation or result.
    if let Some(units) = &mut units {
        units.settle(Duration::from_secs(10))?;
    }
    let output = match captured {
        Ok(output) => output,
        Err(error)
            if error.retained_record().is_some()
                || error.kind() == io::ErrorKind::ConnectionAborted =>
        {
            return Err(error);
        }
        Err(error) if error.kind() == io::ErrorKind::TimedOut => {
            return mcp::error_result(format!(
                "`numera {}` did not finish within {}s — call it again with detach: true",
                path.join(" "),
                mcp::CALL_TIMEOUT_SECONDS
            ))
            .map_err(Failure::from);
        }
        Err(error) => return mcp::error_result(error.to_string()).map_err(Failure::from),
    };
    let stdout = String::from_utf8(output.stdout).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Native command stdout is not UTF8",
        )
    })?;
    let stderr = String::from_utf8(output.stderr).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Native command stderr is not UTF8",
        )
    })?;
    match serde_json::from_str(&stdout) {
        Ok(payload) => mcp::tool_result(crate::cli::profile_commands::Outcome {
            payload,
            exit: output.exit,
        })
        .map_err(Failure::from),
        Err(_) => {
            let message = if stderr.trim().is_empty() {
                stdout.trim()
            } else {
                stderr.trim()
            };
            mcp::error_payload(json!({"error":if message.is_empty(){"command failed"}else{message},"exit":output.exit})).map_err(Failure::from)
        }
    }
}

/// Use only with a complete native executable; no Python application fallback.
/// An unproved child cleanup aborts the loop while preserving its owner in Failure.
pub fn serve(
    executable: &Path,
    root: Option<&Path>,
    password: Option<&str>,
    input: &mut impl io::BufRead,
    output: &mut impl io::Write,
) -> Result<(), Failure> {
    serve_controlled(executable, root, password, input, output, || false)
}

pub fn serve_controlled(
    executable: &Path,
    root: Option<&Path>,
    password: Option<&str>,
    input: &mut impl io::BufRead,
    output: &mut impl io::Write,
    mut cancelled: impl FnMut() -> bool,
) -> Result<(), Failure> {
    mcp::serve(input, output, |message| {
        crate::job_spawn::reap_completed()?;
        if cancelled() {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "Native command cancelled",
            )
            .into());
        }
        mcp::handle_tools(message, |name, arguments| {
            call_controlled(executable, root, password, name, arguments, &mut cancelled)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn timeout_labels_preserve_actual_command_paths() -> io::Result<()> {
        for (tool, expected) in [
            ("bulk_create", "bulk-create"),
            ("import_from", "import-from"),
            ("cookies_export", "cookies export"),
        ] {
            let (path, argv) = reflection::argv_with_path(tool, &Map::new())?;
            assert_eq!(path.join(" "), expected);
            assert_eq!(&argv[..path.len()], path.as_slice());
        }
        Ok(())
    }

    #[test]
    fn stdio_protocol_and_unknown_tools_never_start_the_executable() -> Result<(), Failure> {
        let input = b"{\"id\":1,\"method\":\"ping\"}\n{\"id\":2,\"method\":\"tools/call\",\"params\":{\"name\":\"not_a_tool\"}}\n";
        let mut output = Vec::new();
        serve(
            Path::new("nonexistent-native-fixture"),
            None,
            None,
            &mut io::Cursor::new(input),
            &mut output,
        )?;
        let responses: Vec<Value> = output
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .map(|line| serde_json::from_slice(line).expect("Protocol response"))
            .collect();
        assert_eq!(responses.len(), 2);
        assert_eq!(responses[0], json!({"jsonrpc":"2.0","id":1,"result":{}}));
        assert_eq!(responses[1]["result"]["isError"], true);
        assert_eq!(
            responses[1]["result"]["content"][0]["text"],
            "{\"error\": \"no such tool 'not_a_tool'\"}"
        );
        assert!(responses[1]["result"].get("_meta").is_none());
        Ok(())
    }

    #[test]
    fn same_native_detach_preserves_parser_failure_and_prebirth_cancellation() -> Result<(), Failure>
    {
        let executable = std::env::current_exe()?;
        let arguments = json!({"detach":true});
        let arguments = arguments.as_object().expect("Fixture arguments");
        let result = call_controlled(&executable, None, None, "show", arguments, || false)?;
        assert_eq!(result["isError"], true);
        assert!(result.get("structuredContent").is_none());
        assert!(result.get("_meta").is_none());
        let text = result["content"][0]["text"]
            .as_str()
            .expect("Parser failure");
        let payload: Value = serde_json::from_str(text).map_err(io::Error::other)?;
        assert_eq!(payload["exit"], 2);
        assert!(
            payload["error"]
                .as_str()
                .expect("Parser message")
                .contains("<ref>")
        );
        let error = call_controlled(&executable, None, None, "list", arguments, || true)
            .expect_err("Cancelled detached call started");
        assert_eq!(error.kind(), io::ErrorKind::ConnectionAborted);
        assert!(error.retained_record().is_none());
        Ok(())
    }

    #[test]
    fn unlock_inherits_without_an_explicit_nonempty_override() {
        for password in [None, Some("")] {
            let command = prepare(
                Path::new("native-fixture"),
                None,
                password,
                vec!["list".into(), "--json".into()],
            );
            assert!(
                !command.get_envs().any(|(key, _)| key == "NUMERA_PASSWORD"),
                "Inherited unlock was overridden"
            );
        }
        let command = prepare(
            Path::new("native-fixture"),
            None,
            Some("synthetic-fixture"),
            vec!["list".into(), "--json".into()],
        );
        assert!(
            command
                .get_envs()
                .any(|(key, value)| key == "NUMERA_PASSWORD" && value.is_some())
        );
        assert!(
            !command.get_args().any(|arg| arg == "synthetic-fixture"),
            "Unlock entered public argv"
        );
    }
}
