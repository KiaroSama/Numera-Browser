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
    let (path, argv) = match reflection::argv_with_path(name, arguments) {
        Ok(command) => command,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return mcp::error_result(format!("no such tool '{name}'")).map_err(Failure::from);
        }
        Err(error) => return Err(error.into()),
    };
    let mut command = prepare(executable, root, password, argv);
    let output = match capture::run(
        &mut command,
        Limits {
            wall: Duration::from_secs(mcp::CALL_TIMEOUT_SECONDS),
            idle: Duration::from_secs(mcp::CALL_TIMEOUT_SECONDS),
            bytes: mcp::MAX_FRAME,
            cleanup: Duration::from_secs(10),
        },
    ) {
        Ok(output) => output,
        Err(error) if error.retained_record().is_some() => return Err(error),
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
    mcp::serve(input, output, |message| {
        mcp::handle_tools(message, |name, arguments| {
            call(executable, root, password, name, arguments)
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
